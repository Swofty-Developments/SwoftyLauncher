use super::Diagnosis;
use super::crash_record::{Fatal, NEAR_FRAMES, crash_lines, crash_time, stamp};
use super::evidence::Evidence;
use super::wording::name;

/// Manual Pit Lane, and where F1 25 crashes when it takes the car as the
/// game returns it to the garage.
pub(super) const PITLANE: &str = "kizi.pitlane";

pub(super) const PITLANE_CRASH: &str = "F1_25.exe+0x389a1d7 ";

/// Aggressive AI (Alternate), and the version of it whose track files crash
/// F1 25 as it loads a session.
pub(super) const AGGRESSIVE_AI: &str = "kizi.f1-25-more-aggresive-ai-2";

pub(super) const AGGRESSIVE_AI_BROKEN: &str = "3.0.0";

/// The three game functions that load those track files, as a stack frame
/// writes them: on the Steam build, then on the EA app's. The crash is a bad
/// free in the game's `RACE` heap or an access violation, in or called from
/// one of them.
pub(super) const AGGRESSIVE_AI_CRASH: [&str; 6] = [
    " fn base+0x67b3f0+",
    " fn base+0x67bf30+",
    " fn base+0x67c680+",
    " fn base+0x67be00+",
    " fn base+0x67c940+",
    " fn base+0x67d090+",
];

/// Skip Cutscenes, whose versions up to [`SKIP_CUTSCENES_LAST_BROKEN`] fail to
/// find a vtable on F1 25 1.0.145 and crash within a minute of the core loading.
pub(super) const SKIP_CUTSCENES: &str = "dromodf.skip-cutscenes";

pub(super) const SKIP_CUTSCENES_LAST_BROKEN: &str = "1.9.4";

pub(super) const SKIP_CUTSCENES_MODULE: &str = "skip_cutscenes.dll";

pub(super) const SKIP_CUTSCENES_VTABLE: &str = "vtable not found";

/// How long before the crash Manual Pit Lane's lines count, and after it.
pub(super) const PITLANE_SECS: f64 = 1.5;

pub(super) const PITLANE_AFTER_SECS: f64 = 0.5;

/// The highest control mode a car really has; Manual Pit Lane prints a
/// garbage one when it read the car as the game moved it.
pub(super) const PITLANE_MODES: u64 = 0xffff;

/// Whether the game crashed as Manual Pit Lane took the car in the pit lane:
/// it decoded the car's control mode to garbage around the crash, or kept
/// control just before the game crashed where this crash happens.
pub(super) fn pitlane_crash(e: &Evidence, f: &Fatal) -> bool {
    if f.module.as_deref() != Some("f1_25.exe") {
        return false;
    }
    let Some(end) = crash_time(e, f) else {
        return false;
    };
    let within = |l: &str, after: f64| stamp(l).is_some_and(|t| t >= end - PITLANE_SECS && t <= end + after);
    let (mut kept, mut garbage) = (false, false);
    for l in e.loader.lines().filter(|l| l.contains("] [pitlane] ")) {
        kept |= l.contains("] [pitlane] pit lane: control kept")
            && !l.contains("the AI keeps the car")
            && within(l, 0.0);
        garbage |= l
            .split_once(", mode ")
            .and_then(|(_, m)| m.split_once(" -> "))
            .and_then(|(_, to)| to.trim().parse::<u64>().ok())
            .is_some_and(|mode| mode > PITLANE_MODES)
            && within(l, PITLANE_AFTER_SECS);
    }
    let at = e
        .loader
        .lines()
        .any(|l| l.contains("[crash] unhandled ") && l.contains(PITLANE_CRASH));
    garbage || (kept && at)
}

/// Whether F1 25 crashed loading the track files of the Aggressive AI
/// (Alternate) version that ships broken ones: that version is on in the
/// profile, and the unhandled exception's address or one of the crash's
/// nearest frames is in [`AGGRESSIVE_AI_CRASH`].
pub(super) fn aggressive_ai_crash(e: &Evidence, f: &Fatal) -> bool {
    let unhandled = crash_lines(&e.loader)
        .into_iter()
        .filter(|l| l.starts_with("unhandled ") && l.contains("reached the top-level filter"));
    e.enabled.iter().any(|id| id == AGGRESSIVE_AI)
        && e.versions.get(AGGRESSIVE_AI).map(String::as_str) == Some(AGGRESSIVE_AI_BROKEN)
        && f.frames
            .iter()
            .take(NEAR_FRAMES)
            .map(String::as_str)
            .chain(unhandled)
            .any(|l| l.contains("F1_25.exe+0x") && AGGRESSIVE_AI_CRASH.iter().any(|at| l.contains(at)))
}

/// Whether F1 25 1.0.145 crashed with a Skip Cutscenes version up to
/// [`SKIP_CUTSCENES_LAST_BROKEN`] on in the profile, in its own module or after
/// it logged that it could not find its vtable.
pub(super) fn skip_cutscenes_crash(e: &Evidence, f: &Fatal) -> bool {
    let broken = e
        .versions
        .get(SKIP_CUTSCENES)
        .is_some_and(|v| semver::Version::parse(v.trim()).is_ok_and(|v| v <= semver::Version::new(1, 9, 4)));
    e.game_build == Some(crate::build_support::BUILD_1_0_145)
        && e.enabled.iter().any(|id| id == SKIP_CUTSCENES)
        && broken
        && (f.module.as_deref() == Some(SKIP_CUTSCENES_MODULE)
            || e.loader.lines().any(|l| l.contains(SKIP_CUTSCENES_VTABLE)))
}

pub(super) fn skip_cutscenes(e: &Evidence) -> Diagnosis {
    let addon = name(e, SKIP_CUTSCENES);
    Diagnosis {
        id: "skip_cutscenes_1_0_145",
        title: format!("{addon} (up to {SKIP_CUTSCENES_LAST_BROKEN}) crashed F1 25 version 1.0.145"),
        what: format!(
            "{addon} (up to {SKIP_CUTSCENES_LAST_BROKEN}) crashes the game on F1 25 version 1.0.145 within a \
             minute of loading."
        ),
        steps: vec!["Turn it off on the Addons page until its author releases an update.".to_owned()],
    }
}

pub(super) fn aggressive_ai(e: &Evidence) -> Diagnosis {
    let addon = name(e, AGGRESSIVE_AI);
    Diagnosis {
        id: "aggressive_ai_alternate",
        title: format!("{addon} {AGGRESSIVE_AI_BROKEN} crashed F1 25 while a session loaded"),
        what: format!(
            "Version {AGGRESSIVE_AI_BROKEN} of the {addon} addon crashes F1 25 while it loads a session, on most \
             tracks. That version has been taken down."
        ),
        steps: vec![
            format!(
                "Uninstall {addon} in the launcher, then install it again from the Addon Browser. That gives you \
                 the version the Addon Browser has now, which doesn't have this crash."
            ),
            format!("Or turn off {addon} in your profile."),
        ],
    }
}

pub(super) fn pitlane(e: &Evidence) -> Diagnosis {
    let addon = e
        .names
        .get(PITLANE)
        .cloned()
        .unwrap_or_else(|| "Manual Pit Lane".to_owned());
    Diagnosis {
        id: "pitlane_garage",
        title: format!("{addon} crashed F1 25 in the pit lane"),
        what: format!(
            "F1 25 crashed as {addon} took over your car in the pit lane while the game was moving the car \
             itself, for example for Return to Garage or a flashback."
        ),
        steps: vec![
            format!("Turn off {addon} in your profile until its author fixes this crash."),
            "If you keep it on, don't use Return to Garage or flashbacks while you are in the pit lane."
                .to_owned(),
        ],
    }
}
