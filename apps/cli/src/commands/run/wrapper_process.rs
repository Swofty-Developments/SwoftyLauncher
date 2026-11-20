use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use swofty_core::game_folder::busy::Busy;
use swofty_core::install::Source;
use swofty_core::platform::process::flag;
use swofty_core::session::LaunchMode;
use swofty_core::store::Store;
use swofty_core::{GAME_EXE, STEAM_APP_ID, platform::steam};

use super::failure::start;
use crate::console::output::say;
use crate::stores::steam::{steam_command, steam_launch_args};

pub(crate) const DIRECT_START_TIMEOUT: Duration = Duration::from_secs(600);
pub(crate) const STEAM_START_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const WRAPPER_LOG: &str = "state/wrapper-run.log";
const TAIL_LINES: usize = 12;

pub(crate) struct WrapperProcess {
    pub(crate) child: Option<Child>,
    pub(crate) log: PathBuf,
    pub(crate) timeout: Duration,
}

impl WrapperProcess {
    pub(crate) fn exited(&mut self) -> Option<ExitStatus> {
        self.child.as_mut()?.try_wait().ok().flatten()
    }

    /// The wrapper's error starts at its last `swofty: ` line, and an error
    /// over several lines leads with what failed: the reason runs from there
    /// to the end (else it is the last line), and the lines before it are
    /// echoed.
    pub(crate) fn failure(&self, status: ExitStatus, when: &str) -> anyhow::Error {
        let mut lines = tail(&self.log, TAIL_LINES);
        let from = lines
            .iter()
            .rposition(|l| l.starts_with("swofty: "))
            .unwrap_or(lines.len().saturating_sub(1));
        let reason = lines.split_off(from).join("\n");
        for l in &lines {
            say("wrapper", l);
        }
        if reason.is_empty() {
            return anyhow::anyhow!(
                "the wrapper exited ({status}) {when} without output; see {}",
                self.log.display()
            );
        }
        let reason = reason.strip_prefix("swofty: ").unwrap_or(&reason);
        anyhow::anyhow!("the wrapper exited ({status}) {when}: {reason}")
    }
}

/// How long the wrapper has gone without a sign of progress: its log
/// growing, or the heartbeat of a build it runs in `state/build.json`. An
/// install of a large profile takes minutes, and one step can run for more
/// than ten.
pub(crate) struct Silence {
    len: u64,
    beat: Option<SystemTime>,
    since: Instant,
}

impl Silence {
    pub(crate) fn new() -> Self {
        Self {
            len: 0,
            beat: None,
            since: Instant::now(),
        }
    }

    pub(crate) fn of(&mut self, log: &Path, data: &Path) -> Duration {
        let len = std::fs::metadata(log).map_or(0, |m| m.len());
        let beat = std::fs::metadata(Busy::path(data))
            .and_then(|m| m.modified())
            .ok();
        if len != self.len || (beat.is_some() && beat != self.beat && Busy::live(data).is_some()) {
            self.since = Instant::now();
        }
        self.len = len;
        self.beat = beat;
        self.since.elapsed()
    }
}

pub(crate) fn tail(path: &Path, n: usize) -> Vec<String> {
    let text = std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let lines: Vec<String> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

pub(crate) fn spawn_logged(c: &mut Command, log: &Path) -> Result<Child> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::File::create(log).with_context(|| log.display().to_string())?;
    start(c.stdin(Stdio::null()).stdout(out.try_clone()?).stderr(out))
}

/// The Steam command a direct launch hands the wrapper; `extra` goes to the
/// game after the target, where Steam puts a launch option's arguments.
pub(crate) fn wrapper_command(
    proton: Option<&Path>,
    game: &Path,
    mode: LaunchMode,
    extra: &[String],
) -> Vec<OsString> {
    let mut out: Vec<OsString> = vec!["--".into()];
    if let Some(p) = proton {
        out.push(p.join("proton").into());
        out.push("waitforexitandrun".into());
    }
    out.push(game.join(GAME_EXE).into());
    out.extend(mode.args().iter().map(OsString::from));
    out.extend(extra.iter().map(OsString::from));
    out
}

pub(crate) fn start_wrapper(
    store: &Store,
    game: &Path,
    direct: bool,
    mode: LaunchMode,
    extra: &[String],
) -> Result<WrapperProcess> {
    let install = steam::find_game();
    let wrapped = install
        .as_ref()
        .filter(|g| swofty_core::game_folder::copies::same_dir(&g.dir, game))
        .map(|g| {
            steam::launch_options(&g.steam_root)
                .iter()
                .any(|(_, o)| steam::wrapper_installed(o))
        })
        .unwrap_or(false);
    if crate::stores::ea::store_start(Source::detect(game)) == crate::stores::ea::StoreStart::EaApp {
        let mut c = Command::new(std::env::current_exe()?);
        c.arg(flag("data-dir", &store.root))
            .arg("run")
            .arg(flag("game", game))
            .arg("--via-ea")
            .args(wrapper_command(None, game, mode, extra));
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut c, 0);
        let log = store.root.join(WRAPPER_LOG);
        let child = spawn_logged(c.current_dir(game), &log)?;
        say(
            "info",
            &format!(
                "launching F1 25 through the EA app; waiting for the wrapper, output in {}",
                log.display()
            ),
        );
        return Ok(WrapperProcess {
            child: Some(child),
            log,
            timeout: DIRECT_START_TIMEOUT,
        });
    }
    if cfg!(windows) || direct || !wrapped {
        let me = std::env::current_exe()?;
        let mut c = Command::new(&me);
        c.arg(flag("data-dir", &store.root))
            .arg("run")
            .arg(flag("game", game));
        if let Some(g) = install.as_ref().filter(|_| !cfg!(windows)) {
            let tool = steam::compat_tool(&g.steam_root)
                .context("Steam has no Proton build set for F1 25 and no default; pick one in the game's Properties > Compatibility")?;
            let search = steam::find_proton(&g.steam_root, Some(&g.library), &tool);
            let proton = search.found.with_context(|| {
                let looked: Vec<String> = search.searched.iter().map(|p| p.display().to_string()).collect();
                format!(
                    "could not find the Proton build Steam uses for F1 25 ({tool}); looked in {}",
                    looked.join(", ")
                )
            })?;
            let runtime =
                [&g.library, &g.steam_root].map(|l| l.join("steamapps/common/SteamLinuxRuntime_sniper"));
            let runtime = runtime.iter().find(|p| p.is_dir()).unwrap_or(&runtime[0]);
            c.env(
                "STEAM_COMPAT_TOOL_PATHS",
                std::env::join_paths([&proton, runtime])?,
            )
            .env("STEAM_COMPAT_DATA_PATH", g.compatdata.clone().unwrap_or_default())
            .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &g.steam_root)
            .env("STEAM_COMPAT_INSTALL_PATH", game)
            .env("STEAM_COMPAT_LIBRARY_PATHS", g.library.join("steamapps"))
            .env("STEAM_COMPAT_APP_ID", STEAM_APP_ID.to_string())
            .env("SteamAppId", STEAM_APP_ID.to_string())
            .env("SteamGameId", STEAM_APP_ID.to_string());
            c.args(wrapper_command(Some(&proton), game, mode, extra));
        } else {
            c.args(wrapper_command(None, game, mode, extra));
        }
        if which("steam-run") {
            let mut fhs = Command::new("steam-run");
            fhs.arg(c.get_program())
                .args(c.get_args())
                .envs(c.get_envs().filter_map(|(k, v)| Some((k, v?))));
            c = fhs;
        }
        // The wrapper gets its own process group: a Ctrl-C in this terminal
        // is for this command, and must not kill the wrapper (and with it the
        // session's rollback) along with it.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut c, 0);
        let log = store.root.join(WRAPPER_LOG);
        let child = spawn_logged(c.current_dir(game), &log)?;
        say(
            "info",
            &format!(
                "launching F1 25 directly; waiting for the wrapper, output in {}",
                log.display()
            ),
        );
        Ok(WrapperProcess {
            child: Some(child),
            log,
            timeout: DIRECT_START_TIMEOUT,
        })
    } else {
        let mut ask = steam_command(&steam_launch_args(mode));
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut ask, 0);
        ask.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        say(
            "info",
            if mode.is_vr() {
                "asked Steam to launch F1 25 in VR; waiting for the wrapper"
            } else {
                "asked Steam to launch F1 25; waiting for the wrapper"
            },
        );
        Ok(WrapperProcess {
            child: None,
            log: PathBuf::new(),
            timeout: STEAM_START_TIMEOUT,
        })
    }
}

pub(crate) fn which(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

#[cfg(all(test, unix))]
mod tests {
    use swofty_core::game_folder::recovery::SessionLock;
    use swofty_core::session::{game_args, parse_steam_command};

    use crate::commands::run::tests::temp_log;

    use super::*;

    #[test]
    fn reports_a_wrapper_that_exits_before_the_game_starts() {
        let log = temp_log("early");
        let child = spawn_logged(
                Command::new("sh").args([
                    "-c",
                    "echo '[info] installing profile Default'; \
                     echo 'swofty: version.dll already exists in the game folder; run swofty restore' >&2; exit 3",
                ]),
                &log,
            )
            .unwrap();
        let mut w = WrapperProcess {
            child: Some(child),
            log: log.clone(),
            timeout: DIRECT_START_TIMEOUT,
        };
        let status = loop {
            if let Some(s) = w.exited() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(3));
        assert_eq!(
            w.failure(status, "before the game started").to_string(),
            "the wrapper exited (exit status: 3) before the game started: \
                 version.dll already exists in the game folder; run swofty restore"
        );
        std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
    }

    #[test]
    fn an_error_over_several_lines_leads_with_what_failed() {
        let log = temp_log("lines");
        let child = spawn_logged(
                Command::new("sh").args([
                    "-c",
                    "echo 'downloading offline play 0.3.11 from https://x'; \
                     echo '[info] rolled the profile'\"'\"'s game files back'; \
                     echo 'swofty: downloading the offline play extension: offline play 0.3.11: the download was \
                     blocked on this PC or network, typically by antivirus HTTPS or web scanning, a VPN or proxy, a \
                     network filter or a Wi-Fi login page; turning that off or trying another network fixes it' >&2; \
                     echo '(could not download https://x/v0.3.11/f1op_hook.dll: io: received corrupt message of type \
                     InvalidContentType)' >&2; \
                     exit 1",
                ]),
                &log,
            )
            .unwrap();
        let mut w = WrapperProcess {
            child: Some(child),
            log: log.clone(),
            timeout: DIRECT_START_TIMEOUT,
        };
        let status = loop {
            if let Some(s) = w.exited() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            w.failure(status, "before the game started").to_string(),
            "the wrapper exited (exit status: 1) before the game started: downloading the offline play extension: \
                 offline play 0.3.11: the download was blocked on this PC or network, typically by antivirus HTTPS or \
                 web scanning, a VPN or proxy, a network filter or a Wi-Fi login page; turning that off or trying \
                 another network fixes it\n\
                 (could not download https://x/v0.3.11/f1op_hook.dll: io: received corrupt message of type \
                 InvalidContentType)"
        );
        std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
    }

    #[test]
    fn names_the_log_when_the_wrapper_says_nothing() {
        let log = temp_log("silent");
        let child = spawn_logged(Command::new("sh").args(["-c", "exit 1"]), &log).unwrap();
        let mut w = WrapperProcess {
            child: Some(child),
            log: log.clone(),
            timeout: DIRECT_START_TIMEOUT,
        };
        let status = loop {
            if let Some(s) = w.exited() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let msg = w.failure(status, "before the game started").to_string();
        assert!(
            msg.ends_with(&format!("without output; see {}", log.display())),
            "{msg}"
        );
        std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_vr_launch_reaches_the_game_on_every_path() {
        let game = Path::new("/games/F1 25");
        let proton = Path::new("/steam/compatibilitytools.d/GE");
        let direct = strings(&wrapper_command(Some(proton), game, LaunchMode::Vr, &[]));
        assert_eq!(
            direct,
            [
                "--",
                "/steam/compatibilitytools.d/GE/proton",
                "waitforexitandrun",
                "/games/F1 25/F1_25.exe",
                "-vr"
            ]
        );
        let seen = parse_steam_command(&direct[1..]);
        assert_eq!(seen.game_args(), ["-vr"]);
        assert_eq!(LaunchMode::of(seen.game_args()), LaunchMode::Vr);
        assert_eq!(game_args(seen.game_args(), LaunchMode::Vr), ["-vr"]);

        let flat = strings(&wrapper_command(Some(proton), game, LaunchMode::Normal, &[]));
        assert!(parse_steam_command(&flat[1..]).game_args().is_empty());

        let native = strings(&wrapper_command(None, game, LaunchMode::Vr, &[]));
        assert_eq!(native, ["--", "/games/F1 25/F1_25.exe", "-vr"]);
        assert_eq!(parse_steam_command(&native[1..]).game_args(), ["-vr"]);

        let bench = ["-benchmark".to_owned(), "swofty_test.xml".to_owned()];
        let passed = strings(&wrapper_command(Some(proton), game, LaunchMode::Normal, &bench));
        let seen = parse_steam_command(&passed[1..]);
        assert_eq!(seen.game_args(), bench, "game arguments reach the game");
        assert_eq!(
            game_args(seen.game_args(), LaunchMode::of(seen.game_args())),
            bench
        );

        assert_eq!(steam_launch_args(LaunchMode::Normal), ["-applaunch", "3059520"]);
        assert_eq!(
            steam_launch_args(LaunchMode::Vr),
            ["-applaunch", "3059520", "-vr"]
        );
    }

    #[test]
    fn the_wait_for_the_wrapper_counts_from_its_last_output() {
        let log = temp_log("silence");
        let data = log.ancestors().nth(2).unwrap().to_path_buf();
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        let mut silence = Silence::new();
        std::thread::sleep(Duration::from_millis(30));
        assert!(silence.of(&log, &data) >= Duration::from_millis(30));
        std::fs::write(&log, "[info] installing: 2000 of 7977 file(s) copied\n").unwrap();
        assert!(silence.of(&log, &data) < Duration::from_millis(30));
        std::thread::sleep(Duration::from_millis(30));
        assert!(silence.of(&log, &data) >= Duration::from_millis(30));
        std::fs::remove_dir_all(&data).ok();
    }

    fn beat(data: &Path, secs: u64) {
        std::fs::File::options()
            .write(true)
            .open(Busy::path(data))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }

    #[test]
    fn a_live_build_that_keeps_its_heartbeat_is_not_silence() {
        let log = temp_log("building");
        let data = log.ancestors().nth(2).unwrap().to_path_buf();
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "[info] build: still working after 60 s\n").unwrap();
        let build = Busy {
            pid: std::process::id().wrapping_add(1),
            ..Busy::new("default", vec!["swofty.classic-seasons".into()])
        };
        build.write(&data).unwrap();
        let lock = SessionLock::try_acquire(&data).unwrap().unwrap();
        let mut silence = Silence::new();
        silence.of(&log, &data);
        std::thread::sleep(Duration::from_millis(30));
        beat(&data, 1_000);
        assert!(
            silence.of(&log, &data) < Duration::from_millis(30),
            "the heartbeat moved"
        );
        std::thread::sleep(Duration::from_millis(30));
        assert!(
            silence.of(&log, &data) >= Duration::from_millis(30),
            "a build whose heartbeat stopped is silent"
        );
        drop(lock);
        drop(
            SessionLock::wait(&data, Duration::from_secs(10))
                .unwrap()
                .expect("a forked child still holds the dropped lock"),
        );
        beat(&data, 2_000);
        assert!(
            silence.of(&log, &data) >= Duration::from_millis(30),
            "a build file left by a process that ended is silent"
        );
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn keeps_the_last_lines() {
        let log = temp_log("tail");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "a\n\nb\r\nc\n").unwrap();
        assert_eq!(tail(&log, 2), ["b", "c"]);
        assert_eq!(tail(&log, 9), ["a", "b", "c"]);
        assert!(tail(&log.with_extension("missing"), 3).is_empty());
        std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
    }
}
