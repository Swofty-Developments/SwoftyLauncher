use std::collections::BTreeMap;

use super::Diagnosis;
use super::crash_record::{
    Fatal, NEAR_FRAMES, crash_lines, crash_time, crashed_session, game_function, stamp,
};
use super::evidence::Evidence;
use super::weekend::Weekend;
use super::wording::{list, name};

/// The entrant list search by name (fn 0x141738eb0) that runs off the end of the
/// game's loaded lists when the name is missing.
pub(super) const ENTRANT_LIST_SEARCH: &str = "F1_25.exe+0x1738f30 ";

/// Where F1 25 sets up the track for a session it loads.
pub(super) const SETUP_TRACK: &str = "Sess_Loader_SetupTrackAssets";

/// The fault in the career hub's set-up: the read of the first record of a calendar event that matched nothing.
pub(super) const SEQUENCER_RECORD: &str = "Game_CareerEventsSequencer_Initialise+0x1020";

/// The fault in the career hub's set-up when a driver of the player's team is not in the game's driver list: the
/// driver morph data read of the driver the lookup did not find.
pub(super) const MORPH_DRIVER: &str = "Game_CareerEventResourceManager_AddDriverMorphData+0x82";

/// The frame that called the game's driver name fill for a race's retirement message: the return address of the call
/// whose driver the lookup did not find.
pub(super) const RETIREMENT_NAME: &str = "Sess_InGameRace_Update+0x77e";

/// What the driver name fill reads for a missing driver: the composed name pointer of a null driver's name holder.
pub(super) const RETIREMENT_READ: &str = "reading 0x1f0";

/// The fault in the weekend list builder of the Team HQ screens: the read of the node of a round's track record.
pub(super) const WEEKENDS_TRACK_NODE: &str = "Career_Weekends_BuildResultNodes+0x4da";

/// The fault in the session loader's weekend set-up when the current season of the career's current championship has
/// no weekend at its current round: the read of the null weekend's session list (`mov r12, [r15+0x250]`), by name and
/// as the Steam 1.0.145 frame writes it unnamed.
pub(super) const NO_WEEKEND_LOAD: [&str; 2] = [
    "Sess_WeekendManager_PrepareTrackLoad+0x11b",
    " fn base+0x240b9b0+0x11b",
];
pub(super) const NO_WEEKEND_READ: &str = "reading 0x250";
/// Custom Calendar, whose picker (F9) replaces the career's calendar when it imports its rounds into the game's track
/// select screen, and the line it logs for that import.
pub(super) const CUSTOM_CALENDAR: &str = "majorpayne8779.progressive-season-screen";
pub(super) const CALENDAR_IMPORT: &str = "[Progressive season] Imported ";
/// F2 Revamp, whose career starts in the F2 championship.
pub(super) const F2_REVAMP: &str = "miguelvf3.f2-revamp";
/// The game function that reads a car's racing line, which a car with no line of its own faults in.
pub(super) const LINE_CACHE: &str = "Ai_LineCache_UpdateIfChanged+";

/// Where F1 25's UDP telemetry set-up faults on a vehicle that is null, `mov r15, [r15+0x590]` after the car's vehicle
/// component (`[entry+0x28]`) read as 0, on the Steam 1.0.145 build and the older one.
pub(super) const NULL_VEHICLE_AT: [&str; 2] = [" fn base+0x2d01210+0x116a", " fn base+0x2cfec80+0x116a"];
pub(super) const NULL_VEHICLE_READ: &str = "reading 0x590";

/// The core's session trace at the vehicle set-up, whose `participants: game manager N` counts the cars of the grid.
pub(super) const VEHICLE_SETUP: &str = "session trace Sess_Setup_VehicleComponents ";
pub(super) const GAME_MANAGER: &str = "participants: game manager ";

/// The grid size the game builds its per-car tables for, and the game manager count from which a grid is over it
/// (a stock career logs 23 for its 22 cars).
pub(super) const GRID_CARS: usize = 22;
pub(super) const OVER_GRID_MANAGER: usize = 24;

/// What the core logs for a car that is the third of its team.
pub(super) const THIRD_CAR: &str = "is the third car of its garage";

/// How long after a career loaded a crash counts as loading its next session.
pub(super) const CAREER_LOAD_SECS: f64 = 10.0;

/// The core's line for a session that ended moments after it started, with the career hub opened inside it: the
/// game's calendar check sent the career to Team HQ as the session opened.
pub(super) const QUICK_END: &str = "] [career] session ended ";
pub(super) const HUB_INSIDE: &str = "the career hub opened ";
/// The core's calendar line as the hub opens, and the item the calendar check picks in it.
pub(super) const HUB_OPENED: &str = "] [career] hub opened: ";
/// The core's season break fix, as the hub opens.
pub(super) const BREAK_FIX: &str = "] [career] season break over race weekend: ";
/// Where F1 25 faults after such a session closes: `cmp qword [rdi+0x9c58],0` with no object found (KB build
/// 0x141f8a25c, after `xor edi,edi` at 0x141f8a253). The Steam build 26566308 names no symbol there.
pub(super) const CLOSED_SESSION_AT: [&str; 2] = [
    "Game_Read_OverridesCarMonitorMaximisedConfig_2+0xe6c",
    " fn base+0x1f8aff0+0xe6c",
];
pub(super) const CLOSED_SESSION_READ: &str = "reading 0x9c58";
/// How long before the crash the session's [`QUICK_END`] line counts.
pub(super) const QUICK_END_SECS: f64 = 10.0;

pub(super) fn entrant_list_crash(e: &Evidence, f: &Fatal) -> bool {
    let at = |l: &str| l.contains("unhandled ") && l.contains(ENTRANT_LIST_SEARCH);
    f.module.as_deref() == Some("f1_25.exe")
        && (crash_lines(&e.loader).into_iter().any(at) || e.report.iter().flat_map(|r| r.lines()).any(at))
}

/// Whether F1 25 crashed setting up the track for a career's session: the
/// nearest game function the stack names is [`SETUP_TRACK`], and the career
/// loaded at most [`CAREER_LOAD_SECS`] before.
pub(super) fn career_track_crash(e: &Evidence, f: &Fatal) -> bool {
    let setup = f.frames.iter().take(NEAR_FRAMES).find_map(|l| game_function(l)) == Some(SETUP_TRACK);
    if !setup || f.module.as_deref() != Some("f1_25.exe") {
        return false;
    }
    let Some(end) = crash_time(e, f) else {
        return false;
    };
    e.loader
        .lines()
        .rev()
        .filter(|l| l.contains("] [career] career.loaded: ") || l.contains("] [career] career.unloaded "))
        .filter_map(|l| Some((stamp(l)?, l)))
        .find(|(t, _)| *t <= end)
        .is_some_and(|(t, l)| l.contains("career.loaded: ") && end - t <= CAREER_LOAD_SECS)
}

/// Whether F1 25 faulted reading the first calendar event record while a career session was entered: the
/// nearest frame is [`SEQUENCER_RECORD`].
pub(super) fn sequencer_record_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe") && f.frames.first().is_some_and(|l| l.contains(SEQUENCER_RECORD))
}

/// Whether F1 25 faulted in the AI's racing line update, the crash of a third car in a team that has no line.
pub(super) fn third_car_line_crash(e: &Evidence, f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && f.frames.first().is_some_and(|l| l.contains(LINE_CACHE))
        && e.loader.contains(THIRD_CAR)
}

/// The enabled addons that add drivers or cars to a team.
pub(super) fn third_car_addons(e: &Evidence) -> Vec<String> {
    e.enabled
        .iter()
        .filter(|id| {
            let id = id.to_ascii_lowercase();
            id.contains("more-driver") || id.contains("third") || id.contains("extra-car")
        })
        .map(|id| name(e, id))
        .collect()
}

pub(super) fn third_car_line(e: &Evidence) -> Diagnosis {
    let addons = third_car_addons(e);
    let by = match addons.as_slice() {
        [] => "a team with three cars".to_owned(),
        _ => format!("{} giving a team a third car", list(&addons)),
    };
    let off = match addons.as_slice() {
        [] => "Go back to two cars in every team of My Team, or turn off the addon that adds the third car."
            .to_owned(),
        _ => format!(
            "Turn off {}, or go back to two cars in every team of My Team.",
            list(&addons)
        ),
    };
    Diagnosis {
        id: "third_car_line",
        title: "A third car in a team crashed F1 25".to_owned(),
        what: format!(
            "F1 25 crashed a moment after the session started, while the AI steered the cars. The cause is {by}: \
             the game gives the AI's racing line and tables to two cars per team, and a third car has none to read."
        ),
        steps: vec![off, "Press Play again.".to_owned()],
    }
}

/// The enabled addons that change a career's calendar: calendar and schedule packages, and DB Editor packages.
pub(super) fn calendar_addons(e: &Evidence) -> Vec<String> {
    e.enabled
        .iter()
        .filter(|id| {
            let id = id.to_ascii_lowercase();
            id.contains("calend") || id.contains("schedule") || id.starts_with("dbedit.")
        })
        .map(|id| name(e, id))
        .collect()
}

pub(super) fn calendar_event_missing(e: &Evidence) -> Diagnosis {
    let calendar = calendar_addons(e);
    let (title, cause) = match calendar.as_slice() {
        [] => (
            "Your career's calendar has a round F1 25 can't find".to_owned(),
            "A round of your career's calendar is not in the game's calendar, usually because an addon changed the \
             calendar or a track it uses is missing."
                .to_owned(),
        ),
        _ => (
            format!("{} changed your career's calendar", list(&calendar)),
            format!(
                "{} changed the rounds of the calendar, and a round of your career now points at a track or event \
                 the game can't find.",
                list(&calendar)
            ),
        ),
    };
    let off = match calendar.as_slice() {
        [] => "Turn off the addon that changes your calendar (a calendar or DB Editor package), or one that adds a track the calendar uses."
            .to_owned(),
        _ => format!(
            "Turn off {}, or make sure every track it points the calendar at is installed and on.",
            list(&calendar)
        ),
    };
    Diagnosis {
        id: "calendar_event_missing",
        title,
        what: format!(
            "F1 25 crashed as your career's first session loaded, while it set up the career hub for that round. {cause}"
        ),
        steps: vec![off, "Press Play again.".to_owned()],
    }
}

/// Whether F1 25 faulted reading the driver it passes to the career hub's driver morph data while a career session
/// was entered: the nearest frame is [`MORPH_DRIVER`].
pub(super) fn morph_driver_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe") && f.frames.first().is_some_and(|l| l.contains(MORPH_DRIVER))
}

/// Whether F1 25 faulted naming the driver of a car that retired from a race: the frame after the nearest is
/// [`RETIREMENT_NAME`] and the fault read [`RETIREMENT_READ`].
pub(super) fn retirement_driver_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && f.frames.get(1).is_some_and(|l| l.contains(RETIREMENT_NAME))
        && f.record.iter().any(|l| l.trim_end().ends_with(RETIREMENT_READ))
}

/// Whether F1 25 faulted reading the track node of a round while Team HQ listed the season's weekends: the nearest
/// frame is [`WEEKENDS_TRACK_NODE`].
pub(super) fn weekends_track_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && f.frames.first().is_some_and(|l| l.contains(WEEKENDS_TRACK_NODE))
}

/// Whether F1 25 faulted loading the first session of a career whose current season has no weekend, after Custom
/// Calendar's picker imported the career's rounds: the nearest frame is in [`NO_WEEKEND_LOAD`], the fault read
/// [`NO_WEEKEND_READ`] and the loader logged [`CALENDAR_IMPORT`] before the crash.
pub(super) fn imported_calendar_crash(e: &Evidence, f: &Fatal) -> bool {
    let at = f.module.as_deref() == Some("f1_25.exe")
        && f.frames
            .first()
            .is_some_and(|l| NO_WEEKEND_LOAD.iter().any(|site| l.contains(site)))
        && f.record.iter().any(|l| l.trim_end().ends_with(NO_WEEKEND_READ));
    let end = crash_time(e, f);
    at && e
        .loader
        .lines()
        .any(|l| l.contains(CALENDAR_IMPORT) && end.is_none_or(|end| stamp(l).is_none_or(|t| t <= end)))
}

pub(super) fn imported_calendar_empty_season(e: &Evidence) -> Diagnosis {
    let picker = name(e, CUSTOM_CALENDAR);
    let career = match e.enabled.iter().any(|id| id == F2_REVAMP) {
        true => format!(" into {}'s career, which starts in F2", name(e, F2_REVAMP)),
        false => String::new(),
    };
    Diagnosis {
        id: "imported_calendar_empty_season",
        title: format!("{picker}'s imported calendar left your career without a first round"),
        what: format!(
            "F1 25 crashed loading the first session of your new career: the career's first season has no rounds. \
             Its calendar was imported with the {picker} picker (F9){career}, and that import left the first season \
             empty."
        ),
        steps: vec![
            format!(
                "Start the career again and choose its season on the game's own screens, without the {picker} picker \
                 (F9). The career you just started has no first round, so it can't be played."
            ),
            format!("Or turn off {picker} in your profile before you start the career."),
            "Press Play again.".to_owned(),
        ],
    }
}

pub(super) fn weekends_track_record_bad_index() -> Diagnosis {
    Diagnosis {
        id: "weekends_track_record_bad_index",
        title: "A round of your career has a track F1 25 can't find".to_owned(),
        what: "F1 25 crashed as Team HQ listed the weekends of a season. A round of that season is held at a track \
               that is not in the game's track list."
            .to_owned(),
        steps: vec![
            "Turn on the track addons the career's earlier seasons use, or turn off the addon that changed the \
             tracks of the calendar."
                .to_owned(),
            "Press Play again.".to_owned(),
        ],
    }
}

pub(super) fn race_driver_missing() -> Diagnosis {
    Diagnosis {
        id: "race_driver_missing",
        title: "A car in your race has a driver F1 25 can't find".to_owned(),
        what: "F1 25 crashed as a car retired from the race, while it named that car's driver in the retirement \
               message. That car's driver is not in the game's driver list, usually because an addon changed the \
               career's drivers or line-ups."
            .to_owned(),
        steps: vec![
            "Turn off the addon that changes your career's drivers or line-ups (a driver market, team and driver \
             editor or DB Editor package)."
                .to_owned(),
            "Press Play again.".to_owned(),
        ],
    }
}

pub(super) fn career_driver_missing() -> Diagnosis {
    Diagnosis {
        id: "career_driver_missing",
        title: "Your career has a driver F1 25 can't find".to_owned(),
        what: "F1 25 crashed as your career's session loaded, while it set up the driver models of your team for \
               the career hub. A driver of your team in this career is not in the game's driver list, usually \
               because an addon changed the career's drivers or line-ups."
            .to_owned(),
        steps: vec![
            "Turn off the addon that changes your career's drivers or line-ups (a driver market, team and driver \
             editor or DB Editor package)."
                .to_owned(),
            "Press Play again.".to_owned(),
        ],
    }
}

/// The text `rdi` points at in the crash report's memory dump, the name the game looked up.
pub(super) fn searched_name(report: &str) -> Option<String> {
    let mut lines = report
        .lines()
        .skip_while(|l| !l.starts_with("== Memory at the registers"))
        .skip_while(|l| !l.trim_start().starts_with("rdi="));
    let rdi = u64::from_str_radix(lines.next()?.trim_start().get(4..20)?, 16).ok()?;
    let mut dump = BTreeMap::new();
    for l in lines.take_while(|l| l.starts_with("    ")) {
        let t = l.trim_start();
        let Some(addr) = t.get(..16).and_then(|a| u64::from_str_radix(a, 16).ok()) else {
            continue;
        };
        let bytes: Vec<u8> = t
            .get(17..)
            .unwrap_or_default()
            .split_whitespace()
            .take(16)
            .map_while(|b| u8::from_str_radix(b, 16).ok())
            .collect();
        dump.insert(addr, bytes);
    }
    let mut name = Vec::new();
    let mut addr = rdi;
    loop {
        let byte = *dump.get(&(addr & !0xf))?.get((addr & 0xf) as usize)?;
        if byte == 0 {
            break;
        }
        name.push(byte);
        if name.len() > 64 {
            return None;
        }
        addr += 1;
    }
    let ok = !name.is_empty()
        && name
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
    ok.then(|| String::from_utf8_lossy(&name).into_owned())
}

pub(super) fn missing_entrant_list(e: &Evidence) -> Diagnosis {
    let list = e.report.as_deref().and_then(searched_name);
    let addon = e
        .kept_lists
        .iter()
        .filter(|(id, _)| !e.builtin.contains(*id))
        .find(|(_, lists)| list.as_ref().is_none_or(|l| lists.contains(l)))
        .map(|(id, _)| name(e, id));
    let which = list
        .as_ref()
        .map_or("an entrant list".to_owned(), |l| format!("the entrant list {l}"));
    let by = addon.as_ref().map_or(String::new(), |a| {
        format!(" The last build found {a} renaming or removing it.")
    });
    let mut steps = vec![match &addon {
        Some(a) => format!("Turn off {a}, or ask its author for a version made for this F1 25 update."),
        None => "Turn off the database addons you added most recently, such as imported team packs, one at a time \
                 until the crash stops."
            .to_owned(),
    }];
    steps.push("Press Play again.".to_owned());
    Diagnosis {
        id: "missing_entrant_list",
        title: format!("F1 25 crashed looking for {which}"),
        what: format!(
            "F1 25 looks up {which} by name when it builds a menu such as the F1 World Showroom, and the game's \
             database has no such list. A database addon renames or removes it, often one made for an older F1 25 \
             update.{by}"
        ),
        steps,
    }
}

/// The track stat id the core logged as having no tracks row or track record at the session load.
pub(super) fn missing_track_stat(loader: &str) -> Option<i64> {
    loader.lines().rev().find_map(|l| {
        let rest = l.split("track stat id ").nth(1)?;
        let (id, tail) = rest.split_once(' ')?;
        tail.starts_with("has no tracks row or track record")
            .then(|| id.parse().ok())
            .flatten()
    })
}

pub(super) fn career_content_off(e: &Evidence) -> Diagnosis {
    let off: Vec<String> = e.off.iter().map(|id| name(e, id)).collect();
    let what =
        "F1 25 crashed setting up the track for your career's next session, just after the career loaded.";
    let missing = missing_track_stat(&e.loader);
    let (title, what) = match off.as_slice() {
        [] => (
            "Your career uses a track that is not in this profile".to_owned(),
            format!(
                "{what} A race in your career uses a track (track stat id {}) that no addon in this profile provides.",
                missing.unwrap_or_default()
            ),
        ),
        [one] => (
            format!("Your career uses {one}, which is off"),
            format!("{what} A race in your career uses {one}, which is off in this profile."),
        ),
        _ => (
            "Your career uses a track or season that is off".to_owned(),
            format!(
                "{what} A race in your career uses a track or season that is off in this profile, which has {} off.",
                list(&off)
            ),
        ),
    };
    Diagnosis {
        id: "career_content_off",
        title,
        what,
        steps: vec![
            match off.is_empty() {
                true => "Turn back on the track addon your career's calendar uses. In a My Team career you can also change that round with the Career Mode Editor."
                    .to_owned(),
                false => format!(
                    "Turn {} back on. In a My Team career you can also change that round with the Career Mode Editor.",
                    list(&off)
                ),
            },
            "Press Play again.".to_owned(),
        ],
    }
}

/// The [`QUICK_END`] line with the hub opened inside the session, at most [`QUICK_END_SECS`] before `end`.
fn quick_end(e: &Evidence, end: f64) -> Option<&str> {
    e.loader
        .lines()
        .rev()
        .filter_map(|l| Some((stamp(l)?, l)))
        .skip_while(|(t, _)| *t > end)
        .take_while(|(t, _)| end - t <= QUICK_END_SECS)
        .find(|(_, l)| l.contains(QUICK_END) && l.contains(HUB_INSIDE))
        .map(|(_, l)| l)
}

/// Whether F1 25 faulted as a session it had sent to Team HQ closed: the nearest frame is in
/// [`CLOSED_SESSION_AT`], the fault read [`CLOSED_SESSION_READ`], and the core logged [`QUICK_END`] with the hub
/// opened inside the session just before.
pub(super) fn sent_to_team_hq_crash(e: &Evidence, f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && f.frames
            .first()
            .is_some_and(|l| CLOSED_SESSION_AT.iter().any(|at| l.trim_end().ends_with(at)))
        && f.record.iter().any(|l| l.contains(CLOSED_SESSION_READ))
        && crash_time(e, f).is_some_and(|end| quick_end(e, end).is_some())
}

/// The item the calendar check picked in the last [`HUB_OPENED`] line before `line`, as in `SeasonBreak` from
/// `(item 1, SeasonBreak, decides)`, and whether the core's season break fix ran between them.
fn hub_decision<'a>(e: &'a Evidence, line: &str) -> (Option<&'a str>, bool) {
    let before = e
        .loader
        .find(line)
        .map_or(e.loader.as_str(), |at| &e.loader[..at]);
    let Some(at) = before.rfind(HUB_OPENED) else {
        return (None, false);
    };
    let opened = &before[at..];
    let item = opened
        .lines()
        .next()
        .and_then(|l| l.split_once("(item ")?.1.split_once(", decides)"))
        .and_then(|(item, _)| item.split_once(", ").map(|(_, kind)| kind));
    (item, opened.contains(BREAK_FIX))
}

/// A session the game's calendar check sent to Team HQ as it opened, and the crash as it closed.
pub(super) fn sent_to_team_hq(e: &Evidence, f: &Fatal) -> Diagnosis {
    let line = crash_time(e, f).and_then(|end| quick_end(e, end));
    let weekend = line
        .and_then(|l| l.split_once("after start): "))
        .and_then(|(_, w)| Weekend::parse(w.split("; ").next()?))
        .map_or("the race weekend".to_owned(), |w| {
            format!("the {} weekend", w.text())
        });
    let (item, fixed) = line.map_or((None, false), |l| hub_decision(e, l));
    let unreadable = item.is_none();
    let why = match item {
        Some("SeasonBreak") if !fixed => {
            " The calendar has a season break running over this race weekend, and the check put the break first."
                .to_owned()
        }
        Some(kind) => format!(" The check put the calendar item {kind} first."),
        None => String::new(),
    };
    let mut steps = Vec::new();
    if !fixed && (unreadable || item == Some("SeasonBreak")) {
        steps.push(
            "Update Swofty Launcher to the newest version. As the session opens, its core sets a season break that \
             runs over the race weekend to ignored, and the weekend's first session loads."
                .to_owned(),
        );
    }
    steps.push(
        "If Team HQ then has no Advance, open Career saves: Restore puts back an earlier version of your save.".to_owned(),
    );
    steps.push("If F1 25 crashes here again, press Send Logs.".to_owned());
    Diagnosis {
        id: "sent_to_team_hq",
        title: "F1 25 left the race weekend for Team HQ as it opened".to_owned(),
        what: format!(
            "As the first session of {weekend} opened, the game's calendar check sent the career to Team HQ, and \
             F1 25 crashed as the session closed.{why}"
        ),
        steps,
    }
}

/// A session that ended with its career in a race weekend that started with no session run: the core's last Team HQ
/// weekend line is `weekend already started, session never ran`, with no session load after it.
pub fn stuck_weekend(e: &Evidence) -> Option<Diagnosis> {
    let weekend = super::weekend::stuck_weekend(&e.loader)?;
    Some(Diagnosis {
        id: "stuck_weekend",
        title: "Your career is stuck at the start of a race weekend".to_owned(),
        what: format!(
            "Your career was saved inside its {} weekend before any session of it ran. Each load of that save \
             opens Team HQ with no Advance, and the weekend's first session never loads.",
            weekend.text()
        ),
        steps: vec![
            "Open Career saves: Restore on its banner puts back the newest earlier version of your save, and the \
             save as it is stays in Versions. Review the restore shows the season and round of each career in that \
             version first."
                .to_owned(),
            "If F1 25 crashes or ends up here again on the way into this weekend, press Send Logs.".to_owned(),
        ],
    })
}

/// The cars of the grid the crashed session's set-up counted: `N` in `participants: game manager N`.
pub(super) fn grid_cars(e: &Evidence, f: &Fatal) -> Option<usize> {
    let line = crashed_session(e, f)
        .lines()
        .rev()
        .find(|l| l.contains(VEHICLE_SETUP))?;
    let count = line.split_once(GAME_MANAGER)?.1;
    count.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Whether F1 25 faulted building the UDP telemetry's per-car tables for a car with no vehicle: the nearest frame is in
/// [`NULL_VEHICLE_AT`] and the fault read [`NULL_VEHICLE_READ`].
pub(super) fn null_vehicle_crash(f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe")
        && f.frames
            .first()
            .is_some_and(|l| NULL_VEHICLE_AT.iter().any(|at| l.trim_end().ends_with(at)))
        && f.record.iter().any(|l| l.trim_end().ends_with(NULL_VEHICLE_READ))
}

/// Whether an F1 25 crash came with a grid of [`OVER_GRID_MANAGER`] or more cars at the session set-up.
pub(super) fn over_grid_crash(e: &Evidence, f: &Fatal) -> bool {
    f.module.as_deref() == Some("f1_25.exe") && grid_cars(e, f).is_some_and(|n| n >= OVER_GRID_MANAGER)
}

pub(super) fn career_over_grid(e: &Evidence, f: &Fatal) -> Diagnosis {
    let cars = grid_cars(e, f).map_or("more than 22 cars".to_owned(), |n| format!("{n} cars"));
    Diagnosis {
        id: "career_over_grid",
        title: "Your career's grid has more cars than F1 25 expects".to_owned(),
        what: format!(
            "F1 25 crashed as your career's session started. The grid of this career has {cars} where the game \
             expects {GRID_CARS}. This happens after line-up edits that move a driver into another team's seat and \
             leave that driver's old seat in the grid, so two cars have no vehicle for the game to read."
        ),
        steps: vec![
            "Start a new career with the stock line-ups, with no drivers moved.".to_owned(),
            "If you move drivers, keep exactly 2 drivers in every team and move them one at a time, testing the \
             race load after each move."
                .to_owned(),
            "Turn off the addon or editor that moves drivers in your profile before you start the new career."
                .to_owned(),
        ],
    }
}
