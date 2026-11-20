use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use swofty_core::GAME_EXE;
use swofty_core::diagnostics::launch;
use swofty_core::diagnostics::quarantine::{self, Stage};
use swofty_core::game_folder::recovery::{Recovered, Scope, SessionLock};
use swofty_core::install::{Game, Source};
use swofty_core::online::multiplayer::client::LAUNCH_ENV as NETPLAY_ENV;
use swofty_core::platform::process::{game_processes, game_running_in, stop_game_in};
use swofty_core::session::{LOADER_LOG, LaunchMode, LaunchRequest, LogMark, request_stop};
use swofty_core::store::Store;

use super::RunArgs;
use super::failure::{already_running, note_blocked};
use super::follow::{Follow, Follower, symbols};
use super::wrapper_process::{Silence, WrapperProcess, start_wrapper};
use crate::console::output::{info, say};
use crate::signals::{install_ctrl_c, stop_requested};
use crate::stores::steam::ensure_steamvr;
use crate::time::now_secs;

pub(crate) fn run(store: &Store, profile: &str, args: &RunArgs, g: &Game) -> Result<ExitCode> {
    crate::commands::build::supported(g)?;
    let game = g.dir.clone();
    let running = game_processes();
    if let Some(first) = running.first() {
        if !args.restart {
            return Err(already_running(
                store,
                first.pid,
                "close it first, or pass --restart to close it",
            ));
        }
        if let Some(other) = running.iter().find(|g| !g.is_in(&game)) {
            return Err(already_running(
                store,
                other.pid,
                &format!(
                    "it isn't the copy in {}, so --restart leaves it running; close it first",
                    game.display()
                ),
            ));
        }
        request_stop(&store.root);
        stop_game_in(&game).ok();
        while game_running_in(&game) {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    {
        let lock = crate::session_lock::lock(store, &info)?;
        crate::session_lock::stale(store, &lock, g, Scope::All, &info)?;
    }
    // Modded sessions run without EA AntiCheat: the terms are accepted once,
    // and declining means the game is not launched.
    crate::commands::offline::gate(store, std::io::stdin().is_terminal())?;
    if args.apply {
        crate::commands::setup::gate(store)?;
    }
    if let Some(state) = swofty_core::platform::steam::find_game()
        .filter(|i| swofty_core::game_folder::copies::same_dir(&i.dir, &game))
        .and_then(|i| swofty_core::platform::steam::busy(&i))
    {
        bail!("Steam is {state} F1 25; wait for it to finish, then press Play again");
    }
    let source = Source::detect(&game);
    if crate::stores::ea::store_start(source) == crate::stores::ea::StoreStart::EaApp {
        crate::stores::ea::check_recorded(&game)?;
        let stock = swofty_core::build_support::blocked(&game.join(GAME_EXE)).is_some();
        let vanilla = store.profile(profile).is_ok_and(|p| p.is_vanilla());
        crate::stores::ea::check_elevation(store, &game, !stock && !vanilla)?;
        if args.apply && !stock {
            crate::stores::ea::dll_setting(store, g, args.no_start_store)?;
        }
    }
    crate::stores::client::ensure_client(store, source, &game, args.no_start_store)?;
    let id = format!("{:x}{:x}", std::process::id(), (now_secs() * 1e6) as u64);
    let request = LaunchRequest {
        id: id.clone(),
        winedebug: Some(args.winedebug.clone()),
        profile: Some(profile.into()),
        apply: args.apply,
        netplay: std::env::var(NETPLAY_ENV).ok().filter(|v| !v.is_empty()),
        row_ids: std::env::var_os(swofty_core::row_ids::ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from),
        steam_saves_once: args.steam_saves_once,
    };
    swofty_core::write_json(&store.root.join("state/launch.json"), &request)?;
    let mode = if args.vr {
        LaunchMode::Vr
    } else {
        LaunchMode::Normal
    };
    if mode.is_vr() && !cfg!(windows) {
        ensure_steamvr();
    }
    let loader = game.join(LOADER_LOG);
    let before = LogMark::read(&loader);
    let asked = now_secs();
    let _home_menu = swofty_core::ui::home_menu::publish(
        &store.root,
        &game,
        env!("CARGO_PKG_VERSION"),
        store.config().home_video,
    );
    let mut wrapper = start_wrapper(store, &game, args.direct, mode, &args.game_arg)
        .inspect_err(|e| note_blocked(store, e))?;
    let mut silence = Silence::new();
    let dir = loop {
        if let Some(d) = store
            .latest_session()
            .filter(|d| std::fs::read_to_string(d.join("request_id")).is_ok_and(|s| s.trim() == id))
        {
            break d;
        }
        if let Some(report) = refused(store, asked) {
            settle(store, g, &mut wrapper, None, false);
            eprintln!("swofty: {}", report.text());
            return Ok(ExitCode::from(quarantine::REFUSED_EXIT));
        }
        if let Some(status) = wrapper.exited() {
            let err = wrapper.failure(status, "before the game started");
            settle(store, g, &mut wrapper, None, false);
            return Err(err);
        }
        if silence.of(&wrapper.log, &store.root) > wrapper.timeout {
            match &wrapper.child {
                Some(_) => bail!(
                    "the wrapper has not started the game and has written nothing for {} min; it is still running, \
                     and its output is in {}",
                    wrapper.timeout.as_secs() / 60,
                    wrapper.log.display()
                ),
                None => bail!(
                    "the wrapper did not start within {} min; check the Steam launch option (swofty doctor)",
                    wrapper.timeout.as_secs() / 60
                ),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    say("info", &format!("wrapper started, logging to {}", dir.display()));
    let mut relay = wrapper
        .child
        .as_ref()
        .map(|_| Follower::from_end(wrapper.log.clone()));
    let mut follow = Follow::new(&dir, Some((loader, before)), symbols(&game, args.exe.as_deref()));
    install_ctrl_c();
    let mut quiet = false;
    let mut opened: Option<Instant> = None;
    while !dir.join("exit_code").exists() {
        if let Some(url) = swofty_core::ui::home_menu::take_open_request(&game)
            && opened.is_none_or(|at: Instant| at.elapsed() >= OPEN_AGAIN)
        {
            open_in_browser(&url);
            opened = Some(Instant::now());
        }
        if stop_requested() {
            if wrapper.child.is_none() {
                say("info", "stopped following (the game keeps running)");
                return Ok(ExitCode::SUCCESS);
            }
            if quiet {
                say("info", LEFT);
                return Ok(ExitCode::FAILURE);
            }
            quiet = true;
            say(
                "info",
                "stopped following; F1 25 keeps running, and its game files are rolled back when it exits (Ctrl-C again to leave now)",
            );
        }
        if !quiet {
            follow.poll();
            if let Some(r) = relay.as_mut() {
                relay_lines(r);
            }
        }
        if let Some(status) = wrapper.exited()
            && !dir.join("exit_code").exists()
        {
            let err = wrapper.failure(status, "while the game was running");
            settle(store, g, &mut wrapper, relay.as_mut(), false);
            return Err(err);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if !quiet {
        std::thread::sleep(Duration::from_millis(1500));
        follow.poll();
    }
    let summary = follow.summary();
    settle(store, g, &mut wrapper, relay.as_mut(), true);
    if let Some(text) = crate::commands::saves::notice_since(store, asked) {
        say("warning", &format!("saves: {text}"));
    }
    summary?;
    if let Some(failure) = launch::Failure::read(&store.root).filter(|f| f.at >= asked) {
        if relay.is_none() {
            say("warning", &format!("{}: {}", failure.title(), failure.text()));
        }
        say("info", &format!("launch failed: {}", failure.headline()));
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

const LEFT: &str = "left without waiting; the next swofty run or launcher start rolls the game files back";
const SETTLE_TIMEOUT: Duration = Duration::from_secs(600);

const OPEN_AGAIN: Duration = Duration::from_secs(3);

fn open_in_browser(url: &str) {
    let (opener, lead): (&str, &[&str]) = if cfg!(windows) {
        ("rundll32.exe", &["url.dll,FileProtocolHandler"])
    } else {
        ("xdg-open", &[])
    };
    match Command::new(opener)
        .args(lead)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => say("info", &format!("opened {url} in the browser")),
        Err(e) => say("warning", &format!("could not open {url}: {e}")),
    }
}

/// After the game: wait for the wrapper to finish rolling the session back,
/// relaying what it reports, then put back anything it did not (a wrapper
/// killed mid-rollback leaves its journal open).
pub(crate) fn settle(
    store: &Store,
    g: &Game,
    wrapper: &mut WrapperProcess,
    mut relay: Option<&mut Follower>,
    ended: bool,
) {
    if let Some(child) = wrapper.child.as_mut() {
        loop {
            if let Some(r) = relay.as_deref_mut() {
                relay_lines(r);
            }
            if !matches!(child.try_wait(), Ok(None)) {
                break;
            }
            if stop_requested() {
                say("info", LEFT);
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if let Some(r) = relay {
            relay_lines(r);
        }
    }
    let started = Instant::now();
    let lock = loop {
        match SessionLock::try_acquire(&store.root) {
            Ok(Some(lock)) => break lock,
            Ok(None) => {}
            Err(e) => {
                say("info", &format!("cannot check the session lock: {e}"));
                return;
            }
        }
        if wrapper.child.is_none() && game_running_in(&g.dir) {
            say(
                "info",
                "F1 25 is running again; its game files are rolled back when it exits",
            );
            return;
        }
        if started.elapsed() > SETTLE_TIMEOUT || stop_requested() {
            say(
                "info",
                "the session is still rolling back; the next swofty run or launcher start finishes it",
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    match swofty_core::game_folder::recovery::recover(&lock, &store.root, g, Scope::All) {
        Ok(r) => {
            if r.recovered() {
                say("info", "the wrapper ended without rolling the session back");
                crate::session_lock::announce(store, g, &r.lines(), &info);
            } else {
                r.lines().iter().for_each(|l| info(l));
            }
            if let Some(line) = settled(&r, ended) {
                info(line);
            }
        }
        Err(e) => info(&format!("rollback failed, run swofty restore: {e:#}")),
    }
}

pub(super) const UNFINISHED: &str = "not every game file is back yet; the next swofty run or launcher start retries it, or run swofty restore";

/// The last line of the roll back after a session. One that left a file in
/// the game folder never ends on the line that says the files are back.
pub(super) fn settled(r: &Recovered, ended: bool) -> Option<&'static str> {
    if !r.complete() {
        Some(UNFINISHED)
    } else if ended && !r.recovered() {
        Some("session over: its game files are rolled back")
    } else {
        None
    }
}

fn relay_lines(r: &mut Follower) {
    for line in r.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        match line.strip_prefix('[').and_then(|l| l.split_once("] ")) {
            Some((kind, text)) if !kind.contains(' ') => say(kind, text),
            _ => say("wrapper", line.strip_prefix("swofty: ").unwrap_or(line)),
        }
    }
}

/// The wrapper's refusal of this launch (asked at `since`), once written.
pub(crate) fn refused(store: &Store, since: f64) -> Option<quarantine::Report> {
    quarantine::Report::read(&store.root).filter(|r| r.stage == Stage::BeforeLaunch && r.at >= since)
}
