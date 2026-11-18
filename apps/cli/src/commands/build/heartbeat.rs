use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use swofty_core::game_folder::busy::{Busy, Step};

#[derive(Default)]
pub(super) struct Copied(pub(super) AtomicU64, pub(super) AtomicU64);

/// While a build and install run, a line every 30 s that they moved, and one
/// a minute while a single step runs long, so the launcher's ten-minute limit
/// on a silent wrapper stops only one that is stuck. A session's build also
/// keeps `state/build.json` current.
pub(super) fn heartbeat(done: &AtomicBool, data: &Path, mut busy: Option<Busy>, copied: &Copied) {
    const NOTE: Duration = Duration::from_secs(2);
    let first = swofty_core::work();
    let mut beat = Beat::new(first, swofty_core::doing(), Instant::now());
    let mut noted: Option<Instant> = None;
    while !done.load(Ordering::Relaxed) {
        if let Some(b) = busy.as_mut()
            && noted.is_none_or(|n| n.elapsed() >= NOTE)
        {
            noted = Some(Instant::now());
            track(b, swofty_core::work() - first, copied);
            b.write(data).ok();
        }
        std::thread::sleep(Duration::from_millis(250));
        if let Some(line) = beat.line(swofty_core::work(), swofty_core::doing(), Instant::now()) {
            crate::console::output::say("info", &line);
        }
    }
}

pub(super) struct Beat {
    start: Instant,
    first: u64,
    last: u64,
    doing: String,
    tick: Instant,
    said: Instant,
}

impl Beat {
    const EVERY: Duration = Duration::from_secs(30);
    const QUIET: Duration = Duration::from_secs(60);

    pub(super) fn new(work: u64, doing: String, at: Instant) -> Self {
        Self {
            start: at,
            first: work,
            last: work,
            doing,
            tick: at,
            said: at,
        }
    }

    pub(super) fn line(&mut self, work: u64, step: String, at: Instant) -> Option<String> {
        if at.saturating_duration_since(self.tick) < Self::EVERY {
            return None;
        }
        self.tick = at;
        let moved = work != self.last || step != self.doing;
        if !moved && at.saturating_duration_since(self.said) < Self::QUIET {
            return None;
        }
        self.said = at;
        self.last = work;
        self.doing = step;
        Some(format!(
            "build: still working after {} s ({} file(s) and fragment(s) so far{})",
            at.saturating_duration_since(self.start).as_secs(),
            work - self.first,
            match self.doing.is_empty() {
                true => String::new(),
                false => format!("; {}", self.doing),
            }
        ))
    }
}

pub(super) fn track(b: &mut Busy, work: u64, copied: &Copied) {
    let total = copied.1.load(Ordering::Relaxed);
    if total == 0 {
        b.step = Step::Building;
        b.done = work;
    } else {
        b.step = Step::Copying;
        b.done = copied.0.load(Ordering::Relaxed);
        b.total = total;
    }
}
