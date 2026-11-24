use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Instant;

use swofty_core::diagnostics::launch;
use swofty_core::platform::app_control::{self, Block};
use swofty_core::platform::process::flag;
use swofty_core::platform::steam;
use swofty_core::session::LaunchMode;
use swofty_core::store::Store;
use swofty_i18n::{tr, trf};

use super::{Install, now_secs};

pub const DIRECT_START_FILE: &str = "swofty/direct_start.json";

pub struct Launch {
    child: Child,
    status: Option<ExitStatus>,
    direct_start: Option<PathBuf>,
    pub since: Instant,
    pub at: f64,
    pub log: PathBuf,
    pub saw_game: bool,
    pub stage: Option<String>,
    pub profile: String,
    /// The build's HUD customisation warning was shown, or looked for once the game showed up.
    pub hud_checked: bool,
}

impl Launch {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        store: &Store,
        profile: &str,
        install: &Install,
        mode: LaunchMode,
        netplay: Option<&str>,
        row_ids: Option<&Path>,
        direct_start: Option<&str>,
        steam_saves_once: bool,
    ) -> Result<Self, String> {
        let cli = cli_path(store).ok_or_else(|| app_control::missing(&expected_cli(store)))?;
        let at = now_secs();
        let log = store.root.join("state/launcher-run.log");
        let out = std::fs::create_dir_all(store.root.join("state"))
            .and_then(|_| File::create(&log))
            .map_err(|e| format!("{}: {e}", log.display()))?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        std::fs::remove_file(store.root.join(steam::client::STAGE_FILE)).ok();
        let mut c = Command::new(&cli);
        c.args(run_args(
            &store.root,
            profile,
            &install.dir,
            mode,
            steam_saves_once,
        ));
        if let Some(armed) = netplay {
            c.env(swofty_core::online::multiplayer::client::LAUNCH_ENV, armed);
        }
        if let Some(file) = row_ids {
            c.env(swofty_core::row_ids::ENV, file);
        }
        let request_file = install.dir.join(DIRECT_START_FILE);
        std::fs::remove_file(&request_file).ok();
        let direct_start = match direct_start {
            Some(request) => {
                std::fs::create_dir_all(install.dir.join("swofty"))
                    .and_then(|()| std::fs::write(&request_file, request))
                    .map_err(|e| format!("{}: {e}", request_file.display()))?;
                Some(request_file)
            }
            None => None,
        };
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000);
        let child = c
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| not_started(store, &log, &cli, &e))?;
        Ok(Self {
            child,
            status: None,
            direct_start,
            since: Instant::now(),
            at,
            log,
            saw_game: false,
            stage: None,
            profile: profile.to_owned(),
            hud_checked: false,
        })
    }

    pub fn finished(&mut self) -> Option<bool> {
        let status = self.child.try_wait().ok().flatten()?;
        self.status = Some(status);
        Some(status.success())
    }

    pub fn failure(&self) -> String {
        launch_failure(
            &std::fs::read_to_string(&self.log).unwrap_or_default(),
            self.status,
        )
    }
}

impl Drop for Launch {
    fn drop(&mut self) {
        if let Some(file) = &self.direct_start {
            std::fs::remove_file(file).ok();
        }
    }
}

/// The CLI's arguments for Play. Values go as `--flag=value`, so a profile named
/// `-swofty` from before such names were refused is not read as a flag.
pub(crate) fn run_args(
    data: &Path,
    profile: &str,
    game: &Path,
    mode: LaunchMode,
    steam_saves_once: bool,
) -> Vec<OsString> {
    let mut args = vec![
        flag("data-dir", data),
        flag("profile", profile),
        "run".into(),
        "--apply".into(),
    ];
    if mode.is_vr() {
        args.push("--vr".into());
    }
    if steam_saves_once {
        args.push("--steam-saves-once".into());
    }
    args.push(flag("game", game));
    args
}

/// A CLI that would not start: the error goes in the launch log. One Windows
/// refused is also recorded for Send Logs, and says what blocked it.
pub(super) fn not_started(store: &Store, log: &Path, cli: &Path, e: &std::io::Error) -> String {
    let block = Block::of(cli, e);
    let mut lines = format!("swofty-launcher: could not start {}: {e}\n", cli.display());
    if let Some(b) = &block {
        lines.push_str(&format!("swofty-launcher: {}\n", b.headline()));
        launch::Failure::new(launch::Cause::Blocked(b.clone()))
            .write(&store.root)
            .ok();
    }
    std::fs::write(log, lines).ok();
    block.map_or_else(|| e.to_string(), |b| b.text())
}

/// Why the CLI exited before the game started, from its log, or else its exit
/// status. A CLI that crashed without writing its error says it crashed.
pub(super) fn launch_failure(log: &str, status: Option<ExitStatus>) -> String {
    if let Some(status) = status.filter(crashed)
        && error_line(log).is_none()
    {
        return trf!(
            "the swofty command crashed ({status}) before the game started. Reinstall Swofty Launcher with the \
             installer from f1.swofty.net; your addons and settings are kept",
            status = status
        );
    }
    failure_line(log).unwrap_or_else(|| match status {
        Some(status) => trf!(
            "the swofty command exited ({status}) before the game started, without saying why",
            status = status
        ),
        None => tr("the launch exited before the game started").to_owned(),
    })
}

/// The wrapper's last error, from its `swofty: ` line, or else the last line it
/// wrote. An error over several lines gives its first, which says what failed.
/// A command line the CLI refused gives clap's `error: ` line.
pub(super) fn failure_line(log: &str) -> Option<String> {
    error_line(log).or_else(|| {
        log.lines()
            .rfind(|l| !l.trim().is_empty())
            .map(|l| l.trim_end_matches(':').to_owned())
    })
}

/// The error the CLI wrote: clap's refusal of its command line, or its last
/// `swofty: ` line.
fn error_line(log: &str) -> Option<String> {
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    if let Some(error) = usage_error(&lines) {
        return Some(trf!(
            "the swofty command refused its command line: {error}",
            error = error
        ));
    }
    lines
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix("swofty: "))
        .map(|l| l.trim_end_matches(':').to_owned())
}

fn crashed(status: &ExitStatus) -> bool {
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(status);
    #[cfg(not(unix))]
    let signal = None;
    crash(status.code(), signal)
}

/// An exit that is a crash: an NTSTATUS error code, such as an access
/// violation (0xc0000005), other than the one for a console's Ctrl-C, or
/// SIGILL, SIGABRT, SIGBUS, SIGFPE or SIGSEGV.
pub(super) fn crash(code: Option<i32>, signal: Option<i32>) -> bool {
    const CONTROL_C_EXIT: u32 = 0xC000_013A;
    let ntstatus = code.is_some_and(|c| (c as u32) >> 30 == 0b11 && c as u32 != CONTROL_C_EXIT);
    ntstatus || signal.is_some_and(|s| matches!(s, 4 | 6 | 7 | 8 | 11))
}

/// clap's report of arguments it could not parse: an `error: ` line, then
/// the usage.
pub(super) fn usage_error<'a>(lines: &[&'a str]) -> Option<&'a str> {
    let at = lines.iter().position(|l| l.starts_with("error: "))?;
    lines[at..]
        .iter()
        .any(|l| l.starts_with("Usage: "))
        .then(|| lines[at].trim_start_matches("error: ").trim())
}

/// A failure as the play bar leads with it: a wrapper's error with its
/// innermost cause first and what the wrapper was doing after it, and any
/// other as it is.
pub fn cause_first(failure: &str) -> String {
    let Some(reason) = failure
        .strip_prefix("the wrapper exited (")
        .and_then(|rest| rest.split_once(") "))
        .and_then(|(_, rest)| rest.split_once(": "))
        .map(|(_, reason)| reason)
    else {
        return failure.to_owned();
    };
    match reason.rsplit_once(": ") {
        Some((doing, cause)) if !cause.trim().is_empty() => format!("{cause} ({doing})"),
        _ => reason.to_owned(),
    }
}

pub(super) const CLI: &str = if cfg!(windows) { "swofty.exe" } else { "swofty" };

pub(super) fn beside_launcher() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join(CLI)))
}

pub(crate) fn cli_path(store: &Store) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    beside_launcher()
        .into_iter()
        .chain([store.bin_dir().join(CLI)])
        .chain(std::env::split_paths(&path).map(|d| d.join(CLI)))
        .find(|p| p.is_file())
}

pub(super) fn expected_cli(store: &Store) -> PathBuf {
    beside_launcher().unwrap_or_else(|| store.bin_dir().join(CLI))
}
