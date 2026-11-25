use super::*;

const EXIT_FRAMES: [&str; 8] = [
    "F1_25.exe+0x133db6e2 .trace fn base+0x133db5e0+0x102",
    "F1_25.exe+0x431a191 .xtls fn base+0x431a100+0x91",
    "F1_25.exe+0x42fb742 .xtls fn base+0x42fb700+0x42",
    "F1_25.exe+0x42fb742 .xtls fn base+0x42fb700+0x42",
    "F1_25.exe+0x4169697 .xtls fn base+0x4169600+0x97",
    "F1_25.exe+0x44a5726 .xtls fn base+0x44a5700+0x26",
    "F1_25.exe+0x3ac3203 .xtls fn base+0x3ac3100+0x103",
    "F1_25.exe+0x3ac75a1 .xtls fn base+0x3ac7500+0xa1",
];

const SIBLING_FRAMES: [&str; 8] = [
    "F1_25.exe+0x43700c6 .xtls fn base+0x436ffc0+0x106",
    "F1_25.exe+0x436c210 .xtls fn base+0x436c200+0x10",
    "F1_25.exe+0x436c0f4 .xtls fn base+0x436c0c0+0x34",
    "F1_25.exe+0x42fb7e0 .xtls fn base+0x42fb7c0+0x20",
    "F1_25.exe+0x42fb742 .xtls fn base+0x42fb700+0x42",
    "F1_25.exe+0x4169697 .xtls fn base+0x4169600+0x97",
    "F1_25.exe+0x44a5726 .xtls fn base+0x44a5700+0x26",
    "F1_25.exe+0x3ac3203 .xtls fn base+0x3ac3100+0x103",
];

fn exit_crash(frames: &[&str], access: &str, exiting_at: &str) -> String {
    let at = frames[0];
    let mut log = format!(
        "2026-11-25 16:53:00.044 [4288] [career] flow \"StateSession\" Updating -> Updating\n\
         2026-11-25 {exiting_at} [4288] [career] flow \"StateSession\" Updating -> Exiting\n\
         2026-11-25 16:53:08.884 [4288] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 4288, 2988 s after load, at {at}\n\
         2026-11-25 16:53:08.884 [4288] [crash]   {access}\n\
         2026-11-25 16:53:08.884 [4288] [crash]   rax=00000002ea427980\n\
         2026-11-25 16:53:08.884 [4288] [crash]   stack:\n"
    );
    for (i, frame) in frames.iter().enumerate() {
        log.push_str(&format!(
            "2026-11-25 16:53:08.885 [4288] [crash]   #{i:<2} {frame}\n"
        ));
    }
    log.push_str(&format!(
        "2026-11-25 16:53:08.990 [4288] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at {at} reached the top-level filter; passing it to the game's filter\n"
    ));
    log
}

fn on_steam(loader: &str) -> Evidence {
    let mut e = ev(loader, &[]);
    e.game_build = Some(crate::build_support::BUILD_1_0_145);
    e
}

#[test]
fn a_write_fault_as_a_session_ends_is_recognised() {
    let log = exit_crash(&EXIT_FRAMES, "writing 0x2ea427985", "16:53:08.800");
    let d = diagnose(&on_steam(&log)).unwrap();
    assert_eq!(d.id, "session_exit_fault");
    assert!(d.what.contains("as a session ended"), "{}", d.what);
    assert!(d.what.contains("no fix"), "{}", d.what);
    assert_eq!(d.steps[0], "Press Play again.");
    assert!(d.steps[1].contains("Send Logs"), "{}", d.steps[1]);
}

#[test]
fn the_sibling_fault_with_the_same_tail_is_left_to_other_rules() {
    let log = exit_crash(&SIBLING_FRAMES, "writing 0x2e9fff1b0", "16:53:08.800");
    assert_ne!(
        diagnose(&on_steam(&log)).map(|d| d.id),
        Some("session_exit_fault")
    );
}

#[test]
fn a_different_first_frame_is_not_the_session_exit_fault() {
    let mut frames = EXIT_FRAMES;
    frames[0] = "F1_25.exe+0x133db6e3 .trace fn base+0x133db5e0+0x103";
    let log = exit_crash(&frames, "writing 0x2ea427985", "16:53:08.800");
    assert_eq!(diagnose(&on_steam(&log)), None);
}

#[test]
fn a_read_fault_or_a_late_crash_is_not_the_session_exit_fault() {
    let log = exit_crash(&EXIT_FRAMES, "reading 0x2ea427985", "16:53:08.800");
    assert_eq!(diagnose(&on_steam(&log)), None);
    let log = exit_crash(&EXIT_FRAMES, "writing 0x2ea427985", "16:52:40.000");
    assert_eq!(
        diagnose(&on_steam(&log)),
        None,
        "the session ended long before the crash"
    );
    let log = exit_crash(&EXIT_FRAMES, "writing 0x2ea427985", "16:53:09.500");
    assert_eq!(
        diagnose(&on_steam(&log)),
        None,
        "the session ended after the crash"
    );
}

#[test]
fn the_session_exit_fault_needs_the_steam_build() {
    let log = exit_crash(&EXIT_FRAMES, "writing 0x2ea427985", "16:53:08.800");
    let mut e = on_steam(&log);
    e.game_build = Some((391_000_000, 1_794_315_435));
    assert_eq!(diagnose(&e), None);
}
