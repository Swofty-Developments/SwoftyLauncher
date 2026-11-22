use std::path::Path;
use std::time::{Duration, Instant};

use swofty_core::platform::process::{descends_from, game_processes, stop_process};

use crate::console::output::{info, say};
use crate::signals::{StopPhase, session_stop};

pub(crate) const RESTART_WAIT: Duration = Duration::from_secs(15);
const OTHERS_SETTLE: Duration = Duration::from_secs(3);
const RELAUNCH_SETTLE: Duration = Duration::from_secs(2);
const RELAUNCH_AFTER_FAILURE: Duration = Duration::from_secs(20);
const SHORT_RUN: Duration = Duration::from_secs(60);
const RELAUNCH_CAP: Duration = Duration::from_secs(12 * 3600);

/// After the launched game exits: the EA App build hands a launch it did not
/// get from the EA app back to the EA app, which starts F1 25 again through
/// the game folder's launcher. That copy is not the session's descendant and
/// still holds the staged files, so the session waits for it before the
/// game files are rolled back. A launch that failed quickly gets longer for
/// the relaunch to appear. Another copy's game is not waited for, and only
/// a game that is this copy's is closed.
pub(crate) fn wait_relaunched(game: &Path, dir: &Path, before: &[u32], code: Option<i32>, ran: Duration) {
    let settle = if code.is_some_and(|c| c != 0) && ran < SHORT_RUN {
        RELAUNCH_AFTER_FAILURE
    } else {
        RELAUNCH_SETTLE
    };
    let start = Instant::now();
    let until = start + settle;
    let mut seen: Vec<u32> = Vec::new();
    let mut asked = false;
    loop {
        let running = game_processes();
        let others = swofty_core::platform::process::started_elsewhere(&running, game, before, |pid| {
            descends_from(pid, std::process::id())
        });
        let new: Vec<u32> = others.iter().copied().filter(|p| !seen.contains(p)).collect();
        for pid in new {
            seen.push(pid);
            say(
                "info",
                &format!(
                    "waiting for a second F1 25 process (pid {pid}) started outside this session, usually by the \
                     EA app; the game files are rolled back once it closes"
                ),
            );
        }
        if others.is_empty() && (!seen.is_empty() || Instant::now() >= until) {
            break;
        }
        if start.elapsed() >= RELAUNCH_CAP {
            info(
                "stopped waiting for the second F1 25 process after 12 hours; rolling the game files back now",
            );
            return;
        }
        let phase = session_stop(dir);
        if phase == StopPhase::GiveUp && !others.is_empty() {
            info(
                "the F1 25 started outside this session is still open after Stop; rolling the game files back now",
            );
            return;
        }
        if !asked && phase != StopPhase::Running && !others.is_empty() {
            asked = true;
            info(
                "asked to stop: closing the F1 25 started outside this session, then rolling its game files back",
            );
            for g in running
                .iter()
                .filter(|g| others.contains(&g.pid) && g.is_in(game))
            {
                stop_process(g.pid).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if !seen.is_empty() {
        say("info", "the second F1 25 process has closed");
    }
}

/// A copy of F1 25 still running after `wait`.
pub(crate) fn running_copy(wait: Duration) -> Option<u32> {
    let until = Instant::now() + wait;
    loop {
        match game_processes().first() {
            None => return None,
            Some(g) if Instant::now() >= until => return Some(g.pid),
            Some(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// The copies of F1 25 this session did not start. Ours has exited by now,
/// though its process can take a moment to leave the list; under Proton the
/// session's own processes are its descendants, on Windows the loader names
/// the game's pid.
pub(crate) fn other_copies(ours: Option<u32>) -> Vec<u32> {
    let until = Instant::now() + OTHERS_SETTLE;
    loop {
        let others: Vec<u32> = game_processes()
            .into_iter()
            .filter(|g| Some(g.pid) != ours && !descends_from(g.pid, std::process::id()))
            .map(|g| g.pid)
            .collect();
        if others.is_empty() || Instant::now() >= until {
            return others;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
