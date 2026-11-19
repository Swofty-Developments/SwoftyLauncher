//! How long each step between Play and the game starting took, and each step of the rollback after it.
//! The wrapper prints them as `[play] phase <name> <ms>` lines, so a Send Logs bundle carries them: `<name>`
//! is a dotted lowercase word and `<ms>` whole milliseconds.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The tag the wrapper prints these lines under.
pub const TAG: &str = "play";

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static STEP: Mutex<Option<(&'static str, Instant)>> = Mutex::new(None);
static SUMS: Mutex<Vec<(&'static str, Duration)>> = Mutex::new(Vec::new());

/// Lines kept for a process that builds without printing them, such as the launcher's own checks.
const KEPT: usize = 512;

fn lines() -> std::sync::MutexGuard<'static, Vec<String>> {
    LINES.lock().unwrap_or_else(|e| e.into_inner())
}

fn push(line: String) {
    let mut lines = lines();
    if lines.len() >= KEPT {
        lines.remove(0);
    }
    lines.push(line);
}

/// The text after the tag for a phase that took `took`.
pub fn line(name: &str, took: Duration) -> String {
    format!("phase {name} {}", took.as_millis())
}

pub fn record(name: &str, took: Duration) {
    push(line(name, took));
}

/// A finding about the Play path that is not a timing, such as why a saved build was not reused.
pub fn note(text: impl Into<String>) {
    push(text.into());
}

/// The lines recorded since the last call, oldest first; the step still running is closed first, and
/// each phase timed in parts comes last as one line.
pub fn take() -> Vec<String> {
    end_step();
    let sums = std::mem::take(&mut *SUMS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut out = std::mem::take(&mut *lines());
    out.extend(sums.into_iter().map(|(name, took)| line(name, took)));
    out
}

/// Times `f` as a part of the phase `name`, which is printed once with the time of all its parts.
pub fn time_part<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    let took = start.elapsed();
    let mut sums = SUMS.lock().unwrap_or_else(|e| e.into_inner());
    match sums.iter_mut().find(|(n, _)| *n == name) {
        Some((_, sum)) => *sum += took,
        None => sums.push((name, took)),
    }
    out
}

/// Times `f` as the phase `name`.
pub fn time<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    record(name, start.elapsed());
    out
}

/// The phase a build or install progress step belongs to. Consecutive steps of one phase, such as each
/// addon's `applying`, are timed together.
fn step_phase(step: &str) -> &'static str {
    match step {
        "reading the addons" => "build.inputs",
        "editing the game database" => "build.database",
        "placing files" => "build.place_files",
        "compressing the changed fragments" => "build.compress",
        "collecting the runtime resources" => "build.runtime",
        "saving the build" => "build.save",
        "checking the game files" => "install.check",
        "copying the mods to the game drive" => "install.link_cache",
        "copying the mods into the game folder" => "install.place",
        s if s.starts_with("applying ") => "build.apply",
        s if s.starts_with("writing ") => "build.write",
        _ => "step.other",
    }
}

/// Starts timing the phase of the progress step `step`, closing the one before when it is another phase.
pub(crate) fn step(step: &str) {
    let phase = step_phase(step);
    let mut current = STEP.lock().unwrap_or_else(|e| e.into_inner());
    if current.is_some_and(|(name, _)| name == phase) {
        return;
    }
    if let Some((name, start)) = current.replace((phase, Instant::now())) {
        record(name, start.elapsed());
    }
}

/// Closes the phase of the last progress step, at the end of a build or install.
pub fn end_step() {
    let done = STEP.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((name, start)) = done {
        record(name, start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phase_line_is_a_dotted_name_and_whole_milliseconds() {
        assert_eq!(
            line("install.link_cache", Duration::from_micros(12_345_678)),
            "phase install.link_cache 12345"
        );
    }

    #[test]
    fn a_phase_timed_in_parts_is_one_line_after_the_rest() {
        time_part("test.parts", || std::thread::sleep(Duration::from_millis(3)));
        record("test.whole", Duration::from_millis(9));
        time_part("test.parts", || std::thread::sleep(Duration::from_millis(3)));
        let lines: Vec<String> = take()
            .into_iter()
            .filter(|l| l.starts_with("phase test."))
            .collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], "phase test.whole 9");
        let ms: u64 = lines[1]
            .strip_prefix("phase test.parts ")
            .unwrap()
            .parse()
            .unwrap();
        assert!(ms >= 6, "{ms}");
    }

    #[test]
    fn every_progress_step_has_a_phase_and_each_addon_applied_shares_one() {
        for step in [
            "reading the addons",
            "editing the game database",
            "compressing the changed fragments",
            "collecting the runtime resources",
            "placing files",
            "saving the build",
            "checking the game files",
            "copying the mods to the game drive",
            "copying the mods into the game folder",
        ] {
            assert_ne!(step_phase(step), "step.other", "{step}");
            assert!(!step_phase(step).contains(' '));
        }
        assert_eq!(
            step_phase("applying swofty.core"),
            step_phase("applying kizi.ai-tweaks")
        );
        assert_eq!(step_phase("writing a.erp"), "build.write");
    }
}
