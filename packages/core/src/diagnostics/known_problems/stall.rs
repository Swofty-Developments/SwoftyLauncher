use crate::session::SessionRecord;
use crate::store::Store;

use super::Diagnosis;
use super::evidence::{Evidence, evidence};
use super::slow_init::slow_addon_init;
use super::withdrawn::{noted, withdrawn_stall};
use super::wording::name;

/// Addons known to keep F1 25 on Loading... after Press any button, so it never
/// reaches the main menu.
pub(super) const BOOT_STALLS: &[&str] = &["crew8006.bp-ultimate-livery"];

/// The screen F1 25 shows after Press any button while it loads the main menu.
pub(super) const START_SCREEN: &str = "splash_transition";

/// How long a session that ended on that screen had been on it to count as stuck.
pub(super) const BOOT_STALL_SECS: f64 = 45.0;

/// A row of perf.log's timeline: when it started, in seconds after the core
/// attached, and what the game was doing.
pub(super) fn timeline_row(line: &str) -> Option<(f64, &str)> {
    let mut rest = line;
    let mut at = None;
    for i in 0..6 {
        let (field, more) = rest.trim_start().split_once(' ')?;
        match i {
            0 => {
                let mut hms = field.split(':').map(|n| n.parse::<u32>().ok());
                let (h, m, s) = (hms.next()??, hms.next()??, hms.next()??);
                at = Some(f64::from(h * 3600 + m * 60 + s));
            }
            _ if field != "-" && field.parse::<f64>().is_err() => return None,
            _ => {}
        }
        rest = more;
    }
    let what = rest.trim_start();
    let what = what.split("  [").next().unwrap_or(what);
    Some((at?, what.trim_end().trim_end_matches(" (current)")))
}

/// How long the game had been on the start screen when perf.log was last
/// written, when that is the last screen its timeline shows.
pub(crate) fn perf_start_screen(perf: &str) -> Option<f64> {
    let uptime: f64 = perf
        .lines()
        .find_map(|l| l.strip_prefix("uptime: "))?
        .split(' ')
        .next()?
        .parse()
        .ok()?;
    let mut timeline = false;
    let mut last = None;
    for line in perf.lines() {
        if let Some(section) = line.strip_prefix("== ") {
            timeline = section.starts_with("Timeline");
            continue;
        }
        let Some((at, what)) = timeline.then(|| timeline_row(line)).flatten() else {
            continue;
        };
        if let Some(screen) = what.split("; ").filter_map(|w| w.strip_prefix("screen ")).last() {
            last = Some((at, screen));
        }
    }
    let (at, screen) = last?;
    (screen == START_SCREEN).then_some(uptime - at)
}

/// The last screen the perf timeline names.
pub(crate) fn last_screen(perf: &str) -> Option<&str> {
    let mut timeline = false;
    let mut last = None;
    for line in perf.lines() {
        if let Some(section) = line.strip_prefix("== ") {
            timeline = section.starts_with("Timeline");
            continue;
        }
        let Some((_, what)) = timeline.then(|| timeline_row(line)).flatten() else {
            continue;
        };
        if let Some(screen) = what.split("; ").filter_map(|w| w.strip_prefix("screen ")).last() {
            last = Some(screen.split(' ').next().unwrap_or(screen));
        }
    }
    last
}

/// Whether an addon known to stall the boot, or a withdrawn addon that is on, explains the session's stall.
pub(crate) fn addon_stall(session: &SessionRecord, store: &Store) -> bool {
    let e = evidence(session, None, store);
    boot_stall(&e).is_some() || withdrawn_stall(&e).is_some()
}

/// How long the game had been on the start screen when it ended: from the
/// crash report's black box, or else perf.log.
pub(super) fn start_screen_secs(e: &Evidence) -> Option<f64> {
    let screen = e.report.as_deref().and_then(|r| {
        r.lines()
            .find_map(|l| l.trim_start().strip_prefix("last screen"))
            .map(str::trim_start)
    });
    match screen {
        Some(screen) => {
            let rest = screen
                .strip_prefix(START_SCREEN)?
                .trim_start()
                .strip_prefix('(')?;
            rest.split(' ').next()?.parse().ok()
        }
        None => perf_start_screen(e.perf.as_deref()?),
    }
}

/// An addon known to stall the boot, when the game sat on the start screen for
/// [`BOOT_STALL_SECS`] or more and never left it: its name and the seconds.
pub(super) fn boot_stall(e: &Evidence) -> Option<(String, f64)> {
    let secs = start_screen_secs(e).filter(|s| *s >= BOOT_STALL_SECS)?;
    let on = |id: &str| match e.enabled.is_empty() {
        true => e.names.contains_key(id),
        false => e.enabled.iter().any(|on| on == id),
    };
    let id = BOOT_STALLS
        .iter()
        .find(|id| on(id) && !e.builtin.contains(**id))?;
    Some((name(e, id), secs))
}

pub(super) fn minutes(secs: f64) -> String {
    match (secs / 60.0).round() as u64 {
        0 | 1 => format!("{} seconds", secs.round() as u64),
        n => format!("{n} minutes"),
    }
}

/// The cause of a session that ended without a crash, from the first rule that
/// recognises it, and else the addon that held up its start. A diagnosis that
/// is not about the withdrawn addons that are on ends with a step naming them.
pub fn stalled(e: &Evidence) -> Option<Diagnosis> {
    let d = match boot_stall(e) {
        Some((addon, secs)) => known_stall(&addon, secs),
        None => withdrawn_stall(e)
            .or_else(|| stalled_save(e))
            .or_else(|| slow_addon_init(e))?,
    };
    Some(noted(d, e))
}

fn known_stall(addon: &str, secs: f64) -> Diagnosis {
    Diagnosis {
        id: "boot_stall",
        title: format!("{addon} kept F1 25 on the loading screen"),
        what: format!(
            "F1 25 stayed on Loading... after Press any button for {} and never reached the main menu. \
             {addon} is known to cause this.",
            minutes(secs)
        ),
        steps: vec![format!(
            "Turn off {addon} in your profile, then press Play again."
        )],
    }
}

/// What the core logs for each profile load.
pub(super) const PROFILE_LOAD: &str = "[saves] profile load ";

/// A start-screen stall right after the game read the profile save, with no addon known to cause one.
pub(super) fn stalled_save(e: &Evidence) -> Option<Diagnosis> {
    let secs = start_screen_secs(e).filter(|s| *s >= crate::saves::stall::STALL_SECS)?;
    if !e.loader.contains(PROFILE_LOAD) {
        return None;
    }
    Some(Diagnosis {
        id: "stalled_save",
        title: "F1 25 stayed on the loading screen after reading your save".to_owned(),
        what: format!(
            "F1 25 read your profile save, then stayed on Loading... after Press any button for {} and never \
             reached the main menu. A save last played with addon content that is off now (a career, livery or \
             item from an addon) can do this, also from Steam or the EA app without the launcher. When the same \
             save stalls again, the launcher moves it into the career vault before the next Play and puts back \
             the newest version that isn't stuck.",
            minutes(secs)
        ),
        steps: vec![
            "Press Play again. If it stalls a second time, the launcher puts back an earlier version of your save; \
             the stuck one stays on the Saves page."
                .to_owned(),
            "To get the stuck save back as it is, turn on the addons you last played it with, then restore it from \
             the Saves page."
                .to_owned(),
        ],
    })
}
