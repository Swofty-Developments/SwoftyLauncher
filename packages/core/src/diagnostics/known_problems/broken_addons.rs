use super::Diagnosis;
use super::crash_record::{Fatal, NEAR_FRAMES, crash_lines, crash_time, stamp};
use super::evidence::Evidence;
use super::wording::{list, name};

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

/// Where F1 25 faults as a session starts on a track whose AI data its AISplineData loader refused: the AI system
/// set-up places the AI cars on the racing line (`Sess_Build_AiSystem` calls `Ai_SplineCursor_PlaceAtDistance`), the
/// line's lane is null, and `Game_TrackSpline_EvalFrameAtDistance` reads its closed-loop flag (`cmp byte
/// [rcx+0x88],0`). The fault and the two return addresses above it, on Steam build 26566308 (1.0.145), then on the
/// KB build.
pub(super) const UNLOADED_AI_LINE: [[&str; 3]; 2] = [
    [
        "F1_25.exe+0x679e34 ",
        "F1_25.exe+0x60cdeb ",
        "F1_25.exe+0x6015c0 ",
    ],
    [
        "F1_25.exe+0x679df4 ",
        "F1_25.exe+0x60cdab ",
        "F1_25.exe+0x601580 ",
    ],
];

pub(super) const UNLOADED_AI_LINE_READ: &str = "reading 0x88";

/// The core's line naming the session it loads, as in `[apply] facts: mode grand_prix, session race, track
/// ashton.lbcgp/lbeach`: an addon's track is its addon id and track code.
pub(super) const FACTS: &str = "] [apply] facts: ";

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

/// Whether F1 25 faulted placing the AI cars on a racing line it could not load: the nearest three frames are one
/// set of [`UNLOADED_AI_LINE`] and the fault read [`UNLOADED_AI_LINE_READ`].
pub(super) fn unloaded_ai_line_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && UNLOADED_AI_LINE.iter().any(|frames| {
            frames
                .iter()
                .enumerate()
                .all(|(i, at)| f.frames.get(i).is_some_and(|l| l.contains(at)))
        })
        && f.record
            .iter()
            .any(|l| l.trim_end().ends_with(UNLOADED_AI_LINE_READ))
}

/// The track of the last session the core loaded before the crash, from its [`FACTS`] line.
pub(super) fn session_track(e: &Evidence, f: &Fatal) -> Option<String> {
    let end = crash_time(e, f);
    e.loader
        .lines()
        .rev()
        .find(|l| l.contains(FACTS) && end.is_none_or(|end| stamp(l).is_none_or(|t| t <= end)))
        .and_then(|l| Some(l.rsplit_once(", track ")?.1.trim().to_owned()))
        .filter(|t| t != "none")
}

pub(super) fn unloaded_ai_line(e: &Evidence, f: &Fatal) -> Diagnosis {
    let track = session_track(e, f);
    let addon = match track.as_deref().and_then(|t| t.rsplit_once('/')) {
        Some(("game", _)) => None,
        Some((id, _)) => Some(id.to_owned()),
        None => (track.is_none() && e.tracks.len() == 1).then(|| e.tracks[0].clone()),
    };
    let author = "Ask its author to fix the AI data: a corner start can't be at gate 0, the first gate of the lap, \
                  and every brake line, corner end, retire line and link has to point at a gate the track has.";
    let (title, what, steps) = match addon.map(|id| name(e, &id)) {
        Some(track) => (
            format!("{track}'s AI data crashed F1 25 as the session started"),
            format!(
                "F1 25 crashed as the session on {track} started, while it put the AI cars on the racing line. The \
                 game could not load {track}'s AI data, so the track had no racing line to put them on."
            ),
            vec![
                format!(
                    "Turn {track} off in your profile, and set any race weekend that uses it back to another track."
                ),
                author.to_owned(),
            ],
        ),
        None => {
            let names: Vec<String> = e.tracks.iter().map(|id| name(e, id)).collect();
            let off = match names.as_slice() {
                [] => "Race on another track, or turn off the addon that changes this track's AI data."
                    .to_owned(),
                _ => format!(
                    "Turn off the track you raced on ({} {} on in your profile), and set any race weekend that uses \
                     it back to another track.",
                    list(&names),
                    if names.len() == 1 { "is" } else { "are" }
                ),
            };
            (
                "A track's AI data crashed F1 25 as the session started".to_owned(),
                "F1 25 crashed as the session started, while it put the AI cars on the racing line. The game could \
                 not load the track's AI data, so the track had no racing line to put them on."
                    .to_owned(),
                vec![off, author.to_owned()],
            )
        }
    };
    Diagnosis {
        id: "track_ai_data_unloaded",
        title,
        what,
        steps,
    }
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
