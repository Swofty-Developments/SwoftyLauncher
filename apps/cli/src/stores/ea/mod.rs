mod setup;
mod stub;

use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use swofty_core::diagnostics::ea_launch::{self as ea_diag, Tracker};
use swofty_core::game_folder::recovery::Scope;
use swofty_core::install::{Game, Source};
use swofty_core::offline::{self, Slot};
use swofty_core::platform::ea_app;
use swofty_core::platform::process::own_elevation;
use swofty_core::platform::process::{GameProcess, game_processes, stop_process};
use swofty_core::platform::steam::client as steam_client;
use swofty_core::store::Store;

use self::setup::{desktop_opener, handoff, handoff_failed};
use self::stub::{
    NoGame, antivirus, check_stub, no_game, refused_after_link, removed_after_link, restage, stub_block,
};
use crate::console::output::say;
use crate::signals::{StopPhase, ending, session_stop};
use crate::stores::steam::StageFile;
pub use setup::{
    StoreStart, check_elevation, check_recorded, dll_setting, refused_program, stages_offline, store_start,
};
pub use stub::note_stub_blocked;

// The EA app syncs cloud saves before it starts the game, which took about
// 170 s on a test copy, so the wait for the link outlasts that sync.
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(300);

#[cfg(not(windows))]
const SERVER_STOP_TIMEOUT: Duration = Duration::from_secs(30);

const PRESS_PLAY_TIMEOUT: Duration = Duration::from_secs(600);

const HANDOFF_LOG: &str = "ea_handoff.log";

fn detach(c: &mut std::process::Command) -> std::io::Result<Child> {
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(c, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(c, 0x0000_0008 | 0x0000_0200);
    c.spawn()
}

/// A game started since `before` that is not another copy's than `game`'s.
fn new_copy(game: &Path, before: &[u32]) -> Option<u32> {
    game_processes()
        .into_iter()
        .find(|g| !before.contains(&g.pid) && !g.elsewhere(game))
        .map(|g| g.pid)
}

fn wait_for_game(
    game: &Path,
    before: &[u32],
    timeout: Duration,
    handoff: &mut Option<Child>,
    mut watch: Option<&mut Tracker>,
    mut stop: impl FnMut() -> Result<()>,
) -> Result<Option<u32>> {
    let since = Instant::now();
    loop {
        if let Some(t) = watch.as_deref_mut() {
            for line in t.observe(&ea_diag::running_watched(), Instant::now()) {
                say("ea app", &line);
            }
        }
        if let Some(pid) = new_copy(game, before) {
            return Ok(Some(pid));
        }
        if ending() {
            bail!("asked to stop before F1 25 started");
        }
        stop()?;
        if let Some(c) = handoff.as_mut()
            && let Ok(Some(exit)) = c.try_wait()
        {
            *handoff = None;
            if !exit.success() {
                say(
                    "ea app",
                    &format!(
                        "the hand-off command exited with {exit}{}",
                        handoff_hint(exit.code())
                    ),
                );
                return Ok(None);
            }
        }
        if since.elapsed() >= timeout {
            if let Some(t) = watch.as_deref() {
                say("ea app", &format!("timeout: {}", t.summary(since.elapsed())));
            }
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// What a shell's exit code says about the launch.ea_launch_command setting
/// when it could not start the program the setting names.
fn handoff_hint(code: Option<i32>) -> &'static str {
    match code {
        Some(127) if cfg!(not(windows)) => {
            "; the shell found no program at the path launch.ea_launch_command starts (see ea_handoff.log in the \
             session folder): check the path, and put quotes around a path that has spaces"
        }
        Some(126) if cfg!(not(windows)) => {
            "; the shell could not run the program launch.ea_launch_command starts (see ea_handoff.log in the \
             session folder): check that it is a program and may be run"
        }
        _ => "",
    }
}

fn say_elevation() {
    say(
        "ea app",
        &ea_diag::elevation_line(own_elevation(), ea_diag::ea_app_elevation()),
    );
}

/// The session's game, launched by the EA app: hand it the launch link and
/// wait for F1 25 to start. The game files are staged before this.
pub fn start(store: &Store, game: &Path, args: &[String], session: &Path, before: &[u32]) -> Result<Started> {
    let url = ea_app::install_url(game, args).map_err(|e| anyhow!(e))?;
    let h = handoff(store).ok_or_else(|| anyhow!("{}", ea_app::no_handoff()))?;
    check_stub(store, game)?;
    let stage = StageFile(store.root.join(steam_client::STAGE_FILE));
    stage.set("Starting F1 25 through the EA app…");
    say("ea app", &format!("asking the EA app to launch F1 25: {url}"));
    say_elevation();
    let mut tracker = Tracker::new(Instant::now(), &ea_diag::running_watched());
    say(
        "ea app",
        &format!(
            "launch link sent; waiting up to {} s for F1 25",
            HANDOFF_TIMEOUT.as_secs()
        ),
    );
    let log = std::fs::File::create(session.join(HANDOFF_LOG))?;
    let mut c = h.command(&url);
    c.stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let mut removed = removed_after_link(store, game);
    let mut elevation = refused_after_link(store, game);
    let mut stop = || -> Result<()> {
        removed()?;
        elevation()
    };
    let mut handoff = match c.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            say("ea app", &format!("could not run {h}: {e}"));
            return Err(handoff_failed(store, &c, &h, &e));
        }
    };
    let pid = match wait_for_game(
        game,
        before,
        HANDOFF_TIMEOUT,
        &mut handoff,
        Some(&mut tracker),
        &mut stop,
    )? {
        Some(pid) => pid,
        None => {
            if let Some((mut open, _)) = desktop_opener(&h) {
                detach(&mut open).ok();
            }
            let slot = offline::slot(&offline::journal_path(&store.root));
            match &slot {
                Ok(Some(Slot::Staged)) => say("ea app", "offline play's file is in place"),
                Ok(Some(Slot::Replaced)) => say("ea app", "the EA app put its own file over offline play's"),
                Ok(Some(Slot::Missing)) => say("ea app", "offline play's file is gone from the game folder"),
                Ok(Some(Slot::Unknown)) => {
                    say("ea app", "offline play's file was changed by another program")
                }
                Ok(None) => {}
                Err(e) => say("ea app", &format!("could not check offline play's file: {e}")),
            }
            let (line, status) = match no_game(slot.ok().flatten(), || stub_block(store, game)) {
                NoGame::Antivirus(block) => return Err(antivirus(store, block)),
                NoGame::Wait { line, status } => (line, status),
            };
            say("ea app", line);
            stage.set(status);
            wait_for_game(
                game,
                before,
                PRESS_PLAY_TIMEOUT,
                &mut None,
                Some(&mut tracker),
                &mut stop,
            )?
            .ok_or_else(|| {
                anyhow!(
                    "F1 25 was not started from the EA app within {} min; the game files are rolled back",
                    PRESS_PLAY_TIMEOUT.as_secs() / 60
                )
            })?
        }
    };
    say("ea app", &format!("F1 25 started (pid {pid})"));
    let (exit, unopened) = ea_diag::GameExit::open(pid);
    if let Some(line) = unopened {
        say("ea app", &line);
    }
    restage(store, game);
    Ok(Started { handoff, exit })
}

/// An EA app copy started by the EA app as it is installed, with nothing
/// staged: the profile and offline play stay out, so EA AntiCheat runs as
/// it does from the EA app's own Play button.
pub fn launch_installed(store: &Store, g: &Game, vr: bool, opted_out: bool) -> Result<()> {
    if Source::detect(&g.dir) != Source::EaApp {
        bail!(
            "{} is not an EA app copy; start a Steam copy from Steam, or pick the EA app copy with swofty game use ea",
            g.dir.display()
        );
    }
    if let Some(running) = game_processes().first() {
        bail!("F1 25 is already running (pid {}); close it first", running.pid);
    }
    {
        let out = |l: &str| say("info", l);
        let lock = crate::session_lock::lock(store, &out)?;
        crate::session_lock::stale(store, &lock, g, Scope::All, &out)?;
    }
    check_elevation(store, &g.dir, false)?;
    crate::stores::client::ensure_client(store, Source::EaApp, &g.dir, opted_out)?;
    let args: Vec<String> = if vr { vec!["-vr".into()] } else { Vec::new() };
    let url = ea_app::install_url(&g.dir, &args).map_err(|e| anyhow!(e))?;
    let h = handoff(store).ok_or_else(|| anyhow!("{}", ea_app::no_handoff()))?;
    say("ea app", &format!("asking the EA app to launch F1 25: {url}"));
    say_elevation();
    let mut tracker = Tracker::new(Instant::now(), &ea_diag::running_watched());
    say(
        "ea app",
        &format!(
            "launch link sent; waiting up to {} s for F1 25",
            HANDOFF_TIMEOUT.as_secs()
        ),
    );
    let state = store.root.join("state");
    std::fs::create_dir_all(&state)?;
    let log = std::fs::File::create(state.join(HANDOFF_LOG))?;
    let mut c = h.command(&url);
    c.stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let mut child = Some(c.spawn().map_err(|e| handoff_failed(store, &c, &h, &e))?);
    let pid = wait_for_game(
        &g.dir,
        &[],
        HANDOFF_TIMEOUT,
        &mut child,
        Some(&mut tracker),
        || Ok(()),
    )?
    .ok_or_else(|| {
        anyhow!(
            "F1 25 did not start within {} min of the launch link; press Play on F1 25 in the EA app",
            HANDOFF_TIMEOUT.as_secs() / 60
        )
    })?;
    say("ea app", &format!("F1 25 started (pid {pid})"));
    if let Some(line) = command_line(pid) {
        say("ea app", &format!("command line: {line}"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn command_line(pid: u32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let words: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|w| !w.is_empty())
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

#[cfg(not(target_os = "linux"))]
fn command_line(_: u32) -> Option<String> {
    None
}

pub struct Started {
    handoff: Option<Child>,
    exit: ea_diag::GameExit,
}

/// Until every game of this copy started since the launch has closed. Only
/// the game the EA app started for the session, and games that are this
/// copy's, are closed on a stop. The exit code is that game's, read through
/// the handle `start` opened on it: None when the handle did not open, or
/// when the game was still open as the wait gave up.
pub fn wait(
    game: &Path,
    dir: &Path,
    before: &[u32],
    started: Started,
    mut tick: impl FnMut(),
) -> Option<u32> {
    let Started { handoff, mut exit } = started;
    let mut asked = false;
    loop {
        let all = game_processes();
        let pids: Vec<u32> = all.iter().map(|g| g.pid).collect();
        if let Some(line) = exit.observe(&pids) {
            say("ea app", &line);
        }
        let running: Vec<GameProcess> = all
            .into_iter()
            .filter(|g| !before.contains(&g.pid) && !g.elsewhere(game))
            .collect();
        if running.is_empty() {
            break;
        }
        tick();
        let phase = session_stop(dir);
        if phase == StopPhase::GiveUp {
            say(
                "info",
                "F1 25 is still open 20 s after Stop; rolling the game files back without waiting for it",
            );
            break;
        }
        if !asked && phase != StopPhase::Running {
            asked = true;
            say(
                "info",
                "asked to stop: closing F1 25, then rolling its game files back",
            );
            for g in running.iter().filter(|g| g.pid == exit.pid() || g.is_in(game)) {
                stop_process(g.pid).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if let Some(mut c) = handoff {
        std::thread::spawn(move || c.wait().ok());
    }
    exit.code()
}

#[cfg(test)]
mod tests;
