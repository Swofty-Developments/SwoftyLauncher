use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, bail};
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
use crate::output::{info, say};
use crate::run::RunArgs;
use crate::run::failure::{already_running, start};
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
    let winedebug = request
        .winedebug
        .clone()
        .unwrap_or(config.launch.winedebug.clone());
    // Proton's steam helper quote-wraps targets whose path contains spaces
    // and then fails to open the quoted name at all, so the game folder's
    // "F1 25" cannot be passed directly. A space-free symlink to the folder
    // gives an absolute target that survives untouched; the game and the
    // loader see the same files through it.
    let game_link = {
        let link = store.root.join("state/game-link");
        #[cfg(unix)]
        {
            match std::fs::symlink_metadata(&link) {
                Ok(m) if m.file_type().is_symlink() => {
                    if std::fs::read_link(&link).ok().as_deref() != Some(game) {
                        std::fs::remove_file(&link).and_then(|_| std::os::unix::fs::symlink(game, &link))?;
                    }
                }
                Ok(_) => bail!(
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
        let _ = &link;
        link
    };
    // The game gets a curated environment: Proton's steam helper packs it
    // into a Windows environment block (32767 characters), and a build
    // shell's variables blow past that, which kills the game at startup.
    let (shell_vars, mut game_env) = {
        let shell: Vec<(String, String)> = std::env::vars().collect();
        if let Some((k, v)) = shell.iter().find(|(k, _)| is_compat_layer(k))
            && !config.launch.env.keys().any(|k| is_compat_layer(k))
        {
            say(
                "info",
                &format!("environment: not passing the Windows compatibility layer {k}={v} to the game"),
            );
        }
        let n = shell.iter().filter(|(k, _)| !is_compat_layer(k)).count();
        let env = swofty_core::session::game_env(shell, &config.launch.env);
        (n, env)
    };
    if game_env.len() != shell_vars {
        say(
            "info",
            &format!(
                "environment: passing {} of {} variables to the game (the Windows limit is 32K characters)",
                game_env.len(),
                shell_vars
            ),
        );
    }
    let library_path = proton.as_ref().and_then(|_| {
        let steam_root = std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH")
            .map(PathBuf::from)
            .or_else(|| steam::find_game().map(|g| g.steam_root))?;
        swofty_core::session::sdl_library_path(
            game_env.get("LD_LIBRARY_PATH").map(String::as_str),
            &steam_root,
            &store.root.join(RUNTIME_LIBS_DIR),
        )
    });
    if library_path.is_some() {
        say(
            "info",
            "controllers: this system has no SDL2, so Proton reads wheels and pads through Steam's runtime copy",
        );
    }
    game_env.extend(
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
        game_env.insert(swofty_core::launcher_saves::OFF_ENV.to_owned(), "off".to_owned());
    }
    if let Some(armed) = &request.netplay {
        for (name, value) in parse_launch_env(armed) {
            match game_env.get_mut(&name) {
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
                    game_env.insert(name, value);
                }
            }
        }
    }
    // The prefix's registry override is not always what Wine reads at start,
    // so the session also names version.dll in WINEDLLOVERRIDES, which wins
    // over the registry.
    if proton.is_some() && placed_loader {
        let prefix = match g.user_reg.as_deref() {
            Some(reg)
                if std::fs::read_to_string(reg).is_ok_and(|t| swofty_core::install::has_override(&t)) =>
            {
                format!("{} has the version.dll override", reg.display())
            }
            Some(reg) => format!("{} has no version.dll override", reg.display()),
            None => "no Proton prefix found for this game folder".to_owned(),
        };
        let set = game_env.get("WINEDLLOVERRIDES").map(String::as_str);
        match swofty_core::session::with_version_override(set) {
            Some(v) => {
                game_env.insert("WINEDLLOVERRIDES".to_owned(), v);
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
    let listed: String = game_env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(dir.join("env.txt"), listed)?;
    let spawn = |exe: &str, log: &str| -> Result<std::process::Child> {
        let out = std::fs::File::create(dir.join(log))?;
        let mut c = match &proton {
            Some(p) => {
                let mut c = Command::new(p);
                c.arg("run").arg(game_link.join(exe));
                c
            }
            None => Command::new(game.join(exe)),
        };
        c.args(&extra)
            .current_dir(game)
            .env_clear()
            .envs(&game_env)
            .stdout(out.try_clone()?)
            .stderr(out);
        start(&mut c)
    };
    let copies_before: Vec<u32> = game_processes().iter().map(|g| g.pid).collect();
    let launched = SystemTime::now();
    std::fs::write(dir.join("game_start"), format!("{}\n", now_secs()))?;
    // The offline-play session launcher (staged in place of the EA
    // AntiCheat launcher) creates the game suspended and patches the launch
    // guard before the game's first instruction: the check runs before any
    // window exists, so the in-process hook is too late to act on it.
    let target = if args.via_ea {
        GAME_EXE
    } else if game.join("EAAntiCheat.GameServiceLauncher.exe.bak").is_file() {
        say("info", "offline play: launching via the session launcher");
        "EAAntiCheat.GameServiceLauncher.exe"
    } else {
        GAME_EXE
    };
    if mode.is_vr() {
        say("info", &format!("starting F1 25 in VR ({})", extra.join(" ")));
    }
    let usage = swofty_core::usage::Session::start(&store.root, game);
    let laps = crate::laps::begin(store, config, profile, g);
    let new_watch =
        || AttachWatch::new(loader.clone(), before.clone(), Instant::now()).with_fallback(fallback.clone());
    let mut watch = placed_loader.then(new_watch);
    let placed = quarantine::placed_dlls(&store.root)
        .unwrap_or_default()
        .into_iter()
        .find_map(|(rel, sha)| rel.eq_ignore_ascii_case(PROXY).then_some(sha));
    let sampled: std::cell::RefCell<Option<Vec<loader_check::Seen>>> = std::cell::RefCell::new(None);
    let missed = || {
        let mut found = loader_check::collect(game, placed.as_deref(), launched, g.user_reg.as_deref());
        if found.games.is_empty()
            && let Some(early) = sampled.borrow_mut().take()
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
            loader_missed(store, game, found.cause(game));
        }
    };
    let ea = match args.via_ea {
        true => Some(crate::ea::start(store, game, &extra, &dir, &copies_before)?),
        false => None,
    };
    if ea.is_some() {
        watch = placed_loader.then(new_watch);
    }
    let verdict = |state: Attach| match state {
        Attach::Missed => missed(),
        Attach::Attached => note_refused(&fallback),
        Attach::Waiting => {}
    };
    let tick = || {
        if let Some(w) = watch.as_mut()
            && w.state() == Attach::Waiting
        {
            if w.probe_due(Instant::now()) {
                let early = loader_check::collect(game, placed.as_deref(), launched, g.user_reg.as_deref());
                *sampled.borrow_mut() = Some(early.games);
            }
            verdict(w.poll(Instant::now()));
        }
    };
    let via_ea = ea.is_some();
    let (code, unread) = match ea {
        Some(started) => {
            crate::ea::wait(game, &dir, &copies_before, started, tick);
            (None, EA_UNREAD.to_owned())
        }
        None => {
            let status = wait_game(&mut spawn(target, "game_stdout.log")?, tick)?;
            (status.code(), why_no_code(status))
        }
    };
    if let Some(w) = watch.as_mut()
        && w.state() == Attach::Waiting
    {
        verdict(w.ended(Instant::now()));
    }
    wait_relaunched(
        game,
        &dir,
        &copies_before,
        code,
        launched.elapsed().unwrap_or_default(),
    );
    crate::laps::end(laps, game);
    keep_game_logs(&store.root, game, &fallback, &dir, before.as_ref(), launched);
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
    let end = SessionEnd::resolve(code, &unread, stopped, loader.as_ref());
    say(
        "info",
        &format!("session end: {}, {}", end.end.as_str(), end.reason),
    );
    if let Some(usage) = usage {
        usage.end(&end);
    }
    let code = end.exit_code.unwrap_or(-1);
    end.write(&dir)?;
    std::fs::write(dir.join("exit_code"), format!("{code}\n"))?;
    std::fs::write(dir.join("game_end"), format!("{}\n", now_secs()))?;
    note_known_problem(store, game, &dir, end.end == swofty_core::session::Ending::Crash);
    for g in game_processes()
        .into_iter()
        .filter(|g| descends_from(g.pid, std::process::id()))
    {
        stop_process(g.pid).ok();
    }
    let blocked =
        via_ea && end.end != swofty_core::session::Ending::Quit && crate::ea::note_stub_blocked(store, game);
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
