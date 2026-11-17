use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use swofty_core::install::{Game, PROXY};
use swofty_core::launch;
use swofty_core::loader_check;
use swofty_core::multiplayer::client::parse_launch_env;
use swofty_core::process::{descends_from, game_processes, stop_process};
use swofty_core::quarantine::{self, Attach, AttachWatch};
use swofty_core::session::{
    Config, LOADER_FALLBACK, LOADER_LOG, LOADER_MARK, LaunchMode, LaunchRequest, LogMark, PERF_LOG,
    RUNTIME_LIBS_DIR, STOP_FILE, SessionEnd, SessionRecord, game_args, is_compat_layer, parse_steam_command,
    written_since,
};
use swofty_core::store::Store;
use swofty_core::{GAME_EXE, STEAM_APP_ID, steam};

use super::copies::{RESTART_WAIT, other_copies, running_copy, wait_relaunched};
use super::loader::{check_loader, loader_missed, note_refused};
use super::platform::link;
use super::{Refused, Round};
use crate::commands::run::RunArgs;
use crate::commands::run::failure::{already_running, start};
use crate::console::output::{info, say};
use crate::signals::ending;
use crate::time::{now_secs, stamp};

pub(crate) fn play(
    store: &Store,
    args: &RunArgs,
    config: &Config,
    request: &LaunchRequest,
    g: &Game,
    installed: Option<Instant>,
    round: Round,
) -> Result<i32> {
    let Round { profile, restarted } = round;
    let game = g.dir.as_path();
    let placed_loader = check_loader(store, game, installed).map_err(Refused)?;
    // The profile build and the offline play download before the first
    // start can take minutes, time enough for F1 25 to be started from
    // Steam. On Apply & restart the previous copy is still on its way out.
    if let Some(pid) = running_copy(if restarted { RESTART_WAIT } else { Duration::ZERO }) {
        return Err(already_running(store, pid, "close it before launching it again"));
    }
    let session = SessionDir::open(store, game, request, args)?;
    let dir = session.dir.as_path();
    let cmd = parse_steam_command(&args.steam_command);
    let mode = if args.vr {
        LaunchMode::Vr
    } else {
        LaunchMode::of(cmd.game_args())
    };
    let extra = game_args(cmd.game_args(), mode);
    let proton = cmd.proton.map(PathBuf::from).or_else(|| {
        let tools = std::env::var("STEAM_COMPAT_TOOL_PATHS").ok()?;
        Some(PathBuf::from(tools.split(':').next()?).join("proton"))
    });
    let command = GameCommand {
        game,
        extra,
        link: game_link(store, game)?,
        env: game_env(store, config, request, g, dir, proton.is_some(), placed_loader)?,
        proton,
    };
    let copies_before: Vec<u32> = game_processes().iter().map(|g| g.pid).collect();
    let launched = SystemTime::now();
    std::fs::write(dir.join("game_start"), format!("{}\n", now_secs()))?;
    let target = start_target(args.via_ea, game);
    if mode.is_vr() {
        say(
            "info",
            &format!("starting F1 25 in VR ({})", command.extra.join(" ")),
        );
    }
    let usage = swofty_core::usage::Session::start(&store.root, game);
    let laps = crate::commands::laps::begin(store, config, profile, g);
    let mut watch = LoaderWatch::new(store, g, &session, launched, placed_loader);
    let ea = match args.via_ea {
        true => Some(crate::stores::ea::start(
            store,
            game,
            &command.extra,
            dir,
            &copies_before,
        )?),
        false => None,
    };
    if ea.is_some() {
        watch.restart();
    }
    let via_ea = ea.is_some();
    let (code, unread) = match ea {
        Some(started) => {
            crate::stores::ea::wait(game, dir, &copies_before, started, || watch.tick());
            (None, EA_UNREAD.to_owned())
        }
        None => {
            let status = wait_game(&mut command.spawn(dir, target, "game_stdout.log")?, || {
                watch.tick()
            })?;
            (status.code(), why_no_code(status))
        }
    };
    watch.ended();
    wait_relaunched(
        game,
        dir,
        &copies_before,
        code,
        launched.elapsed().unwrap_or_default(),
    );
    crate::commands::laps::end(laps, game);
    keep_game_logs(
        &store.root,
        game,
        &session.fallback,
        dir,
        session.before.as_ref(),
        launched,
    );
    end_session(store, game, dir, Played { code, unread, via_ea }, launched, usage)
}

/// A session's folder under the sessions folder, with the loader log's mark
/// from before the game started.
struct SessionDir {
    dir: PathBuf,
    loader: PathBuf,
    before: Option<LogMark>,
    fallback: PathBuf,
}

impl SessionDir {
    fn open(store: &Store, game: &Path, request: &LaunchRequest, args: &RunArgs) -> Result<Self> {
        let dir = store.sessions_dir().join(stamp());
        std::fs::create_dir_all(&dir)?;
        let loader = game.join(LOADER_LOG);
        let before = LogMark::read(&loader);
        std::fs::write(dir.join(LOADER_MARK), serde_json::to_vec(&before)?)?;
        let fallback = store.root.join(LOADER_FALLBACK);
        for stale in [fallback.clone(), fallback.with_extension("prev.log")] {
            std::fs::remove_file(stale).ok();
        }
        let latest = store.sessions_dir().join("latest");
        std::fs::remove_file(&latest).ok();
        link(&dir, &latest);
        std::fs::write(store.sessions_dir().join("latest.txt"), dir.display().to_string())?;
        std::fs::write(dir.join("request_id"), format!("{}\n", request.id))?;
        std::fs::write(dir.join("steam_command.txt"), shell_join(&args.steam_command))?;
        Ok(Self {
            dir,
            loader,
            before,
            fallback,
        })
    }
}

/// Proton's steam helper quote-wraps targets whose path contains spaces
/// and then fails to open the quoted name at all, so the game folder's
/// "F1 25" cannot be passed directly. A space-free symlink to the folder
/// gives an absolute target that survives untouched; the game and the
/// loader see the same files through it.
fn game_link(store: &Store, game: &Path) -> Result<PathBuf> {
    let link = store.root.join("state/game-link");
    #[cfg(unix)]
    {
        match std::fs::symlink_metadata(&link) {
            Ok(m) if m.file_type().is_symlink() => {
                if std::fs::read_link(&link).ok().as_deref() != Some(game) {
                    std::fs::remove_file(&link).and_then(|_| std::os::unix::fs::symlink(game, &link))?;
                }
            }
            Ok(_) => anyhow::bail!(
                "{} exists and is not the game link; remove it and try again",
                link.display()
            ),
            Err(_) => {
                std::fs::create_dir_all(store.root.join("state"))?;
                std::os::unix::fs::symlink(game, &link)?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = game;
    Ok(link)
}

/// The game's environment, also listed in the session's env.txt.
fn game_env(
    store: &Store,
    config: &Config,
    request: &LaunchRequest,
    g: &Game,
    dir: &Path,
    proton: bool,
    placed_loader: bool,
) -> Result<BTreeMap<String, String>> {
    let winedebug = request
        .winedebug
        .clone()
        .unwrap_or(config.launch.winedebug.clone());
    let mut env = curated_env(config);
    let library_path = if proton {
        steam_runtime_library_path(store, &env)
    } else {
        None
    };
    env.extend(
        [
            ("PROTON_LOG", "1".to_owned()),
            ("PROTON_LOG_DIR", dir.to_string_lossy().into_owned()),
            ("WINEDEBUG", winedebug),
            ("SteamAppId", STEAM_APP_ID.to_string()),
            ("SteamGameId", STEAM_APP_ID.to_string()),
        ]
        .into_iter()
        .chain(library_path.map(|p| ("LD_LIBRARY_PATH", p)))
        .map(|(k, v)| (k.to_owned(), v)),
    );
    if request.steam_saves_once {
        env.insert(swofty_core::launcher_saves::OFF_ENV.to_owned(), "off".to_owned());
    }
    if let Some(armed) = &request.netplay {
        merge_launch_env(&mut env, parse_launch_env(armed));
    }
    if proton && placed_loader {
        name_version_override(&mut env, g);
    }
    let listed: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(dir.join("env.txt"), listed)?;
    Ok(env)
}

/// The game gets a curated environment: Proton's steam helper packs it
/// into a Windows environment block (32767 characters), and a build
/// shell's variables blow past that, which kills the game at startup.
fn curated_env(config: &Config) -> BTreeMap<String, String> {
    let shell: Vec<(String, String)> = std::env::vars().collect();
    if let Some((k, v)) = shell.iter().find(|(k, _)| is_compat_layer(k))
        && !config.launch.env.keys().any(|k| is_compat_layer(k))
    {
        say(
            "info",
            &format!("environment: not passing the Windows compatibility layer {k}={v} to the game"),
        );
    }
    let shell_vars = shell.iter().filter(|(k, _)| !is_compat_layer(k)).count();
    let env = swofty_core::session::game_env(shell, &config.launch.env);
    if env.len() != shell_vars {
        say(
            "info",
            &format!(
                "environment: passing {} of {} variables to the game (the Windows limit is 32K characters)",
                env.len(),
                shell_vars
            ),
        );
    }
    env
}

/// The library path that lets Proton read wheels and pads through Steam's
/// runtime copy of SDL2 on a system without one.
fn steam_runtime_library_path(store: &Store, env: &BTreeMap<String, String>) -> Option<String> {
    let steam_root = std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH")
        .map(PathBuf::from)
        .or_else(|| steam::find_game().map(|g| g.steam_root))?;
    let path = swofty_core::session::sdl_library_path(
        env.get("LD_LIBRARY_PATH").map(String::as_str),
        &steam_root,
        &store.root.join(RUNTIME_LIBS_DIR),
    )?;
    say(
        "info",
        "controllers: this system has no SDL2, so Proton reads wheels and pads through Steam's runtime copy",
    );
    Some(path)
}

/// An online session's launch variables: a log channel list gains the
/// channels it lacks, and a variable the game already has keeps its value.
fn merge_launch_env(env: &mut BTreeMap<String, String>, launch: Vec<(String, String)>) {
    for (name, value) in launch {
        match env.get_mut(&name) {
            Some(have) if name == "SWOFTY_GAME_LOG_CHANNELS" => {
                let named = |c: &str| c.trim().split(':').next().unwrap_or("").to_owned();
                for channel in value.split(',').map(str::trim) {
                    let name = named(channel);
                    if name.is_empty() || name == "default" || have.split(',').any(|h| named(h) == name) {
                        continue;
                    }
                    if !have.trim().is_empty() {
                        have.push(',');
                    }
                    have.push_str(channel);
                }
            }
            Some(_) => {}
            None => {
                env.insert(name, value);
            }
        }
    }
}

/// The prefix's registry override is not always what Wine reads at start,
/// so the session also names version.dll in WINEDLLOVERRIDES, which wins
/// over the registry.
fn name_version_override(env: &mut BTreeMap<String, String>, g: &Game) {
    let prefix = match g.user_reg.as_deref() {
        Some(reg) if std::fs::read_to_string(reg).is_ok_and(|t| swofty_core::install::has_override(&t)) => {
            format!("{} has the version.dll override", reg.display())
        }
        Some(reg) => format!("{} has no version.dll override", reg.display()),
        None => "no Proton prefix found for this game folder".to_owned(),
    };
    let set = env.get("WINEDLLOVERRIDES").map(String::as_str);
    match swofty_core::session::with_version_override(set) {
        Some(v) => {
            env.insert("WINEDLLOVERRIDES".to_owned(), v);
            say(
                "info",
                &format!("wine: {prefix}; WINEDLLOVERRIDES loads the loader's version.dll"),
            );
        }
        None => say(
            "info",
            &format!(
                "wine: {prefix}; WINEDLLOVERRIDES already names version.dll: {}",
                set.unwrap_or_default()
            ),
        ),
    }
}

/// The offline-play session launcher (staged in place of the EA
/// AntiCheat launcher) creates the game suspended and patches the launch
/// guard before the game's first instruction: the check runs before any
/// window exists, so the in-process hook is too late to act on it.
fn start_target(via_ea: bool, game: &Path) -> &'static str {
    if via_ea {
        GAME_EXE
    } else if game.join("EAAntiCheat.GameServiceLauncher.exe.bak").is_file() {
        say("info", "offline play: launching via the session launcher");
        "EAAntiCheat.GameServiceLauncher.exe"
    } else {
        GAME_EXE
    }
}

/// How the game is started: through Proton when Steam runs it under one,
/// else directly.
struct GameCommand<'a> {
    game: &'a Path,
    extra: Vec<String>,
    link: PathBuf,
    env: BTreeMap<String, String>,
    proton: Option<PathBuf>,
}

impl GameCommand<'_> {
    fn spawn(&self, dir: &Path, exe: &str, log: &str) -> Result<Child> {
        let out = std::fs::File::create(dir.join(log))?;
        let mut c = match &self.proton {
            Some(p) => {
                let mut c = Command::new(p);
                c.arg("run").arg(self.link.join(exe));
                c
            }
            None => Command::new(self.game.join(exe)),
        };
        c.args(&self.extra)
            .current_dir(self.game)
            .env_clear()
            .envs(&self.env)
            .stdout(out.try_clone()?)
            .stderr(out);
        start(&mut c)
    }
}

/// Whether the loader's version.dll attached to the game, watched while it
/// runs and settled when it ends.
struct LoaderWatch<'a> {
    store: &'a Store,
    g: &'a Game,
    session: &'a SessionDir,
    launched: SystemTime,
    on: bool,
    watch: Option<AttachWatch>,
    placed: Option<String>,
    early: Option<Vec<loader_check::Seen>>,
}

impl<'a> LoaderWatch<'a> {
    fn new(store: &'a Store, g: &'a Game, session: &'a SessionDir, launched: SystemTime, on: bool) -> Self {
        let mut watch = Self {
            store,
            g,
            session,
            launched,
            on,
            watch: None,
            placed: None,
            early: None,
        };
        watch.restart();
        watch.placed = quarantine::placed_dlls(&store.root)
            .unwrap_or_default()
            .into_iter()
            .find_map(|(rel, sha)| rel.eq_ignore_ascii_case(PROXY).then_some(sha));
        watch
    }

    fn restart(&mut self) {
        self.watch = self.on.then(|| {
            AttachWatch::new(
                self.session.loader.clone(),
                self.session.before.clone(),
                Instant::now(),
            )
            .with_fallback(self.session.fallback.clone())
        });
    }

    fn tick(&mut self) {
        let probe = match self.watch.as_mut() {
            Some(w) if w.state() == Attach::Waiting => w.probe_due(Instant::now()),
            _ => return,
        };
        if probe {
            self.early = Some(self.collect().games);
        }
        if let Some(state) = self.watch.as_mut().map(|w| w.poll(Instant::now())) {
            self.verdict(state);
        }
    }

    fn ended(&mut self) {
        let state = match self.watch.as_mut() {
            Some(w) if w.state() == Attach::Waiting => w.ended(Instant::now()),
            _ => return,
        };
        self.verdict(state);
    }

    fn verdict(&mut self, state: Attach) {
        match state {
            Attach::Missed => self.missed(),
            Attach::Attached => note_refused(&self.session.fallback),
            Attach::Waiting => {}
        }
    }

    fn collect(&self) -> loader_check::Findings {
        loader_check::collect(
            &self.g.dir,
            self.placed.as_deref(),
            self.launched,
            self.g.user_reg.as_deref(),
        )
    }

    fn missed(&mut self) {
        let game = self.g.dir.as_path();
        let mut found = self.collect();
        if found.games.is_empty()
            && let Some(early) = self.early.take()
        {
            found.games = early;
            say(
                "loader",
                &format!(
                    "  the game had closed by now; the game lines below are from a look {} s after it started",
                    quarantine::PROBE_AFTER.as_secs()
                ),
            );
        }
        let running = found.attached_text();
        say(
            "loader",
            match running {
                Some(_) => {
                    "version.dll attached to F1 25, which the session's watch missed; what the session found:"
                }
                None => "version.dll did not attach to F1 25; what the session found:",
            },
        );
        for line in found.lines(game) {
            say("loader", &format!("  {line}"));
        }
        if let Some(text) = running {
            say("info", text);
            return;
        }
        // The antivirus explanation is a Windows one; under Proton a loader
        // that does not attach is a prefix or override problem instead.
        if cfg!(windows) {
            loader_missed(self.store, game, found.cause(game));
        }
    }
}

/// How the game's run ended, as the wait saw it.
struct Played {
    code: Option<i32>,
    unread: String,
    via_ea: bool,
}

/// Settle how the session ended, record it in the session folder, and stop
/// whatever of the game is still running under the wrapper.
fn end_session(
    store: &Store,
    game: &Path,
    dir: &Path,
    played: Played,
    launched: SystemTime,
    usage: Option<swofty_core::usage::Session>,
) -> Result<i32> {
    let loader = std::fs::read(dir.join("loader.log"))
        .ok()
        .map(|b| launch::LoaderSeen::scan(&String::from_utf8_lossy(&b)));
    let stopped = if dir.join(STOP_FILE).exists() {
        Some("Stop in the launcher closed F1 25")
    } else if ending() {
        Some("the wrapper was asked to stop and closed F1 25")
    } else {
        None
    };
    let end = SessionEnd::resolve(played.code, &played.unread, stopped, loader.as_ref());
    say(
        "info",
        &format!("session end: {}, {}", end.end.as_str(), end.reason),
    );
    if let Some(usage) = usage {
        usage.end(&end);
    }
    let code = end.exit_code.unwrap_or(-1);
    end.write(dir)?;
    std::fs::write(dir.join("exit_code"), format!("{code}\n"))?;
    std::fs::write(dir.join("game_end"), format!("{}\n", now_secs()))?;
    note_known_problem(store, game, dir, end.end == swofty_core::session::Ending::Crash);
    for g in game_processes()
        .into_iter()
        .filter(|g| descends_from(g.pid, std::process::id()))
    {
        stop_process(g.pid).ok();
    }
    let blocked = played.via_ea
        && end.end != swofty_core::session::Ending::Quit
        && crate::stores::ea::note_stub_blocked(store, game);
    if !blocked {
        note_failed_launch(store, loader, code, launched.elapsed().unwrap_or_default());
    }
    Ok(code)
}

/// The known problem a crashed session, or one that ended stuck, matches, for
/// the session log and Send Logs; the launcher shows the same diagnosis to the
/// player.
fn note_known_problem(store: &Store, game: &Path, dir: &Path, crashed: bool) {
    let record = SessionRecord::read(dir);
    if !crashed {
        if let Some(d) = swofty_core::known_problems::stall(&record, Some(game), store)
            .or_else(|| swofty_core::known_problems::tyres(&record, Some(game), store))
        {
            say("warning", &format!("known problem {}: {}", d.id, d.title));
        }
        return;
    }
    match swofty_core::known_problems::crash(&record, Some(game), store) {
        Some(d) => say("crash", &format!("known problem {}: {}", d.id, d.title)),
        None => say("crash", "no known problem matches this crash"),
    }
}

const EA_UNREAD: &str =
    "the EA app started F1 25, so the session has no handle on the game to read its exit code";

/// Why an exit status carries no code.
pub(crate) fn why_no_code(status: ExitStatus) -> String {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return format!("the game's process was ended by signal {signal}, which leaves no exit code");
    }
    format!("the game's process ended without an exit code ({status})")
}

/// After the game exits: a clean exit before its window is a launch that
/// failed, which the session says and records instead of passing it off as
/// a normal quit.
fn note_failed_launch(store: &Store, loader: Option<launch::LoaderSeen>, code: i32, ran: Duration) {
    let ours = loader.and_then(|l| l.pid).filter(|_| cfg!(windows));
    let Some(cause) = launch::classify(code, ran, loader.as_ref(), || other_copies(ours)) else {
        return;
    };
    let failure = launch::Failure::new(cause);
    say("warning", &format!("{}: {}", failure.title(), failure.text()));
    if let Err(e) = failure.write(&store.root) {
        info(&format!("could not record it for the launcher: {e}"));
    }
}

/// The session's loader.log: the game folder's when the loader wrote it,
/// else its fallback in the data folder, which a session starts without.
pub(crate) fn keep_game_logs(
    data: &Path,
    game: &Path,
    fallback: &Path,
    dir: &Path,
    before: Option<&LogMark>,
    launched: SystemTime,
) {
    swofty_core::crash_report::keep_fallback(data, &dir.join("game-logs"), launched);
    let loader = game.join(LOADER_LOG);
    if LogMark::replaced(&loader, before) {
        std::fs::copy(&loader, dir.join("loader.log")).ok();
    } else if fallback.is_file() {
        std::fs::copy(fallback, dir.join("loader.log")).ok();
    }
    let perf = game.join(PERF_LOG);
    if written_since(&perf, launched) {
        std::fs::copy(&perf, dir.join("perf.log")).ok();
    }
}

/// Wait for the game; a stop signal to the wrapper (Steam's Stop button, a
/// kill, a closed terminal) closes the game it started instead of leaving
/// the session's files in the game folder.
fn wait_game(child: &mut Child, mut tick: impl FnMut()) -> Result<ExitStatus> {
    let mut asked = false;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        tick();
        if !asked && ending() {
            asked = true;
            info("asked to stop: closing F1 25, then rolling its game files back");
            let ours: Vec<u32> = game_processes()
                .into_iter()
                .filter(|g| descends_from(g.pid, std::process::id()))
                .map(|g| g.pid)
                .collect();
            if ours.is_empty() {
                child.kill().ok();
            }
            for pid in ours {
                stop_process(pid).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+%@".contains(c))
            {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(all(test, unix))]
mod tests {
    use crate::commands::run::tests::{STALE, session_dir};

    use super::*;

    #[test]
    fn keeps_the_loader_and_perf_logs_this_session_wrote() {
        let dir = session_dir("keep-logs");
        let game = dir.join("game");
        let session = dir.join("session");
        std::fs::create_dir_all(&session).unwrap();
        let loader = game.join(LOADER_LOG);
        let perf = game.join(PERF_LOG);
        let touch = |path: &Path, at: SystemTime| {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        };
        let launched = SystemTime::now();
        std::fs::write(&loader, STALE).unwrap();
        std::fs::write(&perf, "Swofty Launcher performance report\nold\n").unwrap();
        touch(&perf, launched - Duration::from_secs(600));
        let before = LogMark::read(&loader);
        let fallback = dir.join(LOADER_FALLBACK);
        keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
        assert!(!session.join("loader.log").exists());
        assert!(!session.join("perf.log").exists());

        std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
        std::fs::write(&fallback, "attached in the data folder\n").unwrap();
        keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
        assert_eq!(
            std::fs::read_to_string(session.join("loader.log")).unwrap(),
            "attached in the data folder\n"
        );

        std::fs::write(&loader, "fresh\n").unwrap();
        std::fs::write(&perf, "Swofty Launcher performance report\nnew\n").unwrap();
        touch(&perf, launched + Duration::from_secs(1));
        keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
        assert_eq!(
            std::fs::read_to_string(session.join("loader.log")).unwrap(),
            "fresh\n"
        );
        assert_eq!(
            std::fs::read_to_string(session.join("perf.log")).unwrap(),
            "Swofty Launcher performance report\nnew\n"
        );

        std::fs::write(
            fallback.with_file_name("crash-20261101-125350.txt"),
            "Swofty crash report\n",
        )
        .unwrap();
        keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
        assert!(session.join("game-logs/crash-20261101-125350.txt").is_file());
        assert!(!fallback.with_file_name("crash-20261101-125350.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_game_ended_by_a_signal_has_no_exit_code_and_says_why() {
        let status = Command::new("sh").args(["-c", "kill -9 $$"]).status().unwrap();
        assert_eq!(status.code(), None);
        let end = SessionEnd::resolve(status.code(), &why_no_code(status), None, None);
        assert_eq!(
            (end.end, end.exit_code),
            (swofty_core::session::Ending::Unknown, None)
        );
        assert_eq!(
            end.reason,
            "the game's process was ended by signal 9, which leaves no exit code"
        );
    }
}
