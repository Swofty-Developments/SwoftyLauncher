use super::crash_record::{Fatal, frames_start_with};
use super::evidence::Evidence;
use super::wording::SEND_LOGS;
use super::{Crash, Diagnosis};

const FRAMES: [u64; 3] = [0x391a834, 0x311b55, 0x3d3d84a];

pub(super) fn render_slots_crash(e: &Evidence, f: &Fatal) -> bool {
    e.game_build == Some(crate::build_support::BUILD_1_0_145) && frames_start_with(f, 0, &FRAMES)
}

pub(super) fn render_slots(c: &Crash) -> Option<Diagnosis> {
    render_slots_crash(c.evidence, &c.fatal).then(|| Diagnosis {
        id: "render_model_pool",
        title: "F1 25 ran out of room for 3D model parts".to_owned(),
        what: "The game ran out of a fixed internal pool for 3D model parts while loading the session. This has \
               been seen on full grids, often with Driver Display and Project Halo both on, and turning one of \
               them off has helped."
            .to_owned(),
        steps: vec![
            "Press Play again.".to_owned(),
            format!("If it crashes again, {SEND_LOGS} Press it straight after the crash."),
        ],
    })
}
