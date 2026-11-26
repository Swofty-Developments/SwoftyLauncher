use super::broken_addons::{ea_1_0_145, steam_1_0_145};
use super::crash_record::{Fatal, crash_time, frames_start_with, stamp};
use super::evidence::Evidence;
use super::wording::SEND_LOGS;
use super::{Crash, Diagnosis};

struct Shape {
    fault: u64,
    tail: [u64; 4],
}

const STEAM: Shape = Shape {
    fault: 0x133db6e2,
    tail: [0x42fb742, 0x4169697, 0x44a5726, 0x3ac3203],
};
const EA: Shape = Shape {
    fault: 0x11b3d2eb,
    tail: [0x42ef6a2, 0x415d887, 0x449a0d6, 0x3ab0223],
};
const EXITING: &str = "\"StateSession\" Updating -> Exiting";
const EXIT_SECS: f64 = 5.0;

fn tail_at(f: &Fatal, tail: &[u64]) -> bool {
    (1..f.frames.len()).any(|skip| frames_start_with(f, skip, tail))
}

fn after_exit(e: &Evidence, f: &Fatal) -> bool {
    let Some(end) = crash_time(e, f) else {
        return false;
    };
    e.loader
        .lines()
        .filter(|l| l.contains(EXITING))
        .filter_map(stamp)
        .any(|t| t <= end && end - t <= EXIT_SECS)
}

pub(super) fn session_exit_crash(e: &Evidence, f: &Fatal) -> bool {
    let shape = if steam_1_0_145(e) {
        &STEAM
    } else if ea_1_0_145(e) {
        &EA
    } else {
        return false;
    };
    frames_start_with(f, 0, &[shape.fault])
        && tail_at(f, &shape.tail)
        && f.record.iter().any(|l| l.trim_start().starts_with("writing 0x"))
        && after_exit(e, f)
}

pub(super) fn session_exit(c: &Crash) -> Option<Diagnosis> {
    session_exit_crash(c.evidence, &c.fatal).then(|| Diagnosis {
        id: "session_exit_fault",
        title: "F1 25 crashed as a session ended".to_owned(),
        what: "F1 25 crashed on its own as a session ended and the next one was loading, for example after \
               qualifying. This is a fault inside the game, it is known, and there is no fix for it yet."
            .to_owned(),
        steps: vec!["Press Play again.".to_owned(), SEND_LOGS.to_owned()],
    })
}
