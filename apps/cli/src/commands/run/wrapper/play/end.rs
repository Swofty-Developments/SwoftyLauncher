use std::path::Path;
use std::process::ExitStatus;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use swofty_core::diagnostics::launch;
use swofty_core::platform::process::{descends_from, game_processes, stop_process};
use swofty_core::session::{STOP_FILE, SessionEnd, SessionRecord};
use swofty_core::store::Store;

use crate::commands::run::wrapper::copies::other_copies;
use crate::console::output::{info, say};
use crate::signals::ending;
use crate::time::now_secs;

/// How the game's run ended, as the wait saw it.
pub(super) struct Played {
    pub(super) code: Option<i32>,
    pub(super) unread: String,
    pub(super) via_ea: bool,
}

/// Settle how the session ended, record it in the session folder, and stop
/// whatever of the game is still running under the wrapper.
pub(super) fn end_session(
    store: &Store,
    game: &Path,
    dir: &Path,
    played: Played,
    launched: SystemTime,
    usage: Option<swofty_core::online::server::usage::Session>,
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
        if let Some(d) = swofty_core::diagnostics::known_problems::stall(&record, Some(game), store)
            .or_else(|| swofty_core::diagnostics::known_problems::tyres(&record, Some(game), store))
            .or_else(|| swofty_core::diagnostics::known_problems::logos(&record, Some(game), store))
        {
            say("warning", &format!("known problem {}: {}", d.id, d.title));
        }
        return;
    }
    match swofty_core::diagnostics::known_problems::crash(&record, Some(game), store) {
        Some(d) => say("crash", &format!("known problem {}: {}", d.id, d.title)),
        None => say("crash", "no known problem matches this crash"),
    }
}

pub(super) const EA_UNREAD: &str =
    "the session could not read the exit code of the F1 25 the EA app started; the [ea app] lines say why";

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
