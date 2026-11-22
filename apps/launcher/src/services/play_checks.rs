use std::fmt::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use swofty_core::addons::files::Damaged;
use swofty_core::data::space::Room;
use swofty_core::store::Store;
use swofty_i18n::{fill, tr, trf, trn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Note {
    pub level: NoteLevel,
    pub title: String,
    pub body: String,
    pub sticky: bool,
}

impl Note {
    fn warning(title: &str, body: String) -> Self {
        Self {
            level: NoteLevel::Warning,
            title: title.to_owned(),
            body,
            sticky: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Go,
    Stop(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gate {
    pub write_blocked: bool,
    pub setup_done: bool,
    pub offline_accepted: bool,
    pub first_launch: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Timings(Vec<(&'static str, Duration)>);

impl Timings {
    pub(crate) fn time<T>(&mut self, step: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let out = f();
        self.0.push((step, started.elapsed()));
        out
    }

    pub(crate) fn line(&self) -> String {
        let total: Duration = self.0.iter().map(|(_, d)| *d).sum();
        Self::format(total, &self.0)
    }

    fn format(total: Duration, steps: &[(&'static str, Duration)]) -> String {
        let parts: Vec<String> = steps
            .iter()
            .map(|(name, d)| format!("{name} {}", d.as_millis()))
            .collect();
        format!("[play] checks {} ms ({})", total.as_millis(), parts.join(", "))
    }
}

pub(crate) struct Inputs {
    pub store: Store,
    pub profile: String,
    pub game: PathBuf,
    pub build_id: Option<u64>,
}

pub(crate) struct Checked {
    pub profile: String,
    pub game: PathBuf,
    pub gate: Gate,
    pub notes: Vec<Note>,
    pub verdict: Verdict,
    pub room: Room,
    pub timings: Timings,
    /// Lines for the app log after the timings line.
    pub lines: Vec<String>,
    /// On addons whose files are not what their install wrote.
    pub damaged: Vec<Damaged>,
}

pub(crate) fn run(inputs: Inputs) -> Checked {
    let Inputs {
        store,
        profile,
        game,
        build_id,
    } = inputs;
    let mut timings = Timings::default();
    let gate = timings.time("gate", || Gate {
        write_blocked: swofty_core::platform::run_as_admin::write_blocker(&game).is_some(),
        setup_done: swofty_core::game_folder::setup::done(&store.root),
        offline_accepted: swofty_core::offline::is_accepted(&store.root),
        first_launch: swofty_core::game_folder::setup::first_launch(&store.root),
    });
    let mut notes = Vec::new();
    let mut lines = Vec::new();
    let mut damaged = Vec::new();
    let verdict = match (
        timings.time("profile", || store.profile(&profile)),
        timings.time("addons", || store.addons()),
    ) {
        (Ok(p), Ok(installed)) => {
            if !p.is_vanilla() {
                damaged = timings.time("files", || {
                    swofty_core::addons::files::damaged(&store, p.enabled(), &installed)
                });
                for d in &damaged {
                    for text in d.lines() {
                        let mut line = String::new();
                        let _ = write!(line, "[play] files: {} {text}", d.id);
                        lines.push(line);
                    }
                }
            }
            if let Some(stop) = damaged_stop(&damaged) {
                Verdict::Stop(stop)
            } else {
                let catalog = timings.time("catalog", || {
                    swofty_core::catalog::cached(&store.root).unwrap_or_default()
                });
                timings.time("lonely", || {
                    notes.extend(lonely(&store, &p, &installed, &catalog))
                });
                if !p.is_vanilla() {
                    let found = timings.time("withdrawn", || {
                        swofty_core::catalog::withdrawn::enabled(&store, p.enabled())
                    });
                    if !found.is_empty() {
                        let labels: Vec<String> = found.iter().map(|w| w.label()).collect();
                        let mut line = String::new();
                        let _ = write!(line, "[play] withdrawn addons on: {}", labels.join(", "));
                        lines.push(line);
                        notes.push(withdrawn_note(&labels));
                    }
                }
                let all = match p.is_vanilla() {
                    true => Default::default(),
                    false => timings.time("deps", || swofty_core::stdlib::deps::read_all(&store, &installed)),
                };
                timings.time("track limit", || {
                    let (note, line) = track_limit_note(&store, &game, build_id, &p, &all);
                    notes.extend(note);
                    lines.extend(line);
                });
                registry(&store, &game, &p, &installed, &all, &mut notes, &mut timings)
            }
        }
        _ => Verdict::Go,
    };
    let room = timings.time("room", || Room::now(&store.root));
    Checked {
        profile,
        game,
        gate,
        notes,
        verdict,
        room,
        timings,
        lines,
        damaged,
    }
}

/// What stops Play when the build would fail on a damaged addon.
pub(crate) fn damaged_stop(damaged: &[Damaged]) -> Option<String> {
    let stopping: Vec<&Damaged> = damaged.iter().filter(|d| d.stops_play()).collect();
    match stopping.as_slice() {
        [] => None,
        [one] => Some(trf!(
            "{name} is damaged: {files}. Reinstall it, then press Play.",
            name = one.name,
            files = one.summary()
        )),
        many => Some(trf!(
            "{names} are damaged. Reinstall them, then press Play.",
            names = many
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn track_limit_note(
    store: &Store,
    game: &std::path::Path,
    build_id: Option<u64>,
    profile: &swofty_core::profile::Profile,
    std16: &std::collections::BTreeMap<String, swofty_core::stdlib::deps::Std>,
) -> (Option<Note>, Option<String>) {
    if profile.is_vanilla() {
        return (None, None);
    }
    let started = Instant::now();
    let identity =
        match swofty_core::game_folder::baseline::Identity::detect_cached(&store.root, game, build_id) {
            Ok(identity) => identity,
            Err(e) => {
                let mut line = String::new();
                let _ = write!(
                    line,
                    "[play] track limit: the game's version could not be read ({e}), so no check ran ({} ms)",
                    started.elapsed().as_millis()
                );
                return (None, Some(line));
            }
        };
    let identified = started.elapsed();
    let game = swofty_core::install::Game {
        dir: game.to_path_buf(),
        user_reg: None,
        identity,
    };
    let checked = Instant::now();
    let check = swofty_core::build::track_limit(store, &profile.name, &game, std16);
    let checking = checked.elapsed();
    let check = match check {
        Ok(check) => check,
        Err(e) => {
            let mut line = String::new();
            let _ = write!(
                line,
                "[play] track limit: the check failed after {} ms (identity {} ms, check {} ms): {e}",
                started.elapsed().as_millis(),
                identified.as_millis(),
                checking.as_millis()
            );
            return (None, Some(line));
        }
    };
    let recorded = Instant::now();
    swofty_core::tracks::record_track_limit(&store.root, &profile.name, check.limit.as_ref()).ok();
    let parts = LimitTimes {
        total: started.elapsed(),
        identity: identified,
        record: recorded.elapsed(),
    };
    let line = limit_line(&check, &parts);
    (check.limit.as_ref().map(limit_note), Some(line))
}

pub(crate) struct LimitTimes {
    pub total: Duration,
    pub identity: Duration,
    pub record: Duration,
}

struct Tracks(usize);

impl std::fmt::Display for Tracks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            1 => write!(f, "1 track"),
            n => write!(f, "{n} tracks"),
        }
    }
}

struct LeftOut(usize);

impl std::fmt::Display for LeftOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => write!(f, "none left out"),
            n => write!(f, "{} left out", Tracks(n)),
        }
    }
}

pub(crate) fn limit_line(check: &swofty_core::build::TrackLimitCheck, t: &LimitTimes) -> String {
    use swofty_core::tracks::{FILL_FROM, TRACK_LIST_ROOM};
    let added = Tracks(check.added);
    let identity = t.identity.as_millis();
    let counting = check.counting.as_millis();
    let record = t.record.as_millis();
    let mut line = String::new();
    if !check.fitted {
        let _ = write!(
            line,
            "[play] track limit: the on addons add {added}, under the {FILL_FROM} that can fill the game's track lists \
             ({TRACK_LIST_ROOM} tracks), so nothing was fitted ({} ms: identity {identity} ms, counting {counting} ms, \
             record {record} ms)",
            t.total.as_millis(),
        );
        return line;
    }
    let left_out = LeftOut(check.limit.as_ref().map_or(0, |l| l.names.len()));
    let _ = write!(
        line,
        "[play] track limit: the on addons add {added}, at least the {FILL_FROM} that can fill the game's track lists \
         ({TRACK_LIST_ROOM} tracks), so the check fitted their tracks to the lists: {} ms (identity {identity} ms, \
         counting {counting} ms, fitting {} ms, record {record} ms); {left_out}",
        t.total.as_millis(),
        check.fitting.as_millis(),
    );
    line
}

pub(crate) fn limit_text(room: usize, names: &[String]) -> String {
    const SHOWN: usize = 4;
    let list = match names.len().saturating_sub(SHOWN) {
        0 => names.join(", "),
        more => trf!(
            "{shown} and {more} more",
            shown = names[..SHOWN].join(", "),
            more = more
        ),
    };
    fill(
        &trn(
            names.len(),
            "The game holds {room} tracks, so {count} track is left out of this build: {list}. Turn some track addons off to play it.",
            "The game holds {room} tracks, so {count} tracks are left out of this build: {list}. Turn some track addons off to play them.",
        ),
        &[("room", room.to_string()), ("list", list)],
    )
}

pub(crate) fn limit_note(limit: &swofty_core::tracks::TrackLimit) -> Note {
    Note::warning(tr("Too many tracks"), limit_text(limit.room, &limit.names))
}

fn lonely(
    store: &Store,
    profile: &swofty_core::profile::Profile,
    installed: &std::collections::BTreeMap<String, swofty_core::manifest::AddonManifest>,
    catalog: &[swofty_core::catalog::Addon],
) -> Vec<Note> {
    let mut notes = Vec::new();
    for (base, family) in swofty_core::profile::resolver::lonely_bases(profile, installed, catalog) {
        if installed
            .values()
            .any(|m| m.name == base && store.is_builtin(&m.id))
        {
            continue;
        }
        notes.push(lonely_note(&base, &family));
    }
    notes
}

/// The notice for addons that are on whose installed version was withdrawn from the Addon Browser.
fn withdrawn_note(labels: &[String]) -> Note {
    let list = labels.join(", ");
    match labels {
        [one] => Note::warning(
            &trf!("{addon} was withdrawn", addon = one),
            trf!(
                "{addon} was withdrawn from the Addon Browser. Turn it off on the Addons page.",
                addon = one
            ),
        ),
        _ => Note::warning(
            tr("Withdrawn addons are on"),
            trf!(
                "{addons} were withdrawn from the Addon Browser. Turn them off on the Addons page.",
                addons = list
            ),
        ),
    }
}

fn lonely_note(base: &str, family: &[String]) -> Note {
    let shown = family.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
    let more = family.len().saturating_sub(4);
    let body = if more > 0 {
        trf!(
            "{base} is on, but none of its addons are, so it adds nothing to pick on its own. Install at least one in the Addon Browser, such as {shown} and {more} more, and turn it on.",
            base = base,
            shown = shown,
            more = more
        )
    } else {
        trf!(
            "{base} is on, but none of its addons are, so it adds nothing to pick on its own. Install at least one in the Addon Browser, such as {shown}, and turn it on.",
            base = base,
            shown = shown
        )
    };
    Note::warning(&trf!("{base} needs one of its addons", base = base), body)
}

fn registry(
    store: &Store,
    game: &std::path::Path,
    profile: &swofty_core::profile::Profile,
    installed: &std::collections::BTreeMap<String, swofty_core::manifest::AddonManifest>,
    all: &std::collections::BTreeMap<String, swofty_core::stdlib::deps::Std>,
    notes: &mut Vec<Note>,
    timings: &mut Timings,
) -> Verdict {
    use swofty_core::stdlib::{careers, deps};
    if profile.is_vanilla() {
        let found = timings.time("careers", || {
            careers::sync(&store.root, Some(game), None).unwrap_or_default()
        });
        let using = found.iter().filter(|c| c.uses_addons()).count();
        notes.extend(vanilla_note(using));
        return Verdict::Go;
    }
    let plan = match timings.time("resolve", || {
        swofty_core::profile::resolver::resolve(profile, installed)
    }) {
        Ok(plan) => plan,
        Err(swofty_core::Error::AddonConflict { addon, other }) => {
            return Verdict::Stop(conflict_stop(&addon, &other));
        }
        Err(_) => return Verdict::Go,
    };
    let report = timings.time("deps check", || {
        deps::for_order(&profile.name, &plan.order, installed, all)
    });
    let findings = report.findings.iter().map(finding_note).collect();
    assemble(findings, report.stop_text(), notes)
}

/// What stops Play when two addons that are on conflict.
fn conflict_stop(addon: &str, other: &str) -> String {
    trf!(
        "{addon} conflicts with {other}. Turn one of them off on the Addons page",
        addon = addon,
        other = other
    )
}

fn vanilla_note(using: usize) -> Option<Note> {
    if using == 0 {
        return None;
    }
    let text = fill(
        &trn(
            using,
            "{count} career uses addons; loading it in {profile} can lose its progress.",
            "{count} careers use addons; loading them in {profile} can lose their progress.",
        ),
        &[("profile", swofty_core::profile::VANILLA.to_string())],
    );
    Some(Note::warning(tr("Careers that need addons"), text))
}

fn finding_note(f: &swofty_core::stdlib::deps::Finding) -> Note {
    use swofty_core::stdlib::deps::Level;
    let (level, sticky) = match f.level {
        Level::Stop => (NoteLevel::Error, true),
        Level::Warn => (NoteLevel::Warning, true),
        Level::Hint => (NoteLevel::Info, false),
    };
    Note {
        level,
        title: f.headline.clone(),
        body: f.detail.join(" "),
        sticky,
    }
}

pub(crate) fn assemble(
    findings: Vec<Note>,
    dependency_stop: Option<String>,
    notes: &mut Vec<Note>,
) -> Verdict {
    notes.extend(findings);
    match dependency_stop {
        Some(stop) => Verdict::Stop(stop),
        None => Verdict::Go,
    }
}

#[derive(Debug, Default)]
pub(crate) struct Pending {
    generation: u64,
    active: Option<u64>,
}

impl Pending {
    pub(crate) fn begin(&mut self) -> Option<u64> {
        if self.active.is_some() {
            return None;
        }
        self.generation += 1;
        self.active = Some(self.generation);
        Some(self.generation)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn finish(&mut self, generation: u64) -> bool {
        if self.active == Some(generation) {
            self.active = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.generation += 1;
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(body: &str) -> Note {
        Note {
            level: NoteLevel::Info,
            title: "t".into(),
            body: body.into(),
            sticky: false,
        }
    }

    #[test]
    fn dependency_stop_keeps_its_findings_and_blocks() {
        let mut notes = Vec::new();
        let v = assemble(vec![note("f")], Some("dep".into()), &mut notes);
        assert_eq!(v, Verdict::Stop("dep".into()));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn nothing_found_goes_on() {
        let mut notes = Vec::new();
        assert_eq!(assemble(vec![], None, &mut notes), Verdict::Go);
        assert!(notes.is_empty());
    }

    #[test]
    fn vanilla_note_counts_careers() {
        assert!(vanilla_note(0).is_none());
        assert!(vanilla_note(1).unwrap().body.starts_with("1 career uses addons"));
        assert!(vanilla_note(3).unwrap().body.starts_with("3 careers use addons"));
    }

    #[test]
    fn withdrawn_note_names_every_withdrawn_addon_that_is_on() {
        let one = withdrawn_note(&["BMW 2026 Testing 1.0.7".to_owned()]);
        assert_eq!(one.title, "BMW 2026 Testing 1.0.7 was withdrawn");
        assert_eq!(
            one.body,
            "BMW 2026 Testing 1.0.7 was withdrawn from the Addon Browser. Turn it off on the Addons page."
        );
        assert_eq!(one.level, NoteLevel::Warning);
        assert!(one.sticky);
        let two = withdrawn_note(&["A 1.0.0".to_owned(), "B 2.0.0".to_owned()]);
        assert_eq!(two.title, "Withdrawn addons are on");
        assert_eq!(
            two.body,
            "A 1.0.0, B 2.0.0 were withdrawn from the Addon Browser. Turn them off on the Addons page."
        );
    }

    #[test]
    fn lonely_note_names_four_and_counts_the_rest() {
        let family: Vec<String> = (1..=6).map(|i| format!("S{i}")).collect();
        let n = lonely_note("Base", &family);
        assert!(n.body.contains("S1, S2, S3, S4 and 2 more"));
        assert_eq!(n.title, "Base needs one of its addons");
    }

    #[test]
    fn limit_text_names_the_tracks_left_out() {
        let names: Vec<String> = ["Mugello", "Okayama"].map(String::from).into();
        assert_eq!(
            limit_text(64, &names),
            "The game holds 64 tracks, so 2 tracks are left out of this build: Mugello, Okayama. Turn some track addons off to play them."
        );
        assert_eq!(
            limit_text(64, &names[..1]),
            "The game holds 64 tracks, so 1 track is left out of this build: Mugello. Turn some track addons off to play it."
        );
    }

    #[test]
    fn limit_text_names_four_and_counts_the_rest() {
        let names: Vec<String> = (1..=13).map(|i| format!("T{i}")).collect();
        let text = limit_text(64, &names);
        assert!(text.contains("13 tracks are left out"));
        assert!(text.contains("T1, T2, T3, T4 and 9 more."));
        assert!(!text.contains("T5"));
    }

    #[test]
    fn limit_note_is_a_sticky_warning() {
        let limit = swofty_core::tracks::TrackLimit {
            room: 64,
            stock: 35,
            names: vec!["Mugello".into()],
            addons: vec!["mod.mugello".into()],
        };
        let n = limit_note(&limit);
        assert_eq!(n.title, "Too many tracks");
        assert_eq!(n.level, NoteLevel::Warning);
        assert!(n.sticky);
    }

    #[test]
    fn timing_line_lists_every_step() {
        let steps = [
            ("deps", Duration::from_millis(300)),
            ("resolve", Duration::from_millis(40)),
        ];
        assert_eq!(
            Timings::format(Duration::from_millis(412), &steps),
            "[play] checks 412 ms (deps 300, resolve 40)"
        );
    }

    fn limit_times() -> LimitTimes {
        LimitTimes {
            total: Duration::from_millis(155_812),
            identity: Duration::from_millis(3),
            record: Duration::from_millis(1),
        }
    }

    #[test]
    fn limit_line_without_a_fit_says_why() {
        let check = swofty_core::build::TrackLimitCheck {
            added: 5,
            counting: Duration::from_millis(28),
            ..Default::default()
        };
        let t = LimitTimes {
            total: Duration::from_millis(32),
            ..limit_times()
        };
        assert_eq!(
            limit_line(&check, &t),
            "[play] track limit: the on addons add 5 tracks, under the 22 that can fill the game's track lists (64 \
             tracks), so nothing was fitted (32 ms: identity 3 ms, counting 28 ms, record 1 ms)"
        );
    }

    #[test]
    fn limit_line_times_the_fit_and_counts_the_tracks_left_out() {
        let mut check = swofty_core::build::TrackLimitCheck {
            added: 31,
            counting: Duration::from_millis(8),
            fitted: true,
            fitting: Duration::from_millis(412),
            limit: Some(swofty_core::tracks::TrackLimit {
                room: 64,
                stock: 43,
                names: vec!["Mugello".into(), "Kyalami".into()],
                addons: vec!["mod.mugello".into(), "mod.kyalami".into()],
            }),
        };
        let t = LimitTimes {
            total: Duration::from_millis(424),
            ..limit_times()
        };
        assert_eq!(
            limit_line(&check, &t),
            "[play] track limit: the on addons add 31 tracks, at least the 22 that can fill the game's track lists \
             (64 tracks), so the check fitted their tracks to the lists: 424 ms (identity 3 ms, counting 8 ms, \
             fitting 412 ms, record 1 ms); 2 tracks left out"
        );
        check.limit = None;
        assert!(limit_line(&check, &t).ends_with("record 1 ms); none left out"));
    }

    #[test]
    fn timings_record_in_order() {
        let mut t = Timings::default();
        assert_eq!(t.time("a", || 1), 1);
        t.time("b", || ());
        assert!(t.line().starts_with("[play] checks "));
        assert!(t.line().contains("(a "));
        assert!(t.line().contains(", b "));
    }

    #[test]
    fn a_second_press_while_checking_is_refused() {
        let mut p = Pending::default();
        let first = p.begin().unwrap();
        assert!(p.is_active());
        assert!(p.begin().is_none());
        assert!(p.finish(first));
        assert!(!p.is_active());
        assert!(p.begin().is_some());
    }

    #[test]
    fn a_cancelled_check_result_is_stale() {
        let mut p = Pending::default();
        let old = p.begin().unwrap();
        p.cancel();
        assert!(!p.is_active());
        let new = p.begin().unwrap();
        assert!(!p.finish(old));
        assert!(p.is_active());
        assert!(p.finish(new));
    }

    fn damaged(id: &str, stops: bool) -> Damaged {
        use swofty_core::addons::files::{Damage, Problem};
        Damaged {
            id: id.into(),
            name: format!("Liveries {id}"),
            damage: vec![Damage {
                path: "assets/lang.json".into(),
                problem: Problem::Empty,
                stops,
            }],
        }
    }

    #[test]
    fn a_damaged_file_the_build_reads_stops_play_with_the_addon_named() {
        assert_eq!(damaged_stop(&[damaged("a", false)]), None);
        assert_eq!(
            damaged_stop(&[damaged("a", true), damaged("b", false)]).unwrap(),
            "Liveries a is damaged: assets/lang.json is empty. Reinstall it, then press Play."
        );
        assert_eq!(
            damaged_stop(&[damaged("a", true), damaged("b", true)]).unwrap(),
            "Liveries a, Liveries b are damaged. Reinstall them, then press Play."
        );
    }

    #[test]
    fn play_stops_before_the_build_on_an_on_addon_with_an_empty_op_file() {
        let root = std::env::temp_dir().join(format!("swofty-play-damaged-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let src = root.join("src");
        std::fs::create_dir_all(src.join("assets")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            r#"{"schema_version":1,"id":"a.real","name":"Real Liveries","version":"1.1.7","author":"a",
                "operations":{"assets":["assets/lang.json"]}}"#,
        )
        .unwrap();
        std::fs::write(src.join("assets/lang.json"), r#"{"schema_version":1,"ops":[]}"#).unwrap();
        let store = Store::new(root.join("data"));
        store.create_profile("p", None).unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("p", |p| p.set_enabled("a.real", true))
            .unwrap();
        let inputs = || Inputs {
            store: store.clone(),
            profile: "p".into(),
            game: root.join("game"),
            build_id: None,
        };
        assert!(run(inputs()).damaged.is_empty());

        std::fs::write(store.addons_dir().join("a.real/assets/lang.json"), b"").unwrap();
        let checked = run(inputs());
        assert_eq!(checked.damaged.len(), 1);
        assert_eq!(
            checked.verdict,
            Verdict::Stop(
                "Real Liveries is damaged: assets/lang.json is empty. Reinstall it, then press Play.".into()
            )
        );
        assert!(
            checked
                .lines
                .contains(&"[play] files: a.real assets/lang.json is empty".to_owned()),
            "{:?}",
            checked.lines
        );
        assert!(checked.timings.line().contains("files "));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn play_stops_before_the_launch_on_two_addons_that_conflict_and_names_them() {
        let root = std::env::temp_dir().join(format!("swofty-play-conflict-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let store = Store::new(root.join("data")).with_builtins(swofty_builtins::all());
        for (id, name, extra) in [
            (
                "william09leo.heavier-damage-realism",
                "Heavier Damage + 2026 Realism Performance",
                r#","conflicts":[{"id":"formula-nascar.handling"}]"#,
            ),
            ("formula-nascar.handling", "Formula NASCAR", ""),
            ("a.needs", "Needs Missing", r#","requires":[{"id":"a.missing"}]"#),
        ] {
            let src = root.join("src").join(id);
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(
                src.join("swofty.json"),
                format!(
                    r#"{{"schema_version":1,"id":"{id}","name":"{name}","version":"1.0.0","author":"a"{extra}}}"#
                ),
            )
            .unwrap();
            store.install_addon(&src).unwrap();
        }
        store.create_profile("p", None).unwrap();
        let on = |ids: &[&str]| {
            store
                .update_profile("p", |p| {
                    for id in [
                        "william09leo.heavier-damage-realism",
                        "formula-nascar.handling",
                        "a.needs",
                    ] {
                        p.set_enabled(id, ids.contains(&id))?;
                    }
                    Ok(())
                })
                .unwrap();
            run(Inputs {
                store: store.clone(),
                profile: "p".into(),
                game: root.join("game"),
                build_id: None,
            })
        };
        let checked = on(&["william09leo.heavier-damage-realism", "formula-nascar.handling"]);
        assert_eq!(
            checked.verdict,
            Verdict::Stop(
                "Heavier Damage + 2026 Realism Performance conflicts with Formula NASCAR. Turn one of them off \
                 on the Addons page"
                    .into()
            )
        );
        assert!(checked.timings.line().contains("resolve "));
        assert_eq!(on(&["formula-nascar.handling"]).verdict, Verdict::Go);
        assert_eq!(
            on(&["a.needs"]).verdict,
            Verdict::Go,
            "the build reports the other resolver errors"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn play_goes_on_for_an_addon_without_a_file_list_that_has_an_empty_placeholder_archive() {
        let root = std::env::temp_dir().join(format!("swofty-play-placeholder-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let src = root.join("src");
        std::fs::create_dir_all(src.join("erp")).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            r#"{"schema_version":1,"id":"a.season","name":"Season","version":"1.0.10","author":"a",
                "operations":{"erp":["erp/frontend.json"]}}"#,
        )
        .unwrap();
        std::fs::write(src.join("erp/frontend.json"), r#"{"schema_version":1,"ops":[]}"#).unwrap();
        let store = Store::new(root.join("data"));
        store.create_profile("p", None).unwrap();
        store.install_addon(&src).unwrap();
        store
            .update_profile("p", |p| p.set_enabled("a.season", true))
            .unwrap();
        let dir = store.addons_dir().join("a.season");
        std::fs::remove_file(dir.join(swofty_core::addons::files::INDEX_FILE)).unwrap();
        std::fs::write(dir.join("erp/placeholder.erp"), b"").unwrap();
        let checked = run(Inputs {
            store: store.clone(),
            profile: "p".into(),
            game: root.join("game"),
            build_id: None,
        });
        assert!(
            !matches!(checked.verdict, Verdict::Stop(_)),
            "{:?}",
            checked.verdict
        );
        assert!(
            checked
                .lines
                .contains(&"[play] files: a.season warning: erp/placeholder.erp is empty".to_owned()),
            "{:?}",
            checked.lines
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
