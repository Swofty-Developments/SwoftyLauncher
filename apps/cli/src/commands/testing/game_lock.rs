use std::path::Path;
use std::time::{Duration, Instant};

use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::platform::process::{game_processes, stop_process};
use swofty_core::session::request_stop;
use swofty_core::store::Store;
use swofty_core::testsuite::lock::GameLock;

use crate::commands::testing::run::Stop;
use crate::commands::testing::say;
use crate::time::now_secs;

pub(crate) fn take_lock(dir: &Path, owner: &str, wait: Duration) -> std::result::Result<GameLock, Stop> {
    let running = || !game_processes().is_empty();
    match GameLock::acquire(dir, owner, wait, LOCK_POLL, |b| {
        say(&format!("waiting for the game: {}", b.text(running())))
    }) {
        Ok(Ok(lock)) => Ok(lock),
        Ok(Err(busy)) => Err(Stop::Busy(busy.text(running()))),
        Err(e) => Err(Stop::Infra(format!(
            "{e}; set SWOFTY_GAME_LOCK or --lock to a folder this user can create"
        ))),
    }
}

const LOCK_POLL: Duration = Duration::from_secs(5);
const CLOSE_WAIT: Duration = Duration::from_secs(120);
const ROLLBACK_WAIT: Duration = Duration::from_secs(600);

/// A game running without the lock is a player's: wait for it to close, or
/// with `--take-over`, close it by its exact pid. Then wait for its session
/// to put the game files back.
pub(crate) fn free_the_game(store: &Store, take_over: bool, wait: Duration) -> std::result::Result<(), Stop> {
    let running = game_processes();
    if let Some(first) = running.first() {
        if take_over {
            request_stop(&store.root);
            for g in &running {
                say(&format!("--take-over: closing F1 25 (pid {})", g.pid));
                stop_process(g.pid).ok();
            }
            if !until(CLOSE_WAIT, || game_processes().is_empty()) {
                return Err(Stop::Infra(format!(
                    "F1 25 (pid {}) did not close within {} s",
                    first.pid,
                    CLOSE_WAIT.as_secs()
                )));
            }
        } else {
            say(&format!(
                "F1 25 is running (pid {}); waiting for it to close (--take-over closes it)",
                first.pid
            ));
            if !until(wait, || game_processes().is_empty()) {
                return Err(Stop::Busy(format!(
                    "F1 25 is running (pid {}); close it, or pass --take-over to close it",
                    first.pid
                )));
            }
        }
    }
    if SessionLock::held_elsewhere(&store.root) {
        say("waiting for the last session to put its game files back");
        let limit = if take_over {
            ROLLBACK_WAIT
        } else {
            wait.min(ROLLBACK_WAIT)
        };
        if !until(limit, || !SessionLock::held_elsewhere(&store.root)) {
            let why =
                "the last session is still putting the game files back (its session lock is held)".to_owned();
            return Err(if take_over {
                Stop::Infra(why)
            } else {
                Stop::Busy(why)
            });
        }
    }
    Ok(())
}

pub(crate) fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(500).min(limit.saturating_sub(start.elapsed())));
    }
}

/// Local `HH:MM` for the lock's owner line.
pub(crate) fn hh_mm() -> String {
    #[cfg(target_os = "linux")]
    {
        let t = now_secs() as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: localtime_r writes only into `tm`.
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
        }
    }
    let s = now_secs() as u64 % 86_400;
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}
