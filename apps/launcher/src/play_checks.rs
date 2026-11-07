use std::path::PathBuf;
use std::time::{Duration, Instant};

use swofty_core::data_usage::Room;
use swofty_core::store::Store;

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
    Confirm(String),
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
    pub anyway: bool,
}

pub(crate) struct Checked {
    pub profile: String,
    pub game: PathBuf,
    pub gate: Gate,
    pub notes: Vec<Note>,
    pub verdict: Verdict,
    pub room: Room,
    pub timings: Timings,
}

pub(crate) fn run(inputs: Inputs) -> Checked {
    let Inputs {
        store,
        profile,
        game,
        anyway,
    } = inputs;
    let mut timings = Timings::default();
    let gate = timings.time("gate", || Gate {
        write_blocked: swofty_core::compat::write_blocker(&game).is_some(),
        setup_done: swofty_core::setup::done(&store.root),
        offline_accepted: swofty_core::offline::is_accepted(&store.root),
        first_launch: swofty_core::setup::first_launch(&store.root),
    });
    let mut notes = Vec::new();
    let verdict = match (
        timings.time("profile", || store.profile(&profile)),
        timings.time("addons", || store.addons()),
    ) {
        (Ok(p), Ok(installed)) => {
            let catalog = timings.time("catalog", || {
                swofty_core::catalog::cached(&store.root).unwrap_or_default()
            });
            timings.time("lonely", || {
                notes.extend(lonely(&store, &p, &installed, &catalog))
            });
            registry(&store, &game, &p, &installed, anyway, &mut notes, &mut timings)
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
    }
}

fn lonely(
    store: &Store,
    profile: &swofty_core::profile::Profile,
    installed: &std::collections::BTreeMap<String, swofty_core::manifest::AddonManifest>,
    catalog: &[swofty_core::catalog::Addon],
) -> Vec<Note> {
    let mut notes = Vec::new();
    for (base, family) in swofty_core::resolver::lonely_bases(profile, installed, catalog) {
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

fn lonely_note(base: &str, family: &[String]) -> Note {
    let shown = family.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
    let more = family.len().saturating_sub(4);
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Note::warning(
        &format!("{base} needs one of its addons"),
        format!(
            "{base} is on, but none of its addons are, so it adds nothing to pick on its own. Install at \
             least one in the Addon Browser, such as {shown}{more}, and turn it on."
        ),
    )
}

fn registry(
    store: &Store,
    game: &std::path::Path,
    profile: &swofty_core::profile::Profile,
    installed: &std::collections::BTreeMap<String, swofty_core::manifest::AddonManifest>,
    anyway: bool,
    notes: &mut Vec<Note>,
    timings: &mut Timings,
) -> Verdict {
    use swofty_core::stdlib::{careers, deps};
    if profile.is_vanilla() {
        let found = timings.time("careers", || {
            careers::sync(&store.root, Some(game), None).unwrap_or_default()
        });
        let using = found.iter().filter(|c| !c.addons.is_empty()).count();
        notes.extend(vanilla_note(using));
        return Verdict::Go;
    }
    let Ok(plan) = timings.time("resolve", || swofty_core::resolver::resolve(profile, installed)) else {
        return Verdict::Go;
    };
    let all = timings.time("deps", || deps::read_all(store, installed));
    let report = timings.time("deps check", || {
        deps::for_order(&profile.name, &plan.order, installed, &all)
    });
    let (saved, scoped) = timings.time("careers", || {
        let saved = careers::read(&careers::data_path(&store.root));
        let scoped: Vec<_> = careers::in_save_of(&saved, &store.root, &profile.name)
            .into_iter()
            .cloned()
            .collect();
        (saved, scoped)
    });
    let stop = careers::stop_line(&scoped, &plan.order, installed);
    let play = careers::play_line(&saved, &plan.order, installed);
    let findings = report.findings.iter().map(finding_note).collect();
    assemble(stop, play, findings, report.stop_text(), anyway, notes)
}

fn vanilla_note(using: usize) -> Option<Note> {
    let text = match using {
        0 => return None,
        1 => format!(
            "1 career uses addons; loading it in {} can lose its progress.",
            swofty_core::profile::VANILLA
        ),
        n => format!(
            "{n} careers use addons; loading them in {} can lose their progress.",
            swofty_core::profile::VANILLA
        ),
    };
    Some(Note::warning("Careers that need addons", text))
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
    career_stop: Option<String>,
    career_play: Option<String>,
    findings: Vec<Note>,
    dependency_stop: Option<String>,
    anyway: bool,
    notes: &mut Vec<Note>,
) -> Verdict {
    if let Some(stop) = career_stop {
        if anyway {
            notes.push(Note::warning("Careers that need addons", stop));
        } else {
            return Verdict::Confirm(stop);
        }
    }
    notes.extend(career_play.map(|line| Note::warning("Careers that need addons", line)));
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
    fn career_stop_asks_before_anything_else() {
        let mut notes = Vec::new();
        let v = assemble(
            Some("stop".into()),
            Some("play".into()),
            vec![note("f")],
            None,
            false,
            &mut notes,
        );
        assert_eq!(v, Verdict::Confirm("stop".into()));
        assert!(notes.is_empty());
    }

    #[test]
    fn play_anyway_turns_the_stop_into_a_warning_first() {
        let mut notes = Vec::new();
        let v = assemble(
            Some("stop".into()),
            Some("play".into()),
            vec![note("f")],
            None,
            true,
            &mut notes,
        );
        assert_eq!(v, Verdict::Go);
        let bodies: Vec<_> = notes.iter().map(|n| n.body.as_str()).collect();
        assert_eq!(bodies, ["stop", "play", "f"]);
        assert!(notes[0].sticky);
    }

    #[test]
    fn dependency_stop_keeps_its_findings_and_blocks() {
        let mut notes = Vec::new();
        let v = assemble(None, None, vec![note("f")], Some("dep".into()), false, &mut notes);
        assert_eq!(v, Verdict::Stop("dep".into()));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn nothing_found_goes_on() {
        let mut notes = Vec::new();
        assert_eq!(assemble(None, None, vec![], None, false, &mut notes), Verdict::Go);
        assert!(notes.is_empty());
    }

    #[test]
    fn vanilla_note_counts_careers() {
        assert!(vanilla_note(0).is_none());
        assert!(vanilla_note(1).unwrap().body.starts_with("1 career uses addons"));
        assert!(vanilla_note(3).unwrap().body.starts_with("3 careers use addons"));
    }

    #[test]
    fn lonely_note_names_four_and_counts_the_rest() {
        let family: Vec<String> = (1..=6).map(|i| format!("S{i}")).collect();
        let n = lonely_note("Base", &family);
        assert!(n.body.contains("S1, S2, S3, S4 and 2 more"));
        assert_eq!(n.title, "Base needs one of its addons");
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
}
