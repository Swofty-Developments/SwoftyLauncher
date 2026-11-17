use super::*;

const AGGRESSIVE_AI_HEAP: [&str; 7] = [
    "F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4",
    "F1_25.exe+0x395fa23 .xtls Ne_ReportMemoryError+0x343",
    "F1_25.exe+0x39658d8 .xtls fn base+0x39657d0+0x108",
    "F1_25.exe+0x39640e3 .xtls Ne_Heap_DeallocateNonSmall+0x163",
    "F1_25.exe+0x3963f5f .xtls Ne_Heap_Free+0xdf",
    "F1_25.exe+0x67c1f2 .xtls fn base+0x67bf30+0x2c2",
    "F1_25.exe+0x8303f43 .data1 fn base+0x8303f00+0x43",
];

const AGGRESSIVE_AI_HEAP_EA: [&str; 7] = [
    "F1_25.exe+0x3905334 .rdata fn base+0x3905330+0x4",
    "F1_25.exe+0x394cab3 .rdata fn base+0x394c770+0x343",
    "F1_25.exe+0x3952968 .rdata fn base+0x3952860+0x108",
    "F1_25.exe+0x3951173 .rdata fn base+0x3951010+0x163",
    "F1_25.exe+0x3950fef .rdata fn base+0x3950f10+0xdf",
    "F1_25.exe+0x67cc02 .rdata fn base+0x67c940+0x2c2",
    "F1_25.exe+0x80a5f03 .xcode fn base+0x80a5ec0+0x43",
];

fn frames_log(frames: &[&str]) -> String {
    let at = frames[0];
    let mut log = format!(
        "2026-11-15 13:13:10.664 [2668] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 2668, 2607 s after load, at {at}\n\
             2026-11-15 13:13:10.664 [2668] [crash]   reading 0x8\n\
             2026-11-15 13:13:10.666 [2668] [crash]   stack:\n"
    );
    for (i, frame) in frames.iter().enumerate() {
        log.push_str(&format!(
            "2026-11-15 13:13:10.666 [2668] [crash]   #{i:<2} {frame}\n"
        ));
    }
    log.push_str(&format!(
            "2026-11-15 13:13:10.667 [2668] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at {at} reached the top-level filter; passing it to the game's filter\n"
        ));
    log
}

fn aggressive_ai_on(loader: &str, version: &str) -> Evidence {
    let mut e = with_names(
        ev(loader, &[]),
        &[(AGGRESSIVE_AI, "F1 25 + 26 Aggressive AI (Alternate)")],
    );
    e.versions.insert(AGGRESSIVE_AI.to_owned(), version.to_owned());
    e
}

#[test]
fn a_session_load_crash_on_aggressive_ai_alternate_3_0_0_names_it() {
    let faces: [&[&str]; 10] = [
        &AGGRESSIVE_AI_HEAP,
        &[
            "F1_25.exe+0x67c959 .xtls fn base+0x67c680+0x2d9",
            "F1_25.exe+0x8303f69 .data1 fn base+0x8303f00+0x69",
        ],
        &[
            "F1_25.exe+0x67b49b .xtls fn base+0x67b3f0+0xab",
            "F1_25.exe+0x8308ef7 .data1 fn base+0x8308d50+0x1a7",
        ],
        &[
            "F1_25.exe+0x67b952 .xtls fn base+0x67b3f0+0x562",
            "F1_25.exe+0x8014c5c .data1 fn base+0x8014a90+0x1cc",
        ],
        &[
            "F1_25.exe+0x67b9aa .xtls fn base+0x67b3f0+0x5ba",
            "F1_25.exe+0x8014c5c .data1 fn base+0x8014a90+0x1cc",
        ],
        &[
            "0x1c4096c0 (no module)",
            "F1_25.exe+0x67b4a1 .xtls fn base+0x67b3f0+0xb1",
            "F1_25.exe+0x8014c5c .data1 fn base+0x8014a90+0x1cc",
        ],
        &AGGRESSIVE_AI_HEAP_EA,
        &[
            "F1_25.exe+0x67d369 .rdata fn base+0x67d090+0x2d9",
            "F1_25.exe+0x80a5f29 .xcode fn base+0x80a5ec0+0x69",
        ],
        &[
            "F1_25.exe+0x67beab .rdata fn base+0x67be00+0xab",
            "F1_25.exe+0x7e84a49 .xcode fn base+0x7e84870+0x1d9",
        ],
        &[
            "F1_25.exe+0x67c3ba .rdata fn base+0x67be00+0x5ba",
            "F1_25.exe+0x7e84a49 .xcode fn base+0x7e84870+0x1d9",
        ],
    ];
    for face in faces {
        let d = diagnose(&aggressive_ai_on(&frames_log(face), "3.0.0"));
        assert_eq!(d.map(|d| d.id), Some("aggressive_ai_alternate"), "{face:?}");
    }
    let split = frames_log(faces[4]).replace(
            "2026-11-15 13:13:10.666 [2668] [crash]   stack:\n",
            "2026-11-15 13:13:10.666 [9540] [crash] writing a crash report: unhandled ACCESS_VIOLATION (0xc0000005)\n",
        );
    let d = diagnose(&aggressive_ai_on(&split, "3.0.0"));
    assert_eq!(d.map(|d| d.id), Some("aggressive_ai_alternate"));
    let d = diagnose(&aggressive_ai_on(&frames_log(&AGGRESSIVE_AI_HEAP), "3.0.0")).unwrap();
    assert_eq!(
        d.title,
        "F1 25 + 26 Aggressive AI (Alternate) 3.0.0 crashed F1 25 while a session loaded"
    );
    assert!(d.what.contains("has been taken down"), "{}", d.what);
    assert!(
        d.steps[0].contains("install it again from the Addon Browser"),
        "{}",
        d.steps[0]
    );
    assert_eq!(
        d.steps[1],
        "Or turn off F1 25 + 26 Aggressive AI (Alternate) in your profile."
    );
}

#[test]
fn the_aggressive_ai_alternate_crash_is_read_from_the_crash_report() {
    let mut e = aggressive_ai_on(
        "2026-11-15 20:38:35.219 [9112] [crash] Ne heap error 0x16 (fatal) on thread 9112: Failed to find address 0000000399450C30 in the allocated list; heap 000000006CB5F0B0 'RACE', capacity 10485760, used 2440304\n",
        "3.0.0",
    );
    e.report = Some(
            "reason       fatal Ne heap error 0x16 in heap 'RACE': Failed to find address 0000000399450C30 in the allocated list\n\
             == Stack (unwound with the modules' exception data)\n  \
             #0  00007ffe3e9d6f94 rsp=000000006f2ff290  swofty_core.dll+0xd6f94 .text fn +0xd6990+0x604\n  \
             #1  00007ffe3e9d68db rsp=000000006f2ff990  swofty_core.dll+0xd68db .text fn +0xd68b0+0x2b\n  \
             #2  00000001439658d8 rsp=000000006f2ff9e0  F1_25.exe+0x39658d8 .xtls fn base+0x39657d0+0x108\n  \
             #3  00000001439640e3 rsp=000000006f2ffad0  F1_25.exe+0x39640e3 .xtls Ne_Heap_DeallocateNonSmall+0x163\n  \
             #4  0000000143963f5f rsp=000000006f2ffb70  F1_25.exe+0x3963f5f .xtls Ne_Heap_Free+0xdf\n  \
             #5  000000014067c1f2 rsp=000000006f2ffbc0  F1_25.exe+0x67c1f2 .xtls fn base+0x67bf30+0x2c2\n  \
             #6  0000000148303f43 rsp=000000006f2ffc60  F1_25.exe+0x8303f43 .data1 fn base+0x8303f00+0x43\n"
                .to_owned(),
        );
    assert_eq!(diagnose(&e).map(|d| d.id), Some("aggressive_ai_alternate"));
}

fn skip_cutscenes_on(loader: &str, version: &str, build: (u64, u32)) -> Evidence {
    let mut e = with_names(ev(loader, &[]), &[(SKIP_CUTSCENES, "Skip Cutscenes")]);
    e.versions.insert(SKIP_CUTSCENES.to_owned(), version.to_owned());
    e.enabled = vec![SKIP_CUTSCENES.to_owned()];
    e.game_build = Some(build);
    e
}

#[test]
fn skip_cutscenes_up_to_1_9_4_on_1_0_145_is_named() {
    let log = "2026-11-15 13:13:00.000 [1] [loader] vtable not found (Sess_SessionStateIntroCameras_OnEnter 142D6DE80)\n\
                   2026-11-15 13:13:40.000 [2] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at skip_cutscenes.dll+0x23f4 reached the top-level filter; passing it to the game's filter\n";
    for version in ["1.9.4", "1.0.0", "1.9.3"] {
        let d = diagnose(&skip_cutscenes_on(
            log,
            version,
            crate::build_support::BUILD_1_0_145,
        ))
        .unwrap();
        assert_eq!(d.id, "skip_cutscenes_1_0_145", "{version}");
    }
    let d = diagnose(&skip_cutscenes_on(
        log,
        "1.9.4",
        crate::build_support::BUILD_1_0_145,
    ))
    .unwrap();
    assert_eq!(
        d.what,
        "Skip Cutscenes (up to 1.9.4) crashes the game on F1 25 version 1.0.145 within a minute of loading."
    );
    assert_eq!(
        d.steps,
        ["Turn it off on the Addons page until its author releases an update."]
    );
}

#[test]
fn skip_cutscenes_is_not_named_on_other_versions_builds_or_when_off() {
    let log = "2026-11-15 13:13:00.000 [1] [loader] vtable not found (Sess_SessionStateIntroCameras_OnEnter 142D6DE80)\n";
    let on = crate::build_support::BUILD_1_0_145;
    assert_eq!(diagnose(&skip_cutscenes_on(log, "1.9.5", on)), None);
    assert_eq!(
        diagnose(&skip_cutscenes_on(log, "1.9.4", (408_063_872, 1_789_118_616))),
        None
    );
    let mut off = skip_cutscenes_on(log, "1.9.4", on);
    off.enabled.clear();
    assert_eq!(diagnose(&off), None);
}

#[test]
fn other_aggressive_ai_alternate_versions_and_crashes_are_not_named() {
    let heap = frames_log(&AGGRESSIVE_AI_HEAP);
    for version in ["2.0.1", "3.0.1", "3.1.0", "13.0.0"] {
        assert_eq!(diagnose(&aggressive_ai_on(&heap, version)), None, "{version}");
    }
    let mut off = aggressive_ai_on(&heap, "3.0.0");
    off.enabled.clear();
    assert_eq!(diagnose(&off), None);
    assert_eq!(diagnose(&ev(&heap, &[])), None);
    let unrelated = frames_log(&[
        "F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b",
        "F1_25.exe+0x445b18c .xtls fn base+0x445aea0+0x2ec",
    ]);
    assert_eq!(diagnose(&aggressive_ai_on(&unrelated, "3.0.0")), None);
    let deep: Vec<&str> =
        std::iter::repeat_n("F1_25.exe+0x445b18c .xtls fn base+0x445aea0+0x2ec", NEAR_FRAMES)
            .chain(["F1_25.exe+0x67c1f2 .xtls fn base+0x67bf30+0x2c2"])
            .collect();
    assert_eq!(diagnose(&aggressive_ai_on(&frames_log(&deep), "3.0.0")), None);
}

fn pitlane_log(kept: &str, mode: &str, address: &str) -> String {
    format!(
            "2026-10-27 19:26:15.735 [30872] [pitlane] your car: flags 0 -> 0x1, mode 0 -> 1
2026-10-27 19:26:15.741 [30872] [pitlane] pit lane: control kept (7 this session){kept}
2026-10-27 19:26:15.761 [30872] [pitlane] your car: flags 0x1 -> 0, mode 1 -> {mode}
2026-10-27 19:26:15.767 [12860] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 12860, 2079 s after load, at F1_25.exe+{address} .xtls fn base+0x389a120+0xb7
2026-10-27 19:26:15.767 [12860] [crash]   reading 0x2d3bf4f40
2026-10-27 19:26:15.767 [12860] [crash]   stack:
2026-10-27 19:26:15.767 [12860] [crash]   #0  F1_25.exe+{address} .xtls fn base+0x389a120+0xb7
2026-10-27 19:26:15.767 [12860] [crash]   #1  F1_25.exe+0x445b18c .xtls fn base+0x445aea0+0x2ec
2026-10-27 19:26:15.768 [12860] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+{address} .xtls fn base+0x389a120+0xb7 reached the top-level filter; passing it to the game's filter
"
        )
}

#[test]
fn a_crash_as_manual_pit_lane_takes_the_car_names_it() {
    let e = with_names(
        ev(&pitlane_log("", "1474065524", "0x389a1d7"), &[]),
        &[("kizi.pitlane", "Manual Pit Lane")],
    );
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "pitlane_garage");
    assert_eq!(d.title, "Manual Pit Lane crashed F1 25 in the pit lane");
    assert!(d.what.contains("Return to Garage or a flashback"), "{}", d.what);
    assert_eq!(
        d.steps[0],
        "Turn off Manual Pit Lane in your profile until its author fixes this crash."
    );
    let older_game = pitlane_log("", "1159247034", "0x38873a7");
    assert_eq!(diagnose(&ev(&older_game, &[])).unwrap().id, "pitlane_garage");
    let kept_only = pitlane_log("", "0", "0x389a1d7");
    assert_eq!(diagnose(&ev(&kept_only, &[])).unwrap().id, "pitlane_garage");
    let ai_kept = pitlane_log(
        " - another flag is set too, so the AI keeps the car for now",
        "0",
        "0x389a1d7",
    );
    assert_eq!(diagnose(&ev(&ai_kept, &[])), None);
    assert_eq!(diagnose(&ev(&pitlane_log("", "0", "0x1234567"), &[])), None);
    let later = pitlane_log("", "1474065524", "0x389a1d7").replace("19:26:15.7", "19:26:19.7");
    let later = later.replacen("19:26:19.7", "19:26:15.7", 3);
    assert_eq!(diagnose(&ev(&later, &[])), None);
}
