mod end;
mod env;
mod loader_watch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use swofty_core::GAME_EXE;
use swofty_core::install::Game;
use swofty_core::platform::process::{descends_from, game_processes, stop_process};
use swofty_core::session::{
    Config, LOADER_FALLBACK, LOADER_LOG, LOADER_MARK, LaunchMode, LaunchRequest, LogMark, PERF_FALLBACK,
    game_args, keep_game_logs, parse_steam_command,
};
use swofty_core::store::Store;

use self::end::{EA_UNREAD, Played, end_session, why_no_code};
use self::env::{game_env, game_link};
use self::loader_watch::LoaderWatch;
use super::copies::{RESTART_WAIT, running_copy, wait_relaunched};
use super::loader::check_loader;
use super::platform::link;
use super::{Refused, Round};
use crate::commands::run::RunArgs;
use crate::commands::run::failure::{already_running, start};
use crate::console::output::{info, say};
use crate::signals::ending;
use crate::stores::ea::session_env::{self, SessionEnv};
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
    let usage = swofty_core::online::server::usage::Session::start(&store.root, game);
    let laps = crate::commands::laps::begin(store, config, profile, g);
    let mut watch = LoaderWatch::new(store, g, &session, launched, placed_loader);
    session_env::remove_leftover(game);
    let _session_env = match args.via_ea {
        true => SessionEnv::write(
            game,
            &session_env::Contents::new(&request.id, unix_secs(), &command.env, &config.launch.env),
        ),
        false => None,
    };
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
            let code = crate::stores::ea::wait(game, dir, &copies_before, started, || watch.tick());
            (code.map(|c| c as i32), EA_UNREAD.to_owned())
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
    keep_game_logs(&store.root, game, dir, session.before.as_ref(), launched);
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
        for stale in [
            fallback.clone(),
            fallback.with_extension("prev.log"),
            store.root.join(PERF_FALLBACK),
        ] {
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

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
mod tests;
