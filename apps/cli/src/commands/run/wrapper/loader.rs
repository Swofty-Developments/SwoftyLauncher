use std::path::Path;
use std::time::Instant;

use swofty_core::diagnostics::quarantine::{self, Stage};
use swofty_core::install::PROXY;
use swofty_core::store::Store;

use crate::console::output::{info, say};

/// Before the game starts: the DLLs the session placed are still there with
/// the bytes written, on a first look and again a moment later, since a
/// scanner often takes a file only once it is closed. Whether the session
/// placed version.dll (so the loader should attach), or the report on the
/// missing ones, which refuses the launch.
pub(crate) fn check_loader(
    store: &Store,
    game: &Path,
    installed: Option<Instant>,
) -> std::result::Result<bool, quarantine::Report> {
    let dlls = quarantine::placed_dlls(&store.root).unwrap_or_default();
    let loader = dlls.iter().any(|(rel, _)| rel.eq_ignore_ascii_case(PROXY));
    let mut files = quarantine::damaged_install();
    let first = quarantine::missing(game, &dlls);
    if let Some(at) = installed.filter(|_| !dlls.is_empty()) {
        std::thread::sleep(quarantine::RECHECK_AFTER.saturating_sub(at.elapsed()));
    }
    for rel in first.into_iter().chain(quarantine::missing(game, &dlls)) {
        if !files.contains(&rel) {
            files.push(rel);
        }
    }
    if files.is_empty() {
        return Ok(loader);
    }
    let report = quarantine::Report::new(Stage::BeforeLaunch, files, game);
    report_blocked(store, &report);
    Err(report)
}

/// The game is running but the loader never attached: say so, and whether
/// version.dll is gone from the game folder by now or what else the session
/// found keeping it out.
pub(crate) fn loader_missed(store: &Store, game: &Path, cause: Option<quarantine::Cause>) {
    let mut report = quarantine::Report::new(Stage::InGame, vec![PROXY.to_owned()], game);
    report.removed = !game.join(PROXY).is_file();
    report.cause = cause.filter(|_| !report.removed).map(Box::new);
    say(
        "loader",
        &format!(
            "  found: {}",
            match &report.cause {
                Some(c) => c.what(),
                None if report.removed => format!("{PROXY} was removed from the game folder"),
                None => "nothing that explains it".into(),
            }
        ),
    );
    report_blocked(store, &report);
}

fn report_blocked(store: &Store, report: &quarantine::Report) {
    say("warning", &format!("{}: {}", report.title(), report.text()));
    if let Err(e) = report.write(&store.root) {
        info(&format!("could not record it for the launcher: {e}"));
    }
}

/// The loader attached but could not log in the game folder: say why, and
/// what that breaks.
pub(crate) fn note_refused(fallback: &Path) {
    let Some(why) = std::fs::read(fallback)
        .ok()
        .and_then(|b| quarantine::refused(&String::from_utf8_lossy(&b)))
    else {
        return;
    };
    say(
        "loader",
        &format!(
            "version.dll attached, but its log could not go in the game folder: {why}. The log is in {}",
            fallback.display()
        ),
    );
    say("warning", quarantine::refused_text());
}
