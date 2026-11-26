use super::Diagnosis;
use super::crash_record::{Fatal, NEAR_FRAMES, crash_lines, crash_time, stamp};
use super::evidence::Evidence;
use super::wording::{SEND_LOGS, list, name};
use crate::build_support::{BUILD_1_0_145, BUILD_1_0_145_EA};

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

/// Terminal Damage, whose experimental suspension damage tears a wheel off by pushing the game's own suspension
/// damage past its wheel detach limit.
pub(super) const TERMINAL_DAMAGE: &str = "nick.terminal-damage-probe";

/// Its line for that, as in `[td-probe] suspension: your car FR wheel torn off: the game's damage 0.00 -> 1.02`.
pub(super) const TD_WHEEL_OFF: [&str; 2] = ["] [td-probe] suspension: your car ", " wheel torn off"];

/// How long before the crash that line counts.
pub(super) const TD_WHEEL_OFF_SECS: f64 = 1.0;

/// How long before a crash in the game's keyframe samplers that line counts: the UI animation player faulted 43 ms and
/// 4.8 s after it in the two reports that had it.
pub(super) const TD_WHEEL_OFF_ANIM_SECS: f64 = 10.0;

/// Where the game's tyre curve samplers read through the node their bracket search returns, which is the curve
/// tree's head when the key is not a number: `Car_Tyre_SampleCurveSet2KeyLerp+0xbc`,
/// `Car_Tyre_Curve2D_SampleByBinarySearchY+0x54`, where that sampler's keyframe search reads a point of the curve it
/// took from the head, and `Car_Tyre_SampleWearCurve2DAlt+0xa1`, on Steam build 26566308 (1.0.145), the KB build,
/// EA 1.0.141 and EA 1.0.145.
pub(super) const TYRE_CURVE_NAN: [&str; 12] = [
    "F1_25.exe+0x2e7e5ec ",
    "F1_25.exe+0x2e7e9f4 ",
    "F1_25.exe+0x2e7ec21 ",
    "F1_25.exe+0x2e7c05c ",
    "F1_25.exe+0x2e7c464 ",
    "F1_25.exe+0x2e7c691 ",
    "F1_25.exe+0x2e6fc5c ",
    "F1_25.exe+0x2e70064 ",
    "F1_25.exe+0x2e70291 ",
    "F1_25.exe+0x2e721ec ",
    "F1_25.exe+0x2e725f4 ",
    "F1_25.exe+0x2e72821 ",
];

/// Where the game's keyframe samplers read a track's bucket table at the index a time that is not a number turns
/// into (0x80000000, 4 GB below the table): the 3-float, 4-float and rotation per-track samplers, then the inline
/// samplers of the B1, B4 and L1 packets, each on Steam build 26566308 (1.0.145), the KB build, EA 1.0.141 and EA
/// 1.0.145.
pub(super) const ANIM_CLIP_NAN: [&str; 24] = [
    "F1_25.exe+0x3a3d54b ",
    "F1_25.exe+0x3a3e1a2 ",
    "F1_25.exe+0x3a3e5ff ",
    "F1_25.exe+0x3a3af7b ",
    "F1_25.exe+0x3a3bbd2 ",
    "F1_25.exe+0x3a3c02f ",
    "F1_25.exe+0x3a27f9b ",
    "F1_25.exe+0x3a28bf2 ",
    "F1_25.exe+0x3a2904f ",
    "F1_25.exe+0x3a2a56b ",
    "F1_25.exe+0x3a2b1c2 ",
    "F1_25.exe+0x3a2b61f ",
    "F1_25.exe+0x3a3bfaa ",
    "F1_25.exe+0x3a3c5da ",
    "F1_25.exe+0x3a3cc48 ",
    "F1_25.exe+0x3a399da ",
    "F1_25.exe+0x3a3a00a ",
    "F1_25.exe+0x3a3a678 ",
    "F1_25.exe+0x3a269fa ",
    "F1_25.exe+0x3a2702a ",
    "F1_25.exe+0x3a27698 ",
    "F1_25.exe+0x3a28fca ",
    "F1_25.exe+0x3a295fa ",
    "F1_25.exe+0x3a29c68 ",
];

/// Where F1 25 faults matching a track's marshal lights to its marshal light zones at a session enter or restart:
/// `Sess_MarshalLights_AssignToZones` erased the first light of its list, which is in no zone, and stores the lap
/// fraction through the 8 bytes before the list (`vmovss [rax+0x34],xmm7`). On Steam build 26566308 (1.0.145), the KB
/// build, EA 1.0.141 and EA 1.0.145.
pub(super) const MARSHAL_LIGHTS_WALK: [&str; 4] = [
    "F1_25.exe+0x2c794d2 ",
    "F1_25.exe+0x2c76f42 ",
    "F1_25.exe+0x2c6ac92 ",
    "F1_25.exe+0x2c6d222 ",
];

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
        .any(|l| l.contains(super::crash_record::FATAL_LINE) && l.contains(PITLANE_CRASH));
    garbage || (kept && at)
}

/// Whether F1 25 faulted at one of `sites` in its own code: the unhandled exception's address or the crash's nearest
/// frame is one of them.
fn game_fault_at(e: &Evidence, f: &Fatal, sites: &[&str]) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && crash_lines(&e.loader)
            .into_iter()
            .filter(|l| l.starts_with("unhandled ") && l.contains("reached the top-level filter"))
            .chain(f.frames.first().map(String::as_str))
            .any(|l| sites.iter().any(|at| l.contains(at)))
}

/// Whether F1 25 faulted in its own tyre curve lookup: the unhandled exception's address or the crash's nearest
/// frame is in [`TYRE_CURVE_NAN`].
pub(super) fn tyre_curve_crash(e: &Evidence, f: &Fatal) -> bool {
    game_fault_at(e, f, &TYRE_CURVE_NAN)
}

/// Whether F1 25 faulted in its own keyframe sampler: the unhandled exception's address or the crash's nearest frame
/// is in [`ANIM_CLIP_NAN`].
pub(super) fn anim_clip_crash(e: &Evidence, f: &Fatal) -> bool {
    game_fault_at(e, f, &ANIM_CLIP_NAN)
}

/// Whether F1 25 faulted matching a track's marshal lights to its zones: the unhandled exception's address or the
/// crash's nearest frame is in [`MARSHAL_LIGHTS_WALK`].
pub(super) fn marshal_lights_crash(e: &Evidence, f: &Fatal) -> bool {
    game_fault_at(e, f, &MARSHAL_LIGHTS_WALK)
}

/// Whether Terminal Damage tore a wheel off the player's car less than `secs` before the crash.
fn td_wheel_off_before(e: &Evidence, f: &Fatal, secs: f64) -> bool {
    let Some(end) = crash_time(e, f) else {
        return false;
    };
    e.loader.lines().any(|l| {
        l.contains(TD_WHEEL_OFF[0])
            && l.contains(TD_WHEEL_OFF[1])
            && stamp(l).is_some_and(|t| t <= end && end - t < secs)
    })
}

/// Whether F1 25 faulted in its own tyre curve lookup less than [`TD_WHEEL_OFF_SECS`] after Terminal Damage tore a
/// wheel off the player's car.
pub(super) fn td_wheel_off_crash(e: &Evidence, f: &Fatal) -> bool {
    tyre_curve_crash(e, f) && td_wheel_off_before(e, f, TD_WHEEL_OFF_SECS)
}

/// Whether F1 25 faulted in its own keyframe sampler less than [`TD_WHEEL_OFF_ANIM_SECS`] after Terminal Damage tore
/// a wheel off the player's car.
pub(super) fn td_wheel_off_anim_crash(e: &Evidence, f: &Fatal) -> bool {
    anim_clip_crash(e, f) && td_wheel_off_before(e, f, TD_WHEEL_OFF_ANIM_SECS)
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

/// Whether the game is the Steam build of F1 25 1.0.145.
pub(super) fn steam_1_0_145(e: &Evidence) -> bool {
    e.game_build == Some(BUILD_1_0_145)
}

/// Whether the game is the EA app build of F1 25 1.0.145.
pub(super) fn ea_1_0_145(e: &Evidence) -> bool {
    e.game_build == Some(BUILD_1_0_145_EA)
}

/// Whether F1 25 1.0.145 crashed with a Skip Cutscenes version up to
/// [`SKIP_CUTSCENES_LAST_BROKEN`] on in the profile, in its own module or after
/// it logged that it could not find its vtable.
pub(super) fn skip_cutscenes_crash(e: &Evidence, f: &Fatal) -> bool {
    let broken = e
        .versions
        .get(SKIP_CUTSCENES)
        .is_some_and(|v| semver::Version::parse(v.trim()).is_ok_and(|v| v <= semver::Version::new(1, 9, 4)));
    steam_1_0_145(e)
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

/// The name of the addon track the crashed session ran: the addon of the track on the core's last [`FACTS`] line
/// before the crash, or the one track addon on in the profile when there is no such line. None for a stock track and
/// when it can't be told.
fn session_track_name(e: &Evidence, f: &Fatal) -> Option<String> {
    let track = session_track(e, f);
    let addon = match track.as_deref().and_then(|t| t.rsplit_once('/')) {
        Some(("game", _)) => None,
        Some((id, _)) => Some(id.to_owned()),
        None => (track.is_none() && e.tracks.len() == 1).then(|| e.tracks[0].clone()),
    };
    addon.map(|id| name(e, &id))
}

/// The step that turns the crashed session's track off: `track` by name, else the track addons on in the profile,
/// else `stock` for a stock track.
fn track_off_step(e: &Evidence, track: Option<&str>, stock: &str) -> String {
    if let Some(track) = track {
        return format!(
            "Turn {track} off in your profile, and set any race weekend that uses it back to another track."
        );
    }
    let names: Vec<String> = e.tracks.iter().map(|id| name(e, id)).collect();
    match names.as_slice() {
        [] => stock.to_owned(),
        _ => format!(
            "Turn off the track you raced on ({} {} on in your profile), and set any race weekend that uses it back \
             to another track.",
            list(&names),
            if names.len() == 1 { "is" } else { "are" }
        ),
    }
}

pub(super) fn unloaded_ai_line(e: &Evidence, f: &Fatal) -> Diagnosis {
    let track = session_track_name(e, f);
    let author = "Ask its author to fix the AI data: a corner start can't be at gate 0, the first gate of the lap, \
                  and every brake line, corner end, retire line and link has to point at a gate the track has.";
    let off = track_off_step(
        e,
        track.as_deref(),
        "Race on another track, or turn off the addon that changes this track's AI data.",
    );
    let (title, what) = match &track {
        Some(track) => (
            format!("{track}'s AI data crashed F1 25 as the session started"),
            format!(
                "F1 25 crashed as the session on {track} started, while it put the AI cars on the racing line. The \
                 game could not load {track}'s AI data, so the track had no racing line to put them on."
            ),
        ),
        None => (
            "A track's AI data crashed F1 25 as the session started".to_owned(),
            "F1 25 crashed as the session started, while it put the AI cars on the racing line. The game could not \
             load the track's AI data, so the track had no racing line to put them on."
                .to_owned(),
        ),
    };
    Diagnosis {
        id: "track_ai_data_unloaded",
        title,
        what,
        steps: vec![off, author.to_owned()],
    }
}

pub(super) fn marshal_lights(e: &Evidence, f: &Fatal) -> Diagnosis {
    let track = session_track_name(e, f);
    let author = "Ask its author to give the track a marshal light zone (marshal_light_zone in its track markup) for \
                  every part of the lap that has marshal lights.";
    let off = track_off_step(
        e,
        track.as_deref(),
        "Race on another track, or turn off the addon that changes this track's marshal lights.",
    );
    let (title, what) = match &track {
        Some(track) => (
            format!("{track}'s marshal lights crashed F1 25 as the session started"),
            format!(
                "F1 25 crashed as the session on {track} started, while it matched the track's marshal lights to its \
                 marshal light zones. {track} has marshal lights outside every marshal light zone, and the game's own \
                 code crashes on them."
            ),
        ),
        None => (
            "A track's marshal lights crashed F1 25 as the session started".to_owned(),
            "F1 25 crashed as the session started, while it matched the track's marshal lights to its marshal light \
             zones. The track has marshal lights outside every marshal light zone, and the game's own code crashes \
             on them."
                .to_owned(),
        ),
    };
    Diagnosis {
        id: "track_marshal_lights",
        title,
        what,
        steps: vec![off, author.to_owned()],
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

fn terminal_damage(e: &Evidence) -> String {
    e.names
        .get(TERMINAL_DAMAGE)
        .cloned()
        .unwrap_or_else(|| "Terminal Damage".to_owned())
}

pub(super) fn td_wheel_off(e: &Evidence) -> Diagnosis {
    let addon = terminal_damage(e);
    Diagnosis {
        id: "terminal_damage_wheel_off",
        title: format!("F1 25 crashed right after {addon} tore a wheel off your car"),
        what: format!(
            "{addon}'s experimental suspension damage tore a wheel off your car, and F1 25 crashed a moment later in \
             its own tyre wear estimate for that car."
        ),
        steps: vec![format!(
            "On the Addons page, turn off Suspension damage from hits (experimental) and Suspension damage for AI \
             cars (experimental) in {addon}'s options, or turn {addon} off, then press Play."
        )],
    }
}

pub(super) fn tyre_curve_nan() -> Diagnosis {
    Diagnosis {
        id: "tyre_curve_nan",
        title: "F1 25 crashed in its own tyre wear estimate".to_owned(),
        what: "F1 25's own tyre wear estimate for a car hit an invalid number, and the game crashed. This is a fault \
               in the game, and no setting of yours fixes it."
            .to_owned(),
        steps: vec![SEND_LOGS.to_owned()],
    }
}

pub(super) fn td_wheel_off_anim(e: &Evidence) -> Diagnosis {
    let addon = terminal_damage(e);
    Diagnosis {
        id: "terminal_damage_wheel_off_anim",
        title: format!("F1 25 crashed in its own animation code after {addon} tore a wheel off your car"),
        what: format!(
            "{addon}'s experimental suspension damage tore a wheel off your car, and shortly after F1 25's own \
             animation code hit an invalid number and the game crashed. The fault is in the game's code."
        ),
        steps: vec![
            SEND_LOGS.to_owned(),
            format!(
                "If it happens again, turn off Suspension damage from hits (experimental) and Suspension damage for \
                 AI cars (experimental) in {addon}'s options on the Addons page, then press Play."
            ),
        ],
    }
}

pub(super) fn anim_clip_nan() -> Diagnosis {
    Diagnosis {
        id: "anim_clip_nan",
        title: "F1 25 crashed in its own animation code".to_owned(),
        what: "F1 25's own animation code hit an invalid number, and the game crashed. This is a fault in the game, \
               and no setting of yours fixes it."
            .to_owned(),
        steps: vec![SEND_LOGS.to_owned()],
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
