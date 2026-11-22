//! Crash causes the Send Logs triage keeps finding, recognised from a crashed
//! session's own files: the `[crash]`, `[overlay]` and `[offline] module:`
//! lines of its loader.log, the crash report the core wrote, and the files in
//! the game folder. The launcher shows the cause and its fix when one of the
//! rules here matches, and Send Logs and the support Discord when none does.
//! An addon whose changes don't apply to an F1 25 older than the newest
//! update is recognised from the build's error, for the launch's failure.

mod broken_addons;
mod career;
mod crash_record;
mod evidence;
mod game_version;
mod graphics;
mod hang;
mod hooks;
mod logos;
mod memory;
mod package_ids;
mod saves;
mod slow_init;
mod stall;
mod system;
mod tyres;
pub mod weekend;
mod wording;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::session::SessionRecord;
use crate::store::Store;

use broken_addons::{
    aggressive_ai, aggressive_ai_crash, anim_clip_crash, anim_clip_nan, marshal_lights, marshal_lights_crash,
    pitlane, pitlane_crash, skip_cutscenes, skip_cutscenes_crash, td_wheel_off, td_wheel_off_anim,
    td_wheel_off_anim_crash, td_wheel_off_crash, tyre_curve_crash, tyre_curve_nan, unloaded_ai_line,
    unloaded_ai_line_crash,
};
pub use career::stuck_weekend;
use career::{
    calendar_event_missing, career_content_off, career_driver_missing, career_track_crash,
    entrant_list_crash, imported_calendar_crash, imported_calendar_empty_season, missing_entrant_list,
    missing_track_stat, morph_driver_crash, race_driver_missing, retirement_driver_crash, sent_to_team_hq,
    sent_to_team_hq_crash, sequencer_record_crash, third_car_line, third_car_line_crash,
    weekends_track_crash, weekends_track_record_bad_index,
};
use crash_record::{Fatal, fatal, loaded_modules, wine};
pub use evidence::Evidence;
use evidence::evidence;
pub(crate) use evidence::read_perf;
pub use game_version::{LATEST_GAME_VERSION, older_game, outdated_exe, outdated_game};
use graphics::graphics_mods;
use hang::{hang, hang_watchdog};
pub use logos::unbuilt_logos;
pub use package_ids::{PACKAGE_ID_MAX, PackageIds};
pub use saves::unsaved_profile;
pub(crate) use slow_init::slow_start;
use stall::BOOT_STALL_SECS;
pub use stall::stalled;
pub(crate) use stall::{addon_stall, last_screen, perf_start_screen};
use system::{
    APP_CONTROL, app_control, hardware_settings, hardware_settings_crash, low_disk, low_disk_space,
    quit_fault, quit_fault_diagnosis,
};
pub use tyres::unplaced_tyres;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    /// A stable name for the rule that matched, for logs and Send Logs.
    pub id: &'static str,
    pub title: String,
    pub what: String,
    pub steps: Vec<String>,
}

impl Diagnosis {
    /// What happened, then its steps, as one paragraph.
    pub fn text(&self) -> String {
        std::iter::once(&self.what)
            .chain(&self.steps)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The cause of the crash `session` ended with, when a rule recognises it.
pub fn crash(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    diagnose(&evidence(session, game, store))
}

/// Why `session`, which ended without a crash, never got going, when a rule
/// recognises it.
pub fn stall(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    perf_start_screen(&read_perf(&session.dir)?).filter(|s| *s >= BOOT_STALL_SECS)?;
    stalled(&evidence(session, game, store))
}

/// Why the career of `session`, which ended without a crash, can't go on: it
/// ended in a race weekend that started with no session run.
pub fn stuck(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    stuck_weekend(&evidence(session, game, store))
}

/// Why `session`, which ended without a crash, ran with cars that had no
/// tyres, when its log names them.
pub fn tyres(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    unplaced_tyres(&evidence(session, game, store))
}

/// Why `session`, which ended without a crash, ran without its addons' team
/// logos, when its log says the game could not write them.
pub fn logos(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    let move_hint = game.map_or("", |g| {
        crate::game_folder::copies::move_hint(crate::install::Source::detect(g))
    });
    unbuilt_logos(
        &evidence(session, game, store),
        crate::platform::run_as_admin::elevated,
        move_hint,
    )
}

/// Why `session`, which ended without a crash, could not save the profile,
/// when its log says the kept careers could not be written back.
pub fn saves(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Option<Diagnosis> {
    unsaved_profile(&evidence(session, game, store))
}

/// The cause of a crash, from the first rule that recognises it. A crash no
/// rule recognises, on an F1 25 older than [`LATEST_GAME_VERSION`], is put down
/// to the old build.
pub fn diagnose(e: &Evidence) -> Option<Diagnosis> {
    matched(e).or_else(|| {
        let mut d = outdated_exe(e.game_version.as_deref())?;
        d.what = format!(
            "Your F1 25 is {}. Mods are made for {LATEST_GAME_VERSION}, the latest update, and F1 25 crashed on \
             the older one.",
            e.game_version.as_deref().unwrap_or_default().trim()
        );
        Some(d)
    })
}

/// A crash as the rules read it: the evidence, with what every rule needs from it worked out once.
struct Crash<'a> {
    evidence: &'a Evidence,
    fatal: Fatal,
    /// Graphics mods in the game folder, by tool.
    mods: Vec<(&'static str, Vec<String>)>,
    /// False when the game ran under Wine or Proton.
    on_windows: bool,
}

impl<'a> Crash<'a> {
    fn new(evidence: &'a Evidence) -> Self {
        let loaded = loaded_modules(&evidence.loader);
        Self {
            evidence,
            fatal: fatal(evidence),
            mods: graphics_mods(&evidence.game_files, &loaded),
            on_windows: !wine(evidence, &loaded),
        }
    }
}

type Rule = fn(&Crash) -> Option<Diagnosis>;

/// The crash rules, tried in order; the first that recognises the crash names its cause.
const RULES: &[Rule] = &[
    |c| low_disk(&c.evidence.loader).map(|(drive, free, need)| low_disk_space(&drive, free, need)),
    |c| (c.evidence.code == Some(APP_CONTROL)).then(app_control),
    |c| quit_fault(c.evidence).then(|| stuck_weekend(c.evidence).unwrap_or_else(quit_fault_diagnosis)),
    hooks::crash_in_hook,
    |c| aggressive_ai_crash(c.evidence, &c.fatal).then(|| aggressive_ai(c.evidence)),
    |c| skip_cutscenes_crash(c.evidence, &c.fatal).then(|| skip_cutscenes(c.evidence)),
    hooks::stray_jump,
    memory::heap_exhausted,
    |c| hang_watchdog(c.evidence, &c.fatal).then(|| hang(c.evidence, &c.fatal, &c.mods)),
    |c| pitlane_crash(c.evidence, &c.fatal).then(|| pitlane(c.evidence)),
    |c| td_wheel_off_crash(c.evidence, &c.fatal).then(|| td_wheel_off(c.evidence)),
    |c| tyre_curve_crash(c.evidence, &c.fatal).then(tyre_curve_nan),
    |c| td_wheel_off_anim_crash(c.evidence, &c.fatal).then(|| td_wheel_off_anim(c.evidence)),
    |c| anim_clip_crash(c.evidence, &c.fatal).then(anim_clip_nan),
    |c| unloaded_ai_line_crash(&c.fatal).then(|| unloaded_ai_line(c.evidence, &c.fatal)),
    |c| marshal_lights_crash(c.evidence, &c.fatal).then(|| marshal_lights(c.evidence, &c.fatal)),
    |c| hardware_settings_crash(c.evidence, &c.fatal).then(|| hardware_settings(c.evidence)),
    |c| entrant_list_crash(c.evidence, &c.fatal).then(|| missing_entrant_list(c.evidence)),
    |c| sent_to_team_hq_crash(c.evidence, &c.fatal).then(|| sent_to_team_hq(c.evidence, &c.fatal)),
    |c| {
        let content_off = !c.evidence.off.is_empty() || missing_track_stat(&c.evidence.loader).is_some();
        (content_off && career_track_crash(c.evidence, &c.fatal)).then(|| career_content_off(c.evidence))
    },
    |c| sequencer_record_crash(&c.fatal).then(|| calendar_event_missing(c.evidence)),
    |c| morph_driver_crash(&c.fatal).then(career_driver_missing),
    |c| retirement_driver_crash(&c.fatal).then(race_driver_missing),
    |c| weekends_track_crash(&c.fatal).then(weekends_track_record_bad_index),
    |c| imported_calendar_crash(c.evidence, &c.fatal).then(|| imported_calendar_empty_season(c.evidence)),
    |c| third_car_line_crash(c.evidence, &c.fatal).then(|| third_car_line(c.evidence)),
    graphics::device_removed,
    graphics::out_of_graphics_memory,
    graphics::graphics_mod_crash,
    graphics::driver_crash,
    graphics::frame_generation_crash,
];

fn matched(e: &Evidence) -> Option<Diagnosis> {
    let crash = Crash::new(e);
    RULES.iter().find_map(|rule| rule(&crash))
}
