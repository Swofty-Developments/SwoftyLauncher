use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use swofty_core::diagnostics::ea_launch as ea_diag;
use swofty_core::diagnostics::launch;
use swofty_core::offline::{self, Slot};
use swofty_core::platform::app_control::Block;
use swofty_core::platform::run_as_admin::{self as compat, RunAsAdmin};
use swofty_core::session::{LOADER_LOG, LogMark};
use swofty_core::store::Store;

use crate::console::output::say;

const STUB_LOG_WINDOW: i64 = 6 * 3600;

// An antivirus removes a file it flags within a second or two of it being
// written, so the slot is checked this long after staging, before the link.
pub(super) const STUB_SETTLE: Duration = Duration::from_secs(2);

const STUB_POLL: Duration = Duration::from_secs(5);
const ELEVATION_POLL: Duration = Duration::from_secs(1);
const STUB_LOG_READ: u64 = 64 << 10;

/// Before the launch link: an antivirus that removes offline play's file
/// does so within seconds of staging, and an EA app that finds the slot
/// empty repairs F1 25. So the slot is checked a moment after staging, and
/// an empty one stops the session before the EA app sees it.
pub(super) fn check_stub(store: &Store, game: &Path) -> Result<()> {
    let journal = offline::journal_path(&store.root);
    if let Some(left) =
        offline::staged_at(&journal).and_then(|at| (at + STUB_SETTLE).duration_since(SystemTime::now()).ok())
    {
        std::thread::sleep(left.min(STUB_SETTLE));
    }
    if !matches!(offline::slot(&journal), Ok(Some(Slot::Missing))) {
        return Ok(());
    }
    say(
        "ea app",
        "offline play's file is gone from the game folder seconds after staging; not sending the launch link",
    );
    Err(antivirus(store, offline::stub_removed(game)))
}

/// After the launch link: the EA app's proxy logs it when it finds no file
/// to start, or an antivirus blocks it, which ends the wait at once.
pub(super) fn removed_after_link<'a>(store: &'a Store, game: &'a Path) -> impl FnMut() -> Result<()> + 'a {
    let staged = staged_secs(store);
    let mut next = Instant::now();
    move || {
        let Some(at) = staged else {
            return Ok(());
        };
        if Instant::now() < next {
            return Ok(());
        }
        next = Instant::now() + STUB_POLL;
        match offline::stub_blocked(game, &ea_diag::proxy_logs(), at, Some(at)) {
            Some(block) => {
                say(
                    "ea app",
                    "the EA app could not start offline play's file: it is gone from the game folder or blocked",
                );
                Err(antivirus(store, block))
            }
            None => Ok(()),
        }
    }
}

/// After the launch link: the session launcher logs it when Windows would
/// start F1 25 only as administrator, which the EA app, running without
/// administrator rights, never can. That ends the wait at once. Only lines
/// written after the link count.
pub(super) fn refused_after_link<'a>(store: &'a Store, game: &'a Path) -> impl FnMut() -> Result<()> + 'a {
    let log = game.join(LOADER_LOG);
    let before = LogMark::read(&log);
    let from = std::fs::metadata(&log).map_or(0, |m| m.len());
    let mut next = Instant::now();
    move || {
        if Instant::now() < next {
            return Ok(());
        }
        next = Instant::now() + ELEVATION_POLL;
        if !compat::stub_needs_elevation(&written_after(&log, before.as_ref(), from)) {
            return Ok(());
        }
        say(
            "ea app",
            "offline play's session launcher could not start F1 25: Windows starts it only as administrator \
             (error 740)",
        );
        Err(refused(store, compat::game_refused(game)))
    }
}

/// What `log` holds past `from`, its length when `before` was read; all of
/// it when it was started again since.
pub(super) fn written_after(log: &Path, before: Option<&LogMark>, from: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(log) else {
        return String::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let start = if len < from || LogMark::replaced(log, before) {
        0
    } else {
        from
    };
    let mut data = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_ok() {
        f.take(STUB_LOG_READ).read_to_end(&mut data).ok();
    }
    String::from_utf8_lossy(&data).into_owned()
}

fn staged_secs(store: &Store) -> Option<i64> {
    let at = offline::staged_at(&offline::journal_path(&store.root))?;
    Some(at.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

#[derive(Debug, PartialEq)]
pub(super) enum NoGame {
    Antivirus(Block),
    Wait {
        line: &'static str,
        status: &'static str,
    },
}

/// What the session does once the launch link started nothing. An empty
/// slot the EA app's proxy logged as blocked or missing is an antivirus;
/// otherwise the EA app is repairing F1 25 or waiting for Play.
pub(super) fn no_game(slot: Option<Slot>, blocked: impl FnOnce() -> Option<Block>) -> NoGame {
    if slot == Some(Slot::Missing)
        && let Some(block) = blocked()
    {
        return NoGame::Antivirus(block);
    }
    let (line, status) = waiting(slot);
    NoGame::Wait { line, status }
}

/// What the session says and shows while it waits for the EA app after the
/// launch link started nothing. An EA app that took offline play's file out
/// is repairing F1 25 and starts it once the repair is done.
pub(super) fn waiting(slot: Option<Slot>) -> (&'static str, &'static str) {
    match slot {
        Some(Slot::Missing | Slot::Replaced) => (
            "the EA app took offline play's file out to repair F1 25; F1 25 starts without mods after the repair",
            "The EA app is repairing F1 25",
        ),
        _ => (
            "F1 25 did not start from the launch link; press Play on F1 25 in the EA app",
            "Press Play on F1 25 in the EA app",
        ),
    }
}

pub(super) fn stub_block(store: &Store, game: &Path) -> Option<Block> {
    let since = swofty_core::catalog::now_secs() - STUB_LOG_WINDOW;
    offline::stub_blocked(game, &ea_diag::proxy_logs(), since, staged_secs(store))
}

/// Record an antivirus block of offline play's file for the launcher and
/// Send Logs, and end the session with its text.
pub(super) fn antivirus(store: &Store, block: Block) -> anyhow::Error {
    let failure = record(store, launch::Cause::Blocked(block));
    anyhow!("{}", failure.text())
}

/// Record programs Windows won't start because they are set to run as
/// administrator, and end the session with what to untick.
pub(crate) fn refused(store: &Store, r: RunAsAdmin) -> anyhow::Error {
    let failure = record(store, launch::Cause::RunAsAdmin(r));
    anyhow!("{}", failure.text())
}

fn record(store: &Store, cause: launch::Cause) -> launch::Failure {
    let failure = launch::Failure::new(cause);
    say("warning", &format!("{}: {}", failure.title(), failure.text()));
    if let Err(e) = failure.write(&store.root) {
        say("info", &format!("could not record it for the launcher: {e}"));
    }
    failure
}

pub fn note_stub_blocked(store: &Store, game: &Path) -> bool {
    let Some(block) = stub_block(store, game) else {
        return false;
    };
    record(store, launch::Cause::Blocked(block));
    true
}

/// An EA app repair puts EA's anti-cheat launcher over offline play's file,
/// and the game it then starts has no mods. Put the file back for the next
/// Play and tell the player.
pub(super) fn restage(store: &Store, game: &Path) {
    let journal = offline::journal_path(&store.root);
    if matches!(offline::slot(&journal), Ok(None)) {
        return;
    }
    let done = offline::artifacts(&store.root).and_then(|a| offline::restage(&a, &journal));
    match done {
        Ok(Some(Slot::Staged) | None) => {}
        Ok(Some(Slot::Replaced | Slot::Missing)) => {
            if note_stub_blocked(store, game) {
                return;
            }
            let failure = launch::Failure::new(launch::Cause::EaRepaired);
            say("warning", &format!("{}: {}", failure.title(), failure.text()));
            if let Err(e) = failure.write(&store.root) {
                say("info", &format!("could not record it for the launcher: {e}"));
            }
        }
        Ok(Some(Slot::Unknown)) => say(
            "ea app",
            "EAAntiCheat.GameServiceLauncher.exe changed after staging and is neither offline play's nor EA's; \
             left as is",
        ),
        Err(e) => say("ea app", &format!("could not put offline play's file back: {e}")),
    }
}
