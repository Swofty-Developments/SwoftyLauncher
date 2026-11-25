use super::*;

const POOL_FRAMES: [&str; 5] = [
    "F1_25.exe+0x391a834 .xtls Ne_FatalAbort+0x4",
    "F1_25.exe+0x311b55 .xtls fn base+0x311ac0+0x95",
    "F1_25.exe+0x3d3d84a .xtls fn base+0x3d3c640+0x120a",
    "F1_25.exe+0x399543e .xtls fn base+0x3995300+0x13e",
    "F1_25.exe+0x3ac3203 .xtls fn base+0x3ac3100+0x103",
];

const GPU_FRAMES: [&str; 3] = [
    "F1_25.exe+0x391a834 .xtls Ne_FatalAbort+0x4",
    "F1_25.exe+0x39b8802 .xtls fn base+0x39b8510+0x2f2",
    "F1_25.exe+0x3d3d84a .xtls fn base+0x3d3c640+0x120a",
];

fn crash_with(frames: &[&str], registers: &str) -> String {
    let at = frames[0];
    let mut log = format!(
        "2026-11-25 14:53:01.100 [2668] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 2668, 611 s after load, at {at}\n\
         2026-11-25 14:53:01.100 [2668] [crash]   writing 0x1\n\
         2026-11-25 14:53:01.100 [2668] [crash]   {registers}\n\
         2026-11-25 14:53:01.101 [2668] [crash]   stack:\n"
    );
    for (i, frame) in frames.iter().enumerate() {
        log.push_str(&format!(
            "2026-11-25 14:53:01.101 [2668] [crash]   #{i:<2} {frame}\n"
        ));
    }
    log.push_str(&format!(
        "2026-11-25 14:53:01.200 [2668] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at {at} reached the top-level filter; passing it to the game's filter\n"
    ));
    log
}

fn on_steam(loader: &str) -> Evidence {
    let mut e = ev(loader, &[]);
    e.game_build = Some(crate::build_support::BUILD_1_0_145);
    e
}

#[test]
fn the_model_part_pool_running_out_is_recognised() {
    let log = crash_with(
        &POOL_FRAMES,
        "rsi=00000001c3603440 rdi=0000000000000000 rbp=0000000000000000",
    );
    let d = diagnose(&on_steam(&log)).unwrap();
    assert_eq!(d.id, "render_model_pool");
    assert!(
        d.what.contains("fixed internal pool for 3D model parts"),
        "{}",
        d.what
    );
    assert!(d.what.contains("Driver Display and Project Halo"), "{}", d.what);
    assert_eq!(d.steps[0], "Press Play again.");
    assert!(d.steps[1].contains("Driver Display"), "{}", d.steps[1]);
    assert!(d.steps[2].contains("Send Logs"), "{}", d.steps[2]);
}

#[test]
fn the_model_part_pool_wins_over_the_graphics_driver_rule_when_the_gpu_codes_show() {
    let log = crash_with(
        &POOL_FRAMES,
        "rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a0006",
    );
    assert_eq!(diagnose(&on_steam(&log)).map(|d| d.id), Some("render_model_pool"));
}

#[test]
fn the_gpu_hang_sharing_the_abort_frame_is_not_the_model_part_pool() {
    let log = crash_with(
        &GPU_FRAMES,
        "rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a0006",
    );
    assert_eq!(diagnose(&on_steam(&log)).map(|d| d.id), Some("gpu_removed"));
}

#[test]
fn the_model_part_pool_needs_the_steam_build() {
    let log = crash_with(
        &POOL_FRAMES,
        "rsi=00000001c3603440 rdi=0000000000000000 rbp=0000000000000000",
    );
    let mut e = on_steam(&log);
    e.game_build = Some((391_000_000, 1_794_315_435));
    assert_eq!(diagnose(&e), None);
    e.game_build = None;
    assert_eq!(diagnose(&e), None);
}

#[test]
fn a_longer_offset_is_not_the_same_frame() {
    let mut frames = POOL_FRAMES;
    frames[1] = "F1_25.exe+0x311b550 .xtls fn base+0x311ac0+0x95";
    let log = crash_with(&frames, "rsi=0000000000000000");
    assert_eq!(diagnose(&on_steam(&log)), None);
}
