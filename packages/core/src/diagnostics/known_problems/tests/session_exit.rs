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

const EA_FRAMES: [&str; 9] = [
    "F1_25.exe+0x11b3d2eb .xtls fn base+0x11b3d1f0+0xfb",
    "F1_25.exe+0x430e0f1 .xtls fn base+0x430e0a0+0x51",
    "F1_25.exe+0x42ef6a2 .xtls fn base+0x42ef660+0x42",
    "F1_25.exe+0x42ef6a2 .xtls fn base+0x42ef660+0x42",
    "F1_25.exe+0x415d887 .xtls fn base+0x415d800+0x87",
    "F1_25.exe+0x449a0d6 .xtls fn base+0x449a0a0+0x36",
    "F1_25.exe+0x3ab0223 .xtls fn base+0x3ab0100+0x123",
    "F1_25.exe+0x3ab45c1 .xtls fn base+0x3ab4500+0xc1",
    "F1_25.exe+0x390d78a .xtls fn base+0x390d700+0x8a",
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

fn on_ea(loader: &str) -> Evidence {
    let mut e = ev(loader, &[]);
    e.game_build = Some(crate::build_support::BUILD_1_0_145_EA);
    e
}

#[test]
fn the_ea_build_fault_as_a_session_ends_is_recognised() {
    let log = exit_crash(&EA_FRAMES, "writing 0x2ea427985", "16:53:08.800");
    let d = diagnose(&on_ea(&log)).unwrap();
    assert_eq!(d.id, "session_exit_fault");
    assert!(d.what.contains("as a session ended"), "{}", d.what);
}

#[test]
fn the_ea_build_with_another_fault_is_not_the_session_exit_fault() {
    let mut frames = EA_FRAMES;
    frames[0] = "F1_25.exe+0x11b3d2ec .xtls fn base+0x11b3d1f0+0xfc";
    let log = exit_crash(&frames, "writing 0x2ea427985", "16:53:08.800");
    assert_eq!(diagnose(&on_ea(&log)), None);
    let log = exit_crash(&EXIT_FRAMES, "writing 0x2ea427985", "16:53:08.800");
    assert_eq!(diagnose(&on_ea(&log)), None);
    let log = exit_crash(&EA_FRAMES, "writing 0x2ea427985", "16:53:08.800");
    assert_eq!(diagnose(&on_steam(&log)), None);
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

const FULL: &str = "2026-11-25 16:52:20.100 [4288] [perf] last 60 s: memory: RAM 15200/16384 MiB used, commit 37100/40000 MiB used; game working set 6500 MiB, private 7200 MiB; VRAM gpu 0 5100/11000 MiB local used/budget, 120/7000 MiB shared\n";
const ROOMY: &str = "2026-11-25 16:52:20.100 [4288] [perf] last 60 s: memory: RAM 9000/32768 MiB used, commit 22000/40000 MiB used; game working set 6500 MiB, private 7200 MiB; VRAM gpu 0 5100/11000 MiB local used/budget, 120/7000 MiB shared\n";

fn sibling(extra: &str) -> Evidence {
    let log = exit_crash(&SIBLING_FRAMES, "writing 0x2e9fff1b0", "16:53:08.800");
    on_steam(&format!("{extra}{log}"))
}

#[test]
fn the_sibling_fault_names_a_nearly_full_memory() {
    let d = diagnose(&sibling(FULL)).unwrap();
    assert_eq!(d.id, "session_exit_memory");
    assert_eq!(d.title, "F1 25 crashed as a session ended");
    assert!(d.what.contains("fault inside the game"), "{}", d.what);
    assert!(d.what.contains("commit was 92% full"), "{}", d.what);
    assert!(d.what.contains("memory was nearly full"), "{}", d.what);
    let steps = d.text();
    assert!(steps.contains("Turn off add-ons you don't need"), "{steps}");
    assert!(steps.contains("Close other apps"), "{steps}");
    assert!(steps.contains(super::super::memory::PAGE_FILE_STEP), "{steps}");
    assert!(!steps.contains("Classic Seasons"), "{steps}");
    assert!(!steps.contains("32000"), "{steps}");
    assert!(steps.contains("Send Logs"), "{steps}");
}

#[test]
fn the_sibling_fault_without_a_full_commit_does_not_blame_memory() {
    for extra in ["", ROOMY] {
        let d = diagnose(&sibling(extra)).unwrap();
        assert_eq!(d.id, "session_exit_memory");
        assert!(!d.what.contains("commit was"), "{}", d.what);
        assert!(!d.what.contains("nearly full."), "{}", d.what);
    }
}

#[test]
fn the_sibling_fault_is_only_matched_on_its_own_conditions() {
    let log = exit_crash(&SIBLING_FRAMES, "reading 0x2e9fff1b0", "16:53:08.800");
    assert_eq!(diagnose(&on_steam(&format!("{FULL}{log}"))), None);
    let log = exit_crash(&SIBLING_FRAMES, "writing 0x2e9fff1b0", "16:52:40.000");
    assert_eq!(diagnose(&on_steam(&format!("{FULL}{log}"))), None);
    let log = exit_crash(&SIBLING_FRAMES, "writing 0x2e9fff1b0", "16:53:08.800");
    assert_eq!(diagnose(&on_ea(&format!("{FULL}{log}"))), None);
    let mut frames = SIBLING_FRAMES;
    frames[0] = "F1_25.exe+0x43700c7 .xtls fn base+0x436ffc0+0x107";
    let log = exit_crash(&frames, "writing 0x2e9fff1b0", "16:53:08.800");
    assert_eq!(diagnose(&on_steam(&format!("{FULL}{log}"))), None);
}

#[test]
fn the_exit_fault_rule_still_does_not_take_the_sibling() {
    let log = exit_crash(&SIBLING_FRAMES, "writing 0x2e9fff1b0", "16:53:08.800");
    assert_ne!(
        diagnose(&on_steam(&log)).map(|d| d.id),
        Some("session_exit_fault")
    );
}

#[test]
fn the_commit_percentage_comes_from_the_newest_minute_line() {
    let newer = ROOMY.replace("16:52:20", "16:52:50");
    let e = sibling(&format!("{FULL}{newer}"));
    assert_eq!(super::super::memory::commit_percent(&e), Some(55));
    let e = sibling(FULL);
    assert_eq!(super::super::memory::commit_percent(&e), Some(92));
    assert_eq!(super::super::memory::commit_percent(&sibling("")), None);
    let none =
        "2026-11-25 16:52:20.100 [4288] [perf] last 60 s: memory: system RAM unavailable; VRAM unavailable\n";
    assert_eq!(super::super::memory::commit_percent(&sibling(none)), None);
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
