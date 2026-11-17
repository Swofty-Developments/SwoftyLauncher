use std::collections::{BTreeMap, BTreeSet};

use crate::conflicts::Pair;
use crate::launch::LOW_DISK;
use crate::manifest::AddonManifest;

use super::broken_addons::*;
use super::career::*;
use super::crash_record::*;
use super::evidence::*;
use super::game_version::*;
use super::graphics::*;
use super::stall::*;
use super::system::*;
use super::tyres::*;
use super::*;

const TYRES_LOG: &str = "\
2026-11-03 10:00:10.000 [1] [tyres] car 23 (team 4, AI): fitted tyres were never placed on the wheels (3 of 3 fitted tyre model(s) still at the spawn position -1000 and size 60/30, session 12, 10 s in); the car has no visible tyres
2026-11-03 10:00:10.000 [1] [tyres] car 24 (team 4, human): fitted tyres were never placed on the wheels (3 of 3 fitted tyre model(s) still at the spawn position -1000 and size 60/30, session 12, 10 s in); the car has no visible tyres
2026-11-03 10:00:10.000 [1] [tyres] fitted tyre check (10 s in), session 12: 2 of 26 car(s) have unplaced tyres (cars 23, 24)
2026-11-03 10:01:00.000 [1] [tyres] car 23 (team 4, AI): fitted tyres were never placed on the wheels (3 of 3 fitted tyre model(s) still at the spawn position -1000 and size 60/30, session 12, 60 s in); the car has no visible tyres
";

#[test]
fn unplaced_tyres_name_the_cars() {
    let d = unplaced_tyres(&ev(TYRES_LOG, &[])).expect("the tyre lines are recognised");
    assert_eq!(d.id, "tyres_not_placed");
    assert!(
        d.what.contains("your car (car 24) and 1 other car(s)"),
        "{}",
        d.what
    );
    let (human, ai) = unplaced_cars(TYRES_LOG);
    assert_eq!(human, BTreeSet::from([24]));
    assert_eq!(ai, BTreeSet::from([23]));
}

#[test]
fn unplaced_ai_cars_are_listed() {
    let log = "2026-11-03 10:00:10.000 [1] [tyres] car 25 (team 9, AI): fitted tyres were never placed on the wheels (3 of 3 x)\n";
    let d = unplaced_tyres(&ev(log, &[])).unwrap();
    assert!(d.what.contains("1 car(s) (cars 25)"), "{}", d.what);
}

#[test]
fn a_healthy_tyre_check_is_not_a_problem() {
    let log = "2026-11-03 10:00:10.000 [1] [tyres] fitted tyre check (10 s in), session 12: all 26 car(s) have their fitted tyres placed\n";
    assert!(unplaced_tyres(&ev(log, &[])).is_none());
    assert!(unplaced_tyres(&ev("", &[])).is_none());
}

const GPU: &str = "\
2026-10-24 02:52:26.633 [33008] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 33008, 463 s after load, at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-10-24 02:52:26.633 [33008] [crash]   writing 0x1
2026-10-24 02:52:26.633 [33008] [crash]   rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a002b rsp=000000000015be50
2026-10-24 02:52:26.633 [33008] [crash]   stack:
2026-10-24 02:52:26.633 [33008] [crash]   #0  F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-10-24 02:52:26.633 [33008] [crash]   #1  F1_25.exe+0x39b8802 .xtls fn base+0x39b8510+0x2f2
2026-10-24 02:52:27.289 [20344] [crash] crash report swofty\\crash-20261024-025226.txt: the game aborted itself
2026-10-24 02:52:27.289 [33008] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
";

fn crash_in(module: &str) -> String {
    format!(
            "2026-10-26 14:01:42.000 [1] [crash] #1 first-chance (the game may still handle it) BREAKPOINT (0x80000003) on thread 1, 21 s after load, at {module}+0x178fd
2026-10-26 14:01:42.000 [1] [crash]   stack:
2026-10-26 14:01:42.000 [1] [crash]   #0  {module}+0x178fd
2026-10-26 14:01:42.000 [1] [crash]   #1  F1_25.exe+0x1234 .xtls fn base+0x1200+0x34
2026-10-26 14:01:42.100 [1] [crash] unhandled BREAKPOINT (0x80000003) at {module}+0x178fd reached the top-level filter; passing it to the game's filter
"
        )
}

fn ev(loader: &str, files: &[&str]) -> Evidence {
    Evidence {
        loader: loader.to_owned(),
        game_files: files.iter().map(|f| (*f).to_owned()).collect(),
        ..Default::default()
    }
}

const INIT_ABORT: &str = "\
2026-11-01 15:30:27.529 [21084] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 21084, 5 s after load, at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-11-01 15:30:27.529 [21084] [crash]   writing 0x1
2026-11-01 15:30:27.529 [21084] [crash]   stack:
2026-11-01 15:30:27.529 [21084] [crash]   #0  F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-11-01 15:30:27.529 [21084] [crash]   #1  F1_25.exe+0x43c07f1 .xtls Game_Startup_InitSequence+0x291
2026-11-01 15:30:27.529 [21084] [crash]   #2  F1_25.exe+0x43e0af1 .xtls fn base+0x43e0ad0+0x21
2026-11-01 15:30:27.958 [21084] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
2026-11-01 15:30:29.407 [21084] [crash] TerminateProcess(4294967295) on thread 21084, 6 s after load, before the game's window; caller stack:
";

fn init_abort(line: &str) -> Evidence {
    ev(
        &format!("2026-11-01 15:30:22.500 [21084] {line}\n{INIT_ABORT}"),
        &[],
    )
}

#[test]
fn a_startup_abort_on_a_cloud_only_settings_file_is_named() {
    let d = diagnose(&init_abort(
            "[hwsettings] C:\\Users\\m\\OneDrive\\Dokumente\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml: onedrive=yes exists=yes size=5120 attributes=0x00400020 cloud-only=yes (recall_on_data_access) read test: skipped",
        ))
        .expect("a diagnosis");
    assert_eq!(d.id, "hardware_settings_unreadable");
    assert!(d.what.contains("in the cloud only"));
    assert_eq!(d.steps.len(), 3);
}

#[test]
fn a_startup_abort_with_documents_in_onedrive_is_named_softly() {
    let d = diagnose(&init_abort(
            "[hwsettings] C:\\Users\\m\\OneDrive\\Documents\\x.xml: onedrive=yes exists=yes size=5120 attributes=0x00000020 cloud-only=no read test: ok (4096 bytes)",
        ))
        .expect("a diagnosis");
    assert_eq!(d.id, "hardware_settings_unreadable");
    assert!(d.what.contains("is in OneDrive"));
}

#[test]
fn a_startup_abort_with_local_documents_is_not_this_rule() {
    let d = diagnose(&init_abort(
        "[hwsettings] C:\\Users\\v\\Documents\\x.xml: onedrive=no exists=yes size=5120 attributes=0x00000020 cloud-only=no read test: ok (4096 bytes)",
    ));
    assert!(d.is_none_or(|d| d.id != "hardware_settings_unreadable"));
    let silent = ev(INIT_ABORT, &[]);
    assert!(diagnose(&silent).is_none_or(|d| d.id != "hardware_settings_unreadable"));
}

#[test]
fn another_abort_with_a_cloud_file_is_not_this_rule() {
    let other = init_abort("[hwsettings] p: onedrive=yes exists=yes size=1 attributes=0x00400020 cloud-only=yes (recall_on_data_access)")
            .loader
            .replace("InitSequence+0x291", "InitSequence+0x148");
    assert!(diagnose(&ev(&other, &[])).is_none_or(|d| d.id != "hardware_settings_unreadable"));
}

#[test]
fn reads_the_fatal_exception_and_its_stack() {
    let f = fatal(&ev(GPU, &[]));
    assert_eq!(f.module.as_deref(), Some("f1_25.exe"));
    assert_eq!(f.stack, ["f1_25.exe", "f1_25.exe"]);
    assert_eq!(report_name(GPU).as_deref(), Some("crash-20261024-025226.txt"));
    let fallback = "2026-11-01 12:53:50.100 [5] [crash] crash report C:\\Users\\id\\AppData\\Local\\Swofty\\state\\crash-20261101-125350.txt: boom\n";
    assert_eq!(
        report_name(fallback).as_deref(),
        Some("crash-20261101-125350.txt")
    );
}

#[test]
fn a_removed_gpu_is_the_driver() {
    let d = diagnose(&ev(GPU, &["F1_25.exe", "sl.common.dll"])).unwrap();
    assert_eq!(d.id, "gpu_removed");
    assert!(d.what.contains("not from Swofty's code"), "{}", d.what);
    assert!(d.steps[0].starts_with("Update your graphics driver"));
}

#[test]
fn a_removed_gpu_with_graphics_mods_names_them_first() {
    let d = diagnose(&ev(
        GPU,
        &["F1_25.exe", "dxgi.dll", "ReShade.ini", "dlss5-feed.addon64"],
    ))
    .unwrap();
    assert_eq!(d.id, "gpu_removed");
    assert!(d.what.contains("ReShade"), "{}", d.what);
    assert!(
        d.steps[0].contains("dxgi.dll, ReShade.ini and dlss5-feed.addon64"),
        "{}",
        d.steps[0]
    );
}

/// A report's session: the overlay drew on the main menu only, the game aborted in its own
/// resource creation half a minute later, and Present had returned 0.
const GPU_LONG_AFTER_OVERLAY: &str = "\
2026-11-14 10:35:11.303 [12808] [overlay] at start the game's hardware settings (C:\\Users\\id\\Documents\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml) have frame generation on (frame_gen mode 1, multi_frame_gen 3); drawing before frame generation is on
2026-11-14 10:35:23.897 [12808] [overlay] api: alexs.silly-season on_paint -> ok (token 1)
2026-11-14 10:35:23.917 [12808] [overlay] api: kizi.ai-tweaks on_paint -> ok (token 2)
2026-11-14 10:35:38.567 [12808] [overlay] the game's hardware settings (C:\\Users\\id\\Documents\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml) have frame generation on (frame_gen mode 1, multi_frame_gen 3)
2026-11-14 10:37:02.225 [17276] [overlay] first overlay frame submitted: 2820 vertices
2026-11-14 10:37:38.712 [12808] [overlay] last 60 s: 281 frame(s) drawn, 0 skipped while the GPU still held the slot; GPU 0.031 ms per drawn frame (278 timed)
2026-11-14 10:38:11.319 [17276] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 17276, 183 s after load, at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-11-14 10:38:11.319 [17276] [crash]   writing 0x1
2026-11-14 10:38:11.319 [17276] [crash]   rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a0001 rsp=000000007d08f470
2026-11-14 10:38:11.320 [17276] [crash]   stack:
2026-11-14 10:38:11.320 [17276] [crash]   #0  F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-11-14 10:38:11.320 [17276] [crash]   #1  F1_25.exe+0x39b8802 .xtls Render_Check_GfxResult+0x2f2
2026-11-14 10:38:11.320 [17276] [crash]   #2  F1_25.exe+0x39b84b5 .xtls Render_Gfx2_PlacedResourceForward_C+0x25
2026-11-14 10:38:11.320 [17276] [crash]   #3  F1_25.exe+0x39b9c46 .xtls Render_Gfx2Heap_BumpAllocPlacedResource+0x1b6
2026-11-14 10:38:11.320 [17276] [overlay] the GPU was removed (present 0x00000000, reason 0x887a0001) more than 10 s after the overlay last drew; the overlay stops drawing
2026-11-14 10:38:13.897 [17276] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
";

/// A report's session: the device went 0.1 s after an overlay frame at a session's start, found
/// by the game's own resource creation and not by a Present.
const GPU_JUST_AFTER_OVERLAY: &str = "\
2026-11-14 11:10:28.332 [ 4324] [overlay] api: kizi.ai-tweaks on_paint -> ok (token 1)
2026-11-14 11:10:28.337 [ 4324] [overlay] api: dromodf.skip-cutscenes on_paint -> ok (token 2)
2026-11-14 11:10:28.372 [ 4324] [overlay] api: alexs.silly-season on_paint -> ok (token 3)
2026-11-14 11:10:35.253 [ 4324] [overlay] drawing on swapchain 000000007A32A040 with the direct queue 000000007BA5D780 from CreateSwapChainForHwnd (2560x1440, format 28, 3 buffer(s), created by sl.interposer.dll <- amd_fidelityfx_dx12.dll <- F1_25.exe <- KERNEL32.DLL <- ntdll.dll)
2026-11-14 11:23:28.091 [ 4324] [overlay] first overlay frame submitted: 3330 vertices
2026-11-14 11:24:34.769 [ 4324] [overlay] last 60 s: 227 frame(s) drawn, 0 skipped while the GPU still held the slot; GPU 0.038 ms per drawn frame (227 timed)
2026-11-14 11:25:46.381 [ 4324] [overlay] the GPU was removed (present 0x00000000, reason 0x887a0006) 0.1 s after the overlay last drew (swapchain by sl.interposer.dll <- amd_fidelityfx_dx12.dll <- F1_25.exe <- KERNEL32.DLL <- ntdll.dll); the overlay stops drawing
2026-11-14 11:25:46.384 [ 4324] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 4324, 949 s after load, at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-11-14 11:25:46.384 [ 4324] [crash]   rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a0006 rsp=000000000015bdf0
2026-11-14 11:25:46.384 [ 4324] [crash]   stack:
2026-11-14 11:25:46.384 [ 4324] [crash]   #0  F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
";

fn overlay_addons(log: &str) -> Evidence {
    with_names(
        ev(log, &["F1_25.exe"]),
        &[
            ("alexs.silly-season", "Silly Season"),
            ("kizi.ai-tweaks", "Contact Physics"),
            ("dromodf.skip-cutscenes", "Skip Cutscenes"),
        ],
    )
}

#[test]
fn a_gpu_removed_long_after_the_overlay_drew_is_not_the_overlay() {
    let d = diagnose(&overlay_addons(GPU_LONG_AFTER_OVERLAY)).unwrap();
    assert_eq!(d.id, "gpu_removed");
    assert_eq!(d.title, "Your graphics driver stopped responding");
    assert!(d.what.contains("Frame generation was on"), "{}", d.what);
    assert_eq!(d.steps[0], FRAME_GEN_OFF);
    let all = format!("{} {} {}", d.title, d.what, d.steps.join(" "));
    for word in ["overlay", "Silly Season", "Contact Physics"] {
        assert!(!all.contains(word), "{word}: {all}");
    }
}

#[test]
fn a_gpu_removed_before_the_overlay_drew_is_not_the_overlay() {
    let log = GPU_LONG_AFTER_OVERLAY.replace(
        "more than 10 s after the overlay last drew",
        "and the overlay had not drawn",
    );
    assert!(log.contains("and the overlay had not drawn"));
    assert_eq!(diagnose(&overlay_addons(&log)).unwrap().id, "gpu_removed");
    let unknown =
        GPU_LONG_AFTER_OVERLAY.replace("more than 10 s after the overlay last drew", "at some point");
    assert_eq!(diagnose(&overlay_addons(&unknown)).unwrap().id, "gpu_removed");
}

#[test]
fn a_gpu_the_game_found_removed_just_after_an_overlay_frame_is_the_driver() {
    let d = diagnose(&overlay_addons(GPU_JUST_AFTER_OVERLAY)).unwrap();
    assert_eq!(d.id, "gpu_removed");
    assert_eq!(d.title, "Your graphics driver stopped responding");
    assert!(!d.what.contains("overlay"), "{}", d.what);
    assert!(
        d.steps[0].starts_with("Update your graphics driver"),
        "{:?}",
        d.steps
    );
    let named: Vec<&String> = d.steps.iter().filter(|s| s.contains("Silly Season")).collect();
    assert_eq!(
        named,
        [
            "If it keeps happening, turn off Contact Physics, Skip Cutscenes and Silly Season in your profile \
                 for one session."
        ]
    );
    assert_eq!(d.steps.last(), Some(named[0]));
}

#[test]
fn a_present_that_returned_the_removal_just_after_an_overlay_frame_names_the_addons() {
    let log = GPU_JUST_AFTER_OVERLAY.replace("present 0x00000000", "present 0x887a0005");
    let d = diagnose(&overlay_addons(&log)).unwrap();
    assert_eq!(d.id, "gpu_removed_overlay");
    assert_eq!(
        d.steps[0],
        "Turn off Contact Physics, Skip Cutscenes and Silly Season in your profile, then play again."
    );
}

#[test]
fn a_removed_gpu_after_the_overlay_drew_names_the_addons() {
    let log = format!(
        "{GPU}2026-10-24 02:52:26.000 [1] [overlay] api: kizi.pitlane on_paint -> ok\n\
             2026-10-24 02:52:26.000 [1] [overlay] renderer up: d3d12 swapchain 1920x1080\n\
             2026-10-24 02:52:26.500 [1] [overlay] the GPU was removed (present 0x887a0005, reason 0x887a0006) 4.8 s after the overlay last drew; the overlay stops drawing\n"
    );
    let mut e = ev(&log, &[]);
    e.names.insert("kizi.pitlane".into(), "Manual Pit Lane".into());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "gpu_removed_overlay");
    assert_eq!(
        d.steps[0],
        "Turn off Manual Pit Lane in your profile, then play again."
    );
}

#[test]
fn a_crash_inside_a_hook_names_its_program() {
    let d = diagnose(&ev(&crash_in("windhawk.dll"), &[])).unwrap();
    assert_eq!(d.id, "third_party_hook");
    assert_eq!(d.title, "F1 25 crashed inside Windhawk");
    let d = diagnose(&ev(&crash_in("RTSSHooks64.dll"), &[])).unwrap();
    assert!(d.title.contains("RivaTuner"), "{}", d.title);
    let d = diagnose(&ev(&crash_in("pid.dll"), &[])).unwrap();
    assert!(d.title.contains("force feedback"), "{}", d.title);
}

#[test]
fn the_steam_overlay_counts_only_when_the_crash_is_in_it() {
    let log = crash_in("F1_25.exe").replace(
        "#1  F1_25.exe+0x1234 .xtls fn base+0x1200+0x34",
        "#1  GameOverlayRenderer64.dll+0x1234",
    );
    assert_eq!(diagnose(&ev(&log, &[])), None);
    let d = diagnose(&ev(&crash_in("GameOverlayRenderer64.dll"), &[])).unwrap();
    assert!(d.title.contains("Steam overlay"));
}

#[test]
fn graphics_mods_before_the_window_are_named() {
    let d = diagnose(&ev(
        &crash_in("sl.common.dll"),
        &[
            "F1_25.exe",
            "OptiScaler.ini",
            "nvngx.dll",
            "dxgi.dll",
            "OptiPatcher.asi",
        ],
    ))
    .unwrap();
    assert_eq!(d.id, "graphics_mods");
    assert!(d.title.starts_with("OptiScaler and an ASI loader"), "{}", d.title);
    assert!(d.what.contains("before its window opened"), "{}", d.what);
    assert!(d.steps[0].contains("OptiScaler.ini, nvngx.dll, dxgi.dll and OptiPatcher.asi"));
}

#[test]
fn graphics_mods_play_no_part_in_a_game_code_crash_after_the_window() {
    let log = format!(
        "2026-10-26 14:00:00.000 [1] trigger: main window visible\n{}",
        crash_in("F1_25.exe")
    );
    assert_eq!(
        diagnose(&ev(&log, &["F1_25.exe", "dxgi.dll", "ReShade.ini"])),
        None
    );
}

const REPLAY_LOADER: &str = "\
2026-11-12 17:57:27.502 [ 5736] stage: window (USER32!RegisterClassExW from 00000001439f00a0 (.xtls)), addons were loaded at the early stage
2026-11-12 18:23:14.352 [12208] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 12208, 1558 s after load, at F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c
2026-11-12 18:23:14.352 [12208] [crash]   reading 0xffffffffffffffff
2026-11-12 18:23:14.352 [12208] [crash]   rax=40000003c3bc3180 rbx=0000000000000000 rcx=00000003c3bc31e8 rdx=00000000693af410
2026-11-12 18:23:14.353 [12208] [crash]   stack:
2026-11-12 18:23:14.353 [12208] [crash]   #0  F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c
2026-11-12 18:23:14.353 [12208] [crash]   #1  F1_25.exe+0x3b8250 .xtls Game_Garage_ClearOverlayAllModels+0xd0
2026-11-12 18:23:14.353 [12208] [crash]   #2  F1_25.exe+0x3b80ac .xtls Car_PitStop_ApplyTyreAndWingVisuals+0x8ec
2026-11-12 18:23:14.353 [12208] [crash]   #3  F1_25.exe+0x3b7579 .xtls Car_Tyres_ApplyStateToCarPair+0x69
2026-11-12 18:23:14.353 [12208] [crash]   #4  F1_25.exe+0x7e3c9b4 .data1 fn base+0x7e3c870+0x144
2026-11-12 18:23:14.353 [12208] [crash]   #5  F1_25.exe+0x45ceeee .xtls Ne_ReplayRecordable_ReadFrom+0x151e
2026-11-12 18:23:14.353 [12208] [crash]   #6  F1_25.exe+0x45d3aee .xtls Game_ReplayStream_InterpolateBackwards+0x4ae
2026-11-12 18:23:14.353 [12208] [crash]   #7  F1_25.exe+0x45d71e4 .xtls Game_ReplaySystem_TargetTime+0x504
2026-11-12 18:23:14.353 [12208] [crash]   #8  F1_25.exe+0x45d6c56 .xtls Game_ReplaySystem_UpdateFromModule+0xc16
2026-11-12 18:23:14.353 [12208] [crash]   #9  F1_25.exe+0x2a37f0 .xtls fn base+0x2a37e0+0x10
2026-11-12 18:23:14.353 [12208] [crash]   #10 F1_25.exe+0x3ac0c33 .xtls Ne_JobQueue_TryAcquireJob+0x483
2026-11-12 18:23:14.353 [12208] [crash]   #11 F1_25.exe+0x3ac4fb3 .xtls fn base+0x3ac4a50+0x563
2026-11-12 18:23:14.353 [12208] [crash]   #12 F1_25.exe+0x391e16a .xtls fn base+0x391def0+0x27a
2026-11-12 18:23:14.353 [12208] [crash]   #13 KERNEL32.DLL+0x17374
2026-11-12 18:23:14.353 [12208] [crash]   #14 ntdll.dll+0x4cc91
2026-11-12 18:23:14.353 [12208] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c reached the top-level filter; passing it to the game's filter
";

const REPLAY_REPORT: &str = "\
Swofty crash report

reason       unhandled ACCESS_VIOLATION (0xc0000005) reading 0xffffffffffffffff at F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c
runtime      Windows

== Exception
  ACCESS_VIOLATION (0xc0000005), flags 0x0, at F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c

== Stack (unwound with the modules' exception data)
  #0  00000001401fbd2c rsp=00000000693af350  F1_25.exe+0x1fbd2c .xtls fn base+0x1fbd00+0x2c
  #1  00000001403b8250 rsp=00000000693af3e0  F1_25.exe+0x3b8250 .xtls Game_Garage_ClearOverlayAllModels+0xd0
  #2  00000001403b80ac rsp=00000000693af460  F1_25.exe+0x3b80ac .xtls Car_PitStop_ApplyTyreAndWingVisuals+0x8ec
  #3  00000001403b7579 rsp=00000000693af5f0  F1_25.exe+0x3b7579 .xtls Car_Tyres_ApplyStateToCarPair+0x69
  #4  0000000147e3c9b4 rsp=00000000693af640  F1_25.exe+0x7e3c9b4 .data1 fn base+0x7e3c870+0x144
  #5  00000001445ceeee rsp=00000000693af700  F1_25.exe+0x45ceeee .xtls Ne_ReplayRecordable_ReadFrom+0x151e
  #6  00000001445d3aee rsp=00000000693af870  F1_25.exe+0x45d3aee .xtls Game_ReplayStream_InterpolateBackwards+0x4ae
  #7  00000001445d71e4 rsp=00000000693af950  F1_25.exe+0x45d71e4 .xtls Game_ReplaySystem_TargetTime+0x504
  #8  00000001445d6c56 rsp=00000000693afa80  F1_25.exe+0x45d6c56 .xtls Game_ReplaySystem_UpdateFromModule+0xc16

== Return addresses found on the stack (rsp to +0x8000)
  rsp+0x88    00000001403b8250  F1_25.exe+0x3b8250 .xtls Game_Garage_ClearOverlayAllModels+0xd0
  rsp+0x3a8   00000001445ceeee  F1_25.exe+0x45ceeee .xtls Ne_ReplayRecordable_ReadFrom+0x151e
  rsp+0x3b8   00007ff8d501b6ae  KERNELBASE.dll+0x1b6ae .text fn +0x1b620+0x8e
  rsp+0x518   00000001445d3aee  F1_25.exe+0x45d3aee .xtls Game_ReplayStream_InterpolateBackwards+0x4ae
  rsp+0x528   00007ff87ef46586  d3d12.dll+0x6586 .text fn +0x64f0+0x96
  rsp+0x5f8   00000001445d71e4  F1_25.exe+0x45d71e4 .xtls Game_ReplaySystem_TargetTime+0x504
";

#[test]
fn a_graphics_module_only_in_the_stack_scan_does_not_blame_graphics_mods() {
    let mut e = ev(REPLAY_LOADER, &["F1_25.exe", "DINPUT8.dll"]);
    e.report = Some(REPLAY_REPORT.to_owned());
    assert!(diagnose(&e).is_none_or(|d| d.id != "graphics_mods"));
    let mut through_d3d12 = ev(
        &REPLAY_LOADER.replace(
            "#3  F1_25.exe+0x3b7579 .xtls Car_Tyres_ApplyStateToCarPair+0x69",
            "#3  d3d12.dll+0x6586",
        ),
        &["F1_25.exe", "DINPUT8.dll"],
    );
    through_d3d12.report = Some(REPLAY_REPORT.to_owned());
    let d = diagnose(&through_d3d12).expect("d3d12.dll is among the near frames");
    assert_eq!(d.id, "graphics_mods");
    assert_eq!(d.title, "an ASI loader in your game folder crashed F1 25");
}

#[test]
fn a_stale_driver_return_does_not_blame_optiscaler_for_a_core_crash() {
    let loader = "\
2026-11-12 10:26:30.000 [11396] stage: window (USER32!RegisterClassExW from 00000001439f00a0 (.xtls)), addons were loaded at the early stage
2026-11-12 10:27:57.733 [11396] [crash] unhandled exception (0xc0000028) at ntdll.dll+0x1d3df reached the top-level filter; passing it to the game's filter
";
    let report = "\
== Exception
  exception (0xc0000028), flags 0x81, at ntdll.dll+0x1d3df .text fn +0x1d390+0x4f

== Stack (unwound with the modules' exception data)
  #0  00007ffa1a2ed3df rsp=0000000067ffd1f0  ntdll.dll+0x1d3df .text fn +0x1d390+0x4f
  #1  00007ffa1a2edaee rsp=0000000067ffd790  ntdll.dll+0x1daee .text fn +0x1da30+0xbe
  #2  00007ffa17731da2 rsp=0000000067ffdec0  ucrtbase.dll+0xb1da2 .text fn +0xb1c80+0x122
  #3  00007ffa1a43ab3f rsp=0000000067ffdf30  ntdll.dll+0x16ab3f .text fn +0x16ab30+0xf
  #4  00007ffa1a396985 rsp=0000000067ffdf60  ntdll.dll+0xc6985 .text fn +0xc6390+0x5f5
  #5  00007ffa1a43a47e rsp=0000000067ffe6c0  ntdll.dll+0x16a47e .text fn +0x16a450+0x2e
  #6  00007ff92fecfda9 rsp=0000000067ffee78  swofty_core.dll+0x64fda9 .text
  #7  3f80000000000007 rsp=0000000067ffee80  0x3f80000000000007 (no module)

== Return addresses found on the stack (rsp to +0x8000)
  rsp+0x8     00007ff9fed046d3  nvwgf2umx.dll+0x4846d3 .text fn +0x483ce0+0x9f3
  rsp+0x30    00007ffa1a2ed3df  ntdll.dll+0x1d3df .text fn +0x1d390+0x4f
  rsp+0x908   00007ff9a057f54b  D3D12Core.dll+0x3f54b .text fn +0x3f1f0+0x35b
";
    let mut e = ev(loader, &["F1_25.exe", "OptiScaler.ini", "nvngx.dll"]);
    e.report = Some(report.to_owned());
    assert!(diagnose(&e).is_none_or(|d| d.id != "graphics_mods"));
}

#[test]
fn a_crash_in_a_graphics_driver_names_the_vendor() {
    let d = diagnose(&ev(&crash_in("amdxc64.dll"), &["F1_25.exe"])).unwrap();
    assert_eq!(d.id, "graphics_driver");
    assert_eq!(d.title, "F1 25 crashed inside your AMD graphics driver");
    let d = diagnose(&ev(
        &crash_in("amdxc64.dll"),
        &["F1_25.exe", "amdxc64.dll", "OptiScaler.ini"],
    ))
    .unwrap();
    assert_eq!(d.id, "graphics_mods", "OptiScaler's own amdxc64.dll");
}

#[test]
fn a_crash_in_frame_generation_says_turn_it_off() {
    let log = format!(
        "2026-10-26 14:00:00.000 [1] trigger: main window visible\n{}",
        crash_in("libxess_fg.dll")
    );
    let d = diagnose(&ev(&log, &["F1_25.exe"])).unwrap();
    assert_eq!(d.id, "frame_generation");
    assert_eq!(
        d.steps[0],
        "Turn off XeSS frame generation in F1 25's graphics settings, then play again."
    );
}

#[test]
fn running_out_of_memory_says_so() {
    let d = diagnose(&ev(&GPU.replace("887a0005", "8007000e"), &[])).unwrap();
    assert_eq!(d.id, "out_of_memory");
}

#[test]
fn under_wine_the_windows_driver_rules_stay_out() {
    let log = format!(
        "{}2026-10-21 07:05:50.000 [1] [offline] module: winex11.drv @ 0x6ffffde80000\n",
        GPU.replace("887a0005", "8007000e")
    );
    assert_eq!(diagnose(&ev(&log, &[])), None);
}

#[test]
fn a_plain_game_crash_has_no_diagnosis() {
    assert_eq!(
        diagnose(&ev(&crash_in("F1_25.exe"), &["F1_25.exe", "sl.common.dll"])),
        None
    );
    assert_eq!(diagnose(&ev("", &[])), None);
}

const WATCHDOG: &str = "\
2026-10-27 15:00:08.518 [3376] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 3376, 1083 s after load, at F1_25.exe+0x43e1113 .xtls fn base+0x43e10d0+0x43
2026-10-27 15:00:08.518 [3376] [crash]   writing 0x1
2026-10-27 15:00:08.518 [3376] [crash]   rax=0000000000000001 rbx=0000000000222200 rcx=00000000000077ae rdx=0000000000002065
2026-10-27 15:00:08.518 [3376] [crash]   r12=000000014604b4f0 r13=000000000351fe88 r14=00000000000077ae r15=0000000000007530
2026-10-27 15:00:08.519 [3376] [crash]   code: C7 00 BE CC 4B 28 48 83 C4 38 C3 CC CC 50 B8 01
2026-10-27 15:00:08.519 [3376] [crash]   stack:
2026-10-27 15:00:08.519 [3376] [crash]   #0  F1_25.exe+0x43e1113 .xtls fn base+0x43e10d0+0x43
2026-10-27 15:00:08.519 [3376] [crash]   #1  KERNEL32.DLL+0x2dda7
2026-10-27 15:00:08.520 [3376] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x43e1113 .xtls fn base+0x43e10d0+0x43 reached the top-level filter; passing it to the game's filter
";

fn with_names(mut e: Evidence, names: &[(&str, &str)]) -> Evidence {
    e.names = names
        .iter()
        .map(|(id, n)| ((*id).to_owned(), (*n).to_owned()))
        .collect();
    e.enabled = e.names.keys().cloned().collect();
    e
}

#[test]
fn the_hang_watchdog_is_a_freeze() {
    let d = diagnose(&ev(WATCHDOG, &["F1_25.exe"])).unwrap();
    assert_eq!(d.id, "hang_watchdog");
    assert_eq!(d.title, "F1 25 froze and closed itself");
    assert!(d.what.contains("stopped responding for 30 seconds"), "{}", d.what);
    assert!(d.steps[0].starts_with("Press Play again"), "{:?}", d.steps);
    let other = WATCHDOG.replace("r15=0000000000007530", "r15=0000000000000000");
    assert_eq!(diagnose(&ev(&other, &["F1_25.exe"])), None);
}

fn busy_log(tag: &str) -> String {
    let mut log = String::new();
    for s in 0..8 {
        log.push_str(&format!(
            "2026-10-27 14:59:{:02}.807 [21888] [{tag}] working on item {s}\n",
            45 + s
        ));
    }
    log.push_str(WATCHDOG);
    log
}

#[test]
fn a_freeze_inside_the_core_offers_no_turn_off_step() {
    let mut e = with_names(
        ev(&busy_log("core"), &["F1_25.exe"]),
        &[("swofty.core", "Swofty Core")],
    );
    e.builtin = BTreeSet::from(["swofty.core".to_owned()]);
    let d = diagnose(&e).unwrap();
    assert!(!d.text().contains("Turn off"), "{}", d.text());
    assert!(d.steps.iter().any(|s| s.contains("Send Logs")), "{:?}", d.steps);
}

#[test]
fn a_non_builtin_suspect_keeps_its_turn_off_step() {
    let e = with_names(
        ev(&busy_log("helmet-exchange"), &["F1_25.exe"]),
        &[
            ("giggles.helmet-exchange", "Helmet Exchange"),
            ("swofty.core", "Swofty Core"),
        ],
    );
    let mut e = e;
    e.builtin = BTreeSet::from(["swofty.core".to_owned()]);
    let d = diagnose(&e).unwrap();
    assert_eq!(
        d.steps[0],
        "Turn off Helmet Exchange in your profile, then play again."
    );
}

#[test]
fn the_hang_watchdog_names_the_addon_that_kept_the_game_busy() {
    let mut log = String::new();
    for s in 0..8 {
        log.push_str(&format!(
            "2026-10-27 14:59:{:02}.807 [21888] [helmet-exchange] thumbnail requested for helmet {s}\n",
            45 + s
        ));
    }
    log.push_str("2026-10-27 14:59:58.000 [5672] [progressive] standings read\n");
    log.push_str(WATCHDOG);
    let e = with_names(
        ev(&log, &["F1_25.exe"]),
        &[
            ("giggles.helmet-exchange", "Helmet Exchange"),
            ("swofty.progressive", "Progressive Career"),
        ],
    );
    let d = diagnose(&e).unwrap();
    assert!(
        d.what.contains("Helmet Exchange kept the game busy"),
        "{}",
        d.what
    );
    assert_eq!(
        d.steps[0],
        "Turn off Helmet Exchange in your profile, then play again."
    );

    let gap = format!(
        "2026-10-27 14:59:37.896 [14804] [storage] api: swofty.progressive (progressive_cards.dll) read data/years.json -> ok\n\
             2026-10-27 14:59:53.095 [28800] [game] WARN (warn): The signal timed out\n\
             2026-10-27 15:00:06.111 [14804] [progressive] gamedb labels loaded\n{WATCHDOG}"
    );
    let e = with_names(ev(&gap, &[]), &[("swofty.progressive", "Progressive Career")]);
    assert!(
        diagnose(&e)
            .unwrap()
            .what
            .contains("Progressive Career kept the game busy")
    );

    let idle = gap.replace("14:59:37.896", "14:58:20.000");
    let e = with_names(ev(&idle, &[]), &[("swofty.progressive", "Progressive Career")]);
    assert!(!diagnose(&e).unwrap().what.contains("kept the game busy"));
}

#[test]
fn the_hang_watchdog_names_a_driver_error_and_graphics_mods() {
    let log = format!(
        "2026-10-27 14:59:36.800 [9000] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 9000, 1050 s after load, at nvwgf2umx.dll+0x93d4c2\n\
             2026-10-27 14:59:36.800 [9000] [crash]   stack:\n\
             2026-10-27 14:59:36.800 [9000] [crash]   #0  nvwgf2umx.dll+0x93d4c2\n\
             2026-10-27 14:59:36.800 [9000] [crash]   #1  sl.dlss_g.dll+0x1234\n{}",
        WATCHDOG.replace("#1 first-chance", "#2 first-chance")
    );
    let d = diagnose(&ev(&log, &["F1_25.exe"])).unwrap();
    assert_eq!(d.id, "hang_watchdog");
    assert!(
        d.what
            .contains("nvwgf2umx.dll, part of the NVIDIA graphics driver during DLSS Frame Generation"),
        "{}",
        d.what
    );
    assert_eq!(d.steps[0], FRAME_GEN_OFF);
    let d = diagnose(&ev(WATCHDOG, &["F1_25.exe", "dxgi.dll", "OptiScaler.ini"])).unwrap();
    assert!(d.what.contains("OptiScaler"), "{}", d.what);
    assert!(
        d.steps[0].contains("dxgi.dll and OptiScaler.ini"),
        "{}",
        d.steps[0]
    );
}

fn perf(rows: &[&str], uptime: u32) -> String {
    format!(
        "Swofty Launcher performance report (format 1)\n\
             == Summary\n\
             uptime: {uptime} s\n\
             \n\
             == Timeline (last 15; each row runs until the next one)\n\
             at          took ms   frames   slow   worst ms  swofty ms  what\n{}\n",
        rows.join("\n")
    )
}

const SPLASH: &[&str] = &[
    "00:00:37      44811     2157      4        624      33.71  screen splash  [core flow tick 20.39 ms]",
    "00:01:27       1772      105      0         18      57.27  flow #a4d3a4b2 Exiting; flow #04fc8ffa Entering  [core flow tick 50.48 ms]",
    "00:01:29     232600    13654      7       1866     331.45  screen splash_transition  [core flow tick 306.18 ms]",
    "00:05:21      86418     5061      3       1219          -  flow #04fc8ffa Exiting (current)",
];

const LIVERY: (&str, &str) = ("crew8006.bp-ultimate-livery", "BP Ultimate Livery");

fn stuck(rows: &[&str], uptime: u32, names: &[(&str, &str)]) -> Evidence {
    with_names(
        Evidence {
            perf: Some(perf(rows, uptime)),
            ..Default::default()
        },
        names,
    )
}

#[test]
fn reads_the_last_screen_from_the_perf_timeline() {
    assert_eq!(perf_start_screen(&perf(SPLASH, 408)), Some(319.0));
    let menu = [
        SPLASH[2],
        "00:01:45      27019     2404     15        271     189.46  flow #04fc8ffa Exiting; screen playercard  [x]",
        "00:02:12        653       31      0         50      11.96  screen fe_home (current)",
    ];
    assert_eq!(perf_start_screen(&perf(&menu, 408)), None);
    assert_eq!(
        timeline_row(
            "00:04:44    2585578   469557      7       1420          -  screen splash_transition (current)"
        ),
        Some((284.0, "screen splash_transition"))
    );
    assert_eq!(
        timeline_row("at          took ms   frames   slow   worst ms  swofty ms  what"),
        None
    );
}

#[test]
fn a_stall_right_after_the_profile_load_points_at_the_save() {
    let load = "2026-11-04 03:32:34.875 [ 5256] [saves] profile load 1: manager 5 slot 2: gm_my_team id=16 \
                    key=66f051ee751e04d1 career=my_team:66f051ee751e04d1 flags=0x20 name=\"My Team Save 3\"\n";
    let mut e = stuck(SPLASH, 408, &[]);
    e.loader = load.into();
    let d = stalled(&e).unwrap();
    assert_eq!(d.id, "stalled_save");
    assert_eq!(
        d.title,
        "F1 25 stayed on the loading screen after reading your save"
    );
    assert!(d.what.contains("for 5 minutes"), "{}", d.what);
    assert_eq!(d.steps.len(), 2);
    let mut quick = stuck(SPLASH, 120, &[]);
    quick.loader = load.into();
    assert_eq!(stalled(&quick), None, "30 s is a normal load");
    assert_eq!(
        stalled(&stuck(SPLASH, 408, &[])),
        None,
        "no profile load, no save to blame"
    );
    let mut addon = stuck(SPLASH, 408, &[LIVERY]);
    addon.loader = load.into();
    assert_eq!(
        stalled(&addon).unwrap().id,
        "boot_stall",
        "the known addon comes first"
    );
}

#[test]
fn a_boot_stuck_on_loading_names_the_addon_known_to_cause_it() {
    let d = stalled(&stuck(
        SPLASH,
        408,
        &[LIVERY, ("giggles.helmet-exchange", "Helmet Exchange")],
    ))
    .unwrap();
    assert_eq!(d.id, "boot_stall");
    assert_eq!(d.title, "BP Ultimate Livery kept F1 25 on the loading screen");
    assert!(d.what.contains("for 5 minutes"), "{}", d.what);
    assert_eq!(
        d.steps,
        ["Turn off BP Ultimate Livery in your profile, then press Play again."]
    );
    assert_eq!(
        stalled(&stuck(
            SPLASH,
            408,
            &[("giggles.helmet-exchange", "Helmet Exchange")]
        )),
        None
    );
    assert_eq!(
        stalled(&stuck(SPLASH, 120, &[LIVERY])),
        None,
        "30 s is a normal load"
    );
    let mut off = stuck(SPLASH, 408, &[LIVERY]);
    off.enabled = vec!["giggles.helmet-exchange".into()];
    assert_eq!(stalled(&off), None, "installed but off");
    let menu = [
        SPLASH[2],
        "00:05:21        653       31      0         50      11.96  screen fe_home (current)",
    ];
    assert_eq!(stalled(&stuck(&menu, 408, &[LIVERY])), None);
}

#[test]
fn the_hang_watchdog_on_the_loading_screen_names_the_addon_known_to_stall_it() {
    let report = "== Game state (from the black box)\n  last flow      #04fc8ffa Entering (2584.6 s before this report)\n  \
                      last screen    splash_transition (2581.5 s before this report)\n";
    let mut e = with_names(ev(WATCHDOG, &["F1_25.exe"]), &[LIVERY]);
    e.report = Some(report.to_owned());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "hang_watchdog");
    assert!(
        d.what
            .contains("never got past Loading... after Press any button, which BP Ultimate Livery"),
        "{}",
        d.what
    );
    assert_eq!(
        d.steps[0],
        "Turn off BP Ultimate Livery in your profile, then play again."
    );
    e.report = Some(report.replace("splash_transition", "fe_home"));
    e.perf = Some(perf(SPLASH, 408));
    assert!(!diagnose(&e).unwrap().what.contains("BP Ultimate Livery"));
}

const GAME_HEAP: &str = "\
2026-10-27 14:21:24.880 [17796] [crash] Ne heap error 0xc (fatal) on thread 17796: Allocation error. Requested size:3528, Free Memory:829408, Largest Free Block:1392, HeapSize:204472320, Alignment: 8; heap 00000000636F4590 'GAME_DEFAULT', capacity 204472320
2026-10-27 14:21:24.880 [16100] [crash] writing a crash report: fatal Ne heap error 0xc in heap 'GAME_DEFAULT': Allocation error. Requested size:3528
2026-10-27 14:21:25.000 [17796] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_full_game_heap_while_saving_says_so() {
    let mut e = ev(GAME_HEAP, &[]);
    assert!(!diagnose(&e).unwrap().what.contains("saving"));
    e.report = Some(
        "== Stack\n  #9  00000001439340f9 F1_25.exe+0x39340f9 .xtls Game_SaveData_WriteBag+0x39\n".to_owned(),
    );
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "game_heap");
    assert!(
        d.what.contains("while the game was saving your career"),
        "{}",
        d.what
    );
}

const REPLAY_STATE: &str = "\
2026-11-02 05:10:01.100 [4000] [replay] session has 24 cars, the recorder holds 22; CannedEventsSystem needs 1032 bytes of replay state and its holder has 1024, so it is not recorded (replay and flashback lose its state; the session keeps all its cars)
2026-11-02 05:21:24.880 [17796] [crash] Ne heap error 0xc (fatal) on thread 17796: Allocation error. Requested size:3747052065449325568, Free Memory:829408, Largest Free Block:1392, HeapSize:204472320, Alignment: 8; heap 00000000636F4590 'GAME_DEFAULT', capacity 204472320
";

#[test]
fn an_absurd_heap_request_after_a_dropped_replay_state_is_the_replay_crash() {
    let d = diagnose(&ev(REPLAY_STATE, &[])).unwrap();
    assert_eq!(d.id, "replay_state");
    assert!(d.steps[0].contains("Flashbacks off"), "{:?}", d.steps);
    assert!(d.what.contains("not a memory problem"), "{}", d.what);
    let without = REPLAY_STATE.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(diagnose(&ev(&without, &[])).unwrap().id, "game_heap");
    let small = REPLAY_STATE.replace("3747052065449325568", "3528");
    assert_eq!(diagnose(&ev(&small, &[])).unwrap().id, "game_heap");
}

const QUIT_REPORT: &str = "== Game state (from the black box)\n  last flow      #e11edb87 Exiting (1.8 s before this report)\n  \
        last exception first-chance ACCESS_VIOLATION at F1_25.exe+0x537db28 .xtls fn base+0x537db18+0x10 (0.0 s before this report)\n";

#[test]
fn an_access_violation_in_the_exiting_flow_is_a_quit_fault() {
    let mut e = ev("", &[]);
    e.report = Some(QUIT_REPORT.to_owned());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "quit_fault");
    assert!(!d.what.contains("crashed"), "{}", d.what);
    e.report = Some(QUIT_REPORT.replace("Exiting (1.8", "Exiting (400.0"));
    assert_eq!(diagnose(&e), None, "an exit long before the fault");
    e.report = Some(QUIT_REPORT.replace("Exiting", "Entering"));
    assert_eq!(diagnose(&e), None, "not exiting");
    e.report = Some(QUIT_REPORT.replace("F1_25.exe+", "other.dll+"));
    assert_eq!(diagnose(&e), None, "not in the game's code");
    e.report = Some(QUIT_REPORT.replace("#e11edb87", "StateApp"));
    assert_eq!(diagnose(&e).unwrap().id, "quit_fault", "the root state by name");
}

const SESSION_EXIT_REPORT: &str = "== Game state (from the black box)\n  \
        last flow      StateSession Exiting (1.2 s before this report)\n  \
        last screen    garage_monitor_container (0.1 s before this report)\n  \
        last session   started (Game_GameModule_OnSessionEnter); frame 19364 (1.9 s before this report)\n  \
        last race      none recorded\n  \
        last career    hub left (117.5 s before this report)\n  \
        last heap      none recorded\n  \
        last exception first-chance ACCESS_VIOLATION at F1_25.exe+0x1f8a25c .xtls \
        Game_Read_OverridesCarMonitorMaximisedConfig_2+0xe6c (0.0 s before this report)\n";

const SESSION_EXIT_LOADER: &str = "\
2026-11-14 19:35:37.360 [ 9720] [career] flow \"StateSession\" Updating -> Exiting
2026-11-14 19:35:37.362 [ 9720] [events] session.end (StateSession exited)
2026-11-14 19:35:37.769 [ 5108] [events] career.saved (driver_career): 0 handlers
2026-11-14 19:35:38.524 [17320] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 17320, 419 s after load, at F1_25.exe+0x1f8a25c .xtls Game_Read_OverridesCarMonitorMaximisedConfig_2+0xe6c
2026-11-14 19:35:38.524 [17320] [crash]   reading 0x9c58
2026-11-14 19:35:38.525 [17320] [crash]   stack:
2026-11-14 19:35:38.525 [17320] [crash]   #0  F1_25.exe+0x1f8a25c .xtls Game_Read_OverridesCarMonitorMaximisedConfig_2+0xe6c
2026-11-14 19:35:38.525 [17320] [crash]   #1  F1_25.exe+0x42c02eb .xtls Vm_Dispatch_BindingMessage+0x20b
2026-11-14 19:35:38.525 [17320] [crash]   #2  F1_25.exe+0x398c32c .xtls Ne_EventQueue_DispatchPending+0xbc
2026-11-14 19:35:38.526 [17320] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x1f8a25c .xtls Game_Read_OverridesCarMonitorMaximisedConfig_2+0xe6c reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_session_that_exits_while_the_game_runs_is_not_a_quit_fault() {
    let mut e = ev(SESSION_EXIT_LOADER, &[]);
    e.report = Some(SESSION_EXIT_REPORT.to_owned());
    assert_eq!(diagnose(&e), None, "the session was exiting, not the game");
    for flow in ["StateUnloadSession", "SessionStateCareerHubUpdate", "#04fc8ffa"] {
        e.report = Some(SESSION_EXIT_REPORT.replace("StateSession", flow));
        assert_eq!(diagnose(&e), None, "{flow}");
    }
    e.report = Some(SESSION_EXIT_REPORT.replace("StateSession", "#e11edb87"));
    assert_eq!(diagnose(&e).unwrap().id, "quit_fault");
}

#[test]
fn a_full_game_heap_names_sepang_first() {
    let mut e = with_names(
        ev(GAME_HEAP, &[]),
        &[
            ("mod.hockenheim", "Hockenheim"),
            ("mod.portimao", "Portimao"),
            ("mod.bahrain24", "Sepang"),
            ("swofty.classic-seasons", "Classic Seasons"),
            ("swofty.classic-seasons-2016", "Classic Seasons 2016"),
            ("swofty.classic-seasons-2017", "Classic Seasons 2017"),
        ],
    );
    e.tracks = vec!["mod.hockenheim".into(), "mod.bahrain24".into()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "game_heap");
    assert!(
        d.what
            .contains("your profile has Sepang, Hockenheim and 2 Classic Seasons years on"),
        "{}",
        d.what
    );
    assert!(
        d.steps[0].starts_with("Turn off Sepang in your profile"),
        "{}",
        d.steps[0]
    );
    assert!(
            d.steps[0].ends_with(
                "In a My Team career whose next round is at Sepang, change that round with the Career Mode Editor first: \
                 the career won't load while that track is off."
            ),
            "{}",
            d.steps[0]
        );
    e.tracks = vec!["mod.hockenheim".into(), "mod.portimao".into()];
    let d = diagnose(&e).unwrap();
    assert!(
        d.steps[0].contains("Turn off Hockenheim and Portimao in your profile")
            && d.steps[0].contains("next round is at one of them,"),
        "{}",
        d.steps[0]
    );
    let d = diagnose(&ev(GAME_HEAP, &[])).unwrap();
    assert_eq!(d.id, "game_heap");
    assert_eq!(d.steps.len(), 1);
}

#[test]
fn a_full_game_heap_in_progressive_keeps_every_year_and_never_moves_a_round() {
    let mut e = with_names(
        ev(GAME_HEAP, &[]),
        &[
            ("mod.bahrain24", "Sepang"),
            ("swofty.progressive", "Progressive Career"),
            ("swofty.classic-seasons-2016", "Classic Seasons 2016"),
        ],
    );
    e.tracks = vec!["mod.bahrain24".into()];
    let d = diagnose(&e).unwrap();
    let all = d.steps.join(" ");
    assert!(
        !all.contains("Career Mode Editor") && !all.contains("you race"),
        "{all}"
    );
    assert!(all.contains("Keep every Classic Seasons year on"), "{all}");
    assert!(d.steps[0].starts_with("Turn off Sepang in your profile"), "{all}");
}

const COMMIT: &str = "\
2026-10-27 13:35:41.000 [15812] [crash] Ne heap error 0x58 (fatal) on thread 15812: FATAL ERROR : allocateAligned / useAddressSpace(block = 0xcdfe3a80, allocSize = 2088976) failed.Out of physical memory or Virtual memory problem; heap 0000000064243050 'RENDERER_INTERNAL'
2026-10-27 13:35:41.100 [15812] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
";

#[test]
fn windows_running_out_of_memory_is_not_the_graphics_card() {
    let mut e = with_names(
        ev(COMMIT, &[]),
        &[("swofty.classic-seasons-2019", "Classic Seasons 2019")],
    );
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "windows_out_of_memory");
    assert!(d.what.contains("not the graphics card's"), "{}", d.what);
    assert!(d.steps[0].contains("page file"), "{}", d.steps[0]);
    assert!(d.steps[2].contains("1 Classic Seasons year on"), "{}", d.steps[2]);
    let warned = COMMIT
        .replace("0x58 (fatal)", "0x58 (not fatal)")
        .replace("Ne_FatalAbort", "Other");
    e.loader = warned;
    assert_eq!(diagnose(&e), None);
    e.loader =
        format!("{COMMIT}2026-10-21 07:05:50.000 [1] [offline] module: winex11.drv @ 0x6ffffde80000\n");
    assert_eq!(diagnose(&e), None);
}

#[test]
fn exit_code_0xc0e90002_is_app_control() {
    let mut e = ev("", &[]);
    e.code = Some(APP_CONTROL);
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "app_control");
    assert!(
        d.steps[0].contains("Smart App Control settings, and choose Off"),
        "{}",
        d.steps[0]
    );
}

const RTSS: &str = "\
2026-10-27 17:37:40.100 [1] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 1, 3 s after load, at ntdll.dll+0x4a612
2026-10-27 17:37:40.100 [1] [crash]   reading 0xffffffffffffffff
2026-10-27 17:37:40.100 [1] [crash]   stack:
2026-10-27 17:37:40.100 [1] [crash]   #0  ntdll.dll+0x4a612
2026-10-27 17:37:40.100 [1] [crash]   #1  ntdll.dll+0x1d3b2
2026-10-27 17:37:40.100 [1] [crash]   #2  RTSSHooks64.dll+0x2a1b0
2026-10-27 17:37:40.100 [1] [crash]   #3  RTSSHooks64.dll+0x2a0f0
";

const RTSS_STRAY_JUMP: &str = "\
[offline] module: RTSSHooks64.dll @ 0x180000000
[offline] module: dxgi.dll @ 0x7fffa3b20000
2026-11-15 07:12:24.289 [ 1212] [d3d12] swapchain 000000006B9A5530 created through CreateSwapChainForHwnd on queue 00000000812DD3C0 (type 0, 1600x1024, format 28, 3 buffer(s)); created by sl.interposer.dll <- F1_25.exe <- KERNEL32.DLL <- ntdll.dll
2026-11-15 07:12:26.735 [ 1212] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 1212, 20 s after load, at 0x7c3615b0 (no module)
2026-11-15 07:12:26.735 [ 1212] [crash]   executing (DEP) 0x7c3615b0
2026-11-15 07:12:26.735 [ 1212] [crash]   rax=0000000080070057 rbx=00000000865546b0 rcx=0000000000000000 rdx=0000000000010000
2026-11-15 07:12:26.735 [ 1212] [crash]   rsi=000000018024056d rdi=00007fff2a76b566 rbp=0000000180073c51 rsp=000000000015c590
2026-11-15 07:12:26.735 [ 1212] [crash]   r8 =7ffffffffffffffc r9 =0000000000000000 r10=00007fffa78fb000 r11=000000000015c320
2026-11-15 07:12:26.735 [ 1212] [crash]   r12=0000000000000003 r13=000000006b9a5530 r14=0000000000000400 r15=0000000000000640
2026-11-15 07:12:26.735 [ 1212] [crash]   code: FF 00 00 00 FF 00 00 00 B4 13 6E 46 FF 17 00 88
2026-11-15 07:12:26.735 [ 1212] [crash]   stack:
2026-11-15 07:12:26.735 [ 1212] [crash]   #0  0x7c3615b0 (no module)
2026-11-15 07:12:26.735 [ 1212] [crash]   #1  0x64b82cb0 (no module)
";

#[test]
fn a_jump_to_no_module_with_registers_in_a_hook_names_its_program() {
    let mut e = ev(RTSS_STRAY_JUMP, &[]);
    e.code = Some(0xc000_0005);
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "third_party_hook");
    assert!(d.title.contains("RivaTuner"), "{}", d.title);
    assert!(d.what.contains("rtsshooks64.dll"), "{}", d.what);
    assert!(d.steps[0].contains("MSI Afterburner"), "{}", d.steps[0]);
    let reading = RTSS_STRAY_JUMP.replace("executing (DEP)", "reading");
    e.loader = reading;
    assert_eq!(
        diagnose(&e),
        None,
        "only a jump to an address that is not code counts"
    );
    e.loader = RTSS_STRAY_JUMP.replace("RTSSHooks64.dll @ 0x180000000", "RTSSHooks64.dll @ 0x809f0000");
    assert_eq!(diagnose(&e), None, "no register points into the hook's module");
    e.loader = RTSS_STRAY_JUMP.replace("module: RTSSHooks64.dll", "module: sl.interposer.dll");
    assert_eq!(
        diagnose(&e),
        None,
        "the registers point into a module that is no hook"
    );
    e.loader = RTSS_STRAY_JUMP.replace("[offline] module: RTSSHooks64.dll @ 0x180000000\n", "");
    assert_eq!(diagnose(&e), None, "the hook is not among the loaded modules");
}

#[test]
fn module_bases_and_record_registers_are_read() {
    assert_eq!(
        module_bases(RTSS_STRAY_JUMP),
        [
            ("rtsshooks64.dll".to_owned(), 0x1_8000_0000),
            ("dxgi.dll".to_owned(), 0x7fff_a3b2_0000)
        ]
    );
    let f = fatal(&Evidence {
        code: Some(0xc000_0005),
        ..ev(RTSS_STRAY_JUMP, &[])
    });
    assert_eq!(f.module, None);
    let regs = record_registers(&f.record);
    assert_eq!(regs.len(), 16, "{regs:x?}");
    assert!(
        regs.contains(&0x1_8024_056d) && regs.contains(&0x7fff_ffff_ffff_fffc),
        "{regs:x?}"
    );
}

#[test]
fn a_first_chance_crash_without_an_unhandled_line_still_counts() {
    let mut e = ev(RTSS, &[]);
    e.code = Some(0xc000_0005);
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "third_party_hook");
    assert!(d.title.contains("RivaTuner"), "{}", d.title);
    e.code = Some(0xc000_0409);
    assert_eq!(diagnose(&e), None);
    e.code = None;
    assert_eq!(diagnose(&e), None);
}

fn car_pack(winner: &str, loser: &str, resources: usize) -> Pair {
    Pair {
        winner: winner.into(),
        loser: loser.into(),
        resources,
        items: vec![
            "2025_asset_groups/f1_2025_vehicle_package/teams/ferrari/wep/ferrari.erp:eaid://ferrari.idf"
                .into(),
        ],
        ..Pair::default()
    }
}

#[test]
fn a_removed_gpu_names_car_packs_and_frame_generation() {
    let mut e = with_names(
        ev(GPU, &["F1_25.exe"]),
        &[
            ("mod.f1-2016", "F1 2016 Cars"),
            ("mod.f1-2018", "F1 2018 Cars"),
            ("mod.livery", "Livery"),
        ],
    );
    e.conflicts = vec![
        car_pack("mod.f1-2018", "mod.f1-2016", 8902),
        car_pack("mod.livery", "mod.f1-2018", 12),
    ];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "gpu_removed");
    assert!(!d.what.contains("not from Swofty's code"), "{}", d.what);
    assert!(
        d.what
            .contains("F1 2016 Cars and F1 2018 Cars are all full car packs"),
        "{}",
        d.what
    );
    assert_eq!(
        d.steps[0],
        "Keep only one of F1 2016 Cars and F1 2018 Cars on in your profile."
    );
    e.conflicts.clear();
    e.loader = format!(
        "2026-10-24 02:50:00.000 [1] [overlay] the game's hardware settings (x.xml) have frame generation on (frame_gen mode 1, multi_frame_gen 1)\n{GPU}"
    );
    let d = diagnose(&e).unwrap();
    assert!(d.what.contains("Frame generation was on"), "{}", d.what);
    assert_eq!(d.steps[0], FRAME_GEN_OFF);
    assert_eq!(
        d.steps.iter().filter(|s| s.contains("frame generation")).count(),
        1
    );
}

#[test]
fn the_games_low_disk_dialog_names_the_drive_and_the_space() {
    let log = "2026-10-27 22:08:24.555 [ 3820] dialog shown: Insufficient Storage: 652/2048 MiB available on \
                   'C:\\', please free some space.\n22:08:32.240 [stub] [offline] game exited with code 4294967295\n";
    let d = diagnose(&ev(log, &[])).unwrap();
    assert_eq!(d.id, "low_disk");
    assert_eq!(d.title, "F1 25 needs more free space on drive C");
    assert_eq!(
        d.what,
        "F1 25 needs 2 GB free on drive C and found 0.6 GB, so it showed Insufficient Storage and closed. It \
             checks drive C even when the game is installed on another drive."
    );
    assert!(
        d.steps[0].starts_with("Free up at least 1.4 GB on drive C: empty"),
        "{}",
        d.steps[0]
    );
    let other = log.replace("652/2048", "2000/2048").replace("'C:", "'d:");
    let d = diagnose(&ev(&other, &[])).unwrap();
    assert_eq!(
        d.what,
        "F1 25 needs 2 GB free on drive D and found 1.9 GB, so it showed Insufficient Storage and closed."
    );
    assert!(
        d.steps[0].starts_with("Free up at least 0.1 GB on drive D: empty"),
        "{}",
        d.steps[0]
    );
    let path = log.replace("'C:\\'", "'\\\\server\\share'");
    assert!(
        diagnose(&ev(&path, &[]))
            .unwrap()
            .what
            .contains("on the drive F1 25 saves to")
    );
    assert_eq!(diagnose(&ev(&log.replace("652/2048", "lots"), &[])), None);
}

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

/// The game folder's file names among the crash report's loaded modules.
fn game_folder_modules(report: &str) -> Vec<String> {
    let exe = report
        .lines()
        .find_map(|l| l.strip_prefix("exe")?.trim().split(", ").next())
        .unwrap_or_default();
    let Some((folder, _)) = exe.rsplit_once('\\') else {
        return Vec::new();
    };
    report
        .lines()
        .filter_map(|l| l.trim().split_once("  ").and(l.find(folder)).map(|i| &l[i..]))
        .filter_map(|p| p.strip_prefix(folder)?.strip_prefix('\\'))
        .filter(|rest| !rest.contains('\\'))
        .map(str::to_owned)
        .collect()
}

const LIST_SEARCH: &str = "\
2026-10-28 01:49:51.119 [15588] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 15588, 237 s after load, at F1_25.exe+0x1738f30 .xtls fn base+0x1738eb0+0x80
2026-10-28 01:49:51.119 [15588] [crash]   rsi=000000006b18b9ca rdi=0000000021500d09 rbp=0000000000000000 rsp=000000006b18b940
2026-10-28 01:49:51.119 [15588] [crash]   #0  F1_25.exe+0x1738f30 .xtls fn base+0x1738eb0+0x80
2026-10-28 01:49:51.119 [15588] [crash]   #1  F1_25.exe+0x17384ec .xtls fn base+0x17382b0+0x23c
2026-10-28 01:49:51.120 [15588] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x1738f30 .xtls fn base+0x1738eb0+0x80 reached the top-level filter; passing it to the game's filter
";

const LIST_REPORT: &str = "\
== Exception
  ACCESS_VIOLATION (0xc0000005), flags 0x0, at F1_25.exe+0x1738f30 .xtls fn base+0x1738eb0+0x80

== Memory at the registers
  rcx=000000000715a9d1: committed private region 0x715a000 +0x238a6000 (allocation 0x6c90000), read/write
    000000000715a9d0> 66 31 5f 32 30 32 36 5f 64 72 69 76 65 72 5f 70  f1_2026_driver_p
  rdi=0000000021500d09: committed private region 0x21500000 +0x9500000 (allocation 0x6c90000), read/write
    0000000021500cf0  64 00 95 9e 01 00 00 00 f0 40 6a 21 00 00 00 00  d........@j!....
    0000000021500d00> 66 17 e9 2a 01 00 07 00 c0 66 32 5f 32 30 32 36  f..*.....f2_2026
    0000000021500d10  00 6e 5f 70 61 63 6b 61 67 65 00 17 00 00 00 00  .n_package......
  r14=cb42b7f752c18f53: not a valid address
";

#[test]
fn a_missing_entrant_list_names_the_list_and_the_addon_the_build_saw() {
    let mut e = ev(LIST_SEARCH, &["F1_25.exe"]);
    e.report = Some(LIST_REPORT.to_owned());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "missing_entrant_list");
    assert_eq!(d.title, "F1 25 crashed looking for the entrant list f2_2026");
    assert!(
        d.steps[0].starts_with("Turn off the database addons"),
        "{:?}",
        d.steps
    );

    e = with_names(e, &[("mod.old-team-pack", "Old Team Pack")]);
    e.kept_lists = BTreeMap::from([("mod.old-team-pack".to_owned(), vec!["f2_2026".to_owned()])]);
    let d = diagnose(&e).unwrap();
    assert!(
        d.what
            .ends_with("The last build found Old Team Pack renaming or removing it."),
        "{}",
        d.what
    );
    assert_eq!(
        d.steps[0],
        "Turn off Old Team Pack, or ask its author for a version made for this F1 25 update."
    );

    e.report = None;
    let d = diagnose(&e).unwrap();
    assert_eq!(d.title, "F1 25 crashed looking for an entrant list");
    let other = LIST_SEARCH.replace("+0x1738f30", "+0x1738f31");
    assert_ne!(
        diagnose(&ev(&other, &["F1_25.exe"])).map(|d| d.id),
        Some("missing_entrant_list")
    );
}

const CAREER_TRACK: &str = "\
2026-10-28 20:58:46.394 [4936] [career] career.loaded: mode driver_career, key 'dc-00000003' (components live, component mode driver_career, Team HQ answer none)
2026-10-28 20:58:47.933 [5060] [career] flow \"StateLoadSession\" None -> Entering
2026-10-28 20:58:47.954 [7472] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 7472, 169 s after load, at F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b
2026-10-28 20:58:47.954 [7472] [crash]   reading 0x78
2026-10-28 20:58:47.954 [7472] [crash]   stack:
2026-10-28 20:58:47.954 [7472] [crash]   #0  F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b
2026-10-28 20:58:47.954 [7472] [crash]   #1  F1_25.exe+0x7a13ce .xtls fn base+0x7a1310+0xbe
2026-10-28 20:58:47.954 [7472] [crash]   #2  F1_25.exe+0x8706316 .data1 fn base+0x87062f0+0x26
2026-10-28 20:58:47.954 [7472] [crash]   #3  F1_25.exe+0x2d48bb7 .xtls Sess_Loader_SetupTrackAssets+0x47
2026-10-28 20:58:47.954 [7472] [crash]   #4  F1_25.exe+0x2d471af .xtls Sess_Loader_RequestSessionLoad+0xef
2026-10-28 20:58:47.955 [7472] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b reached the top-level filter; passing it to the game's filter
";

fn career_off(loader: &str, off: &[&str]) -> Evidence {
    let mut e = with_names(
        ev(loader, &[]),
        &[
            ("mod.bahrain24", "Sepang"),
            ("swofty.classic-seasons-2018", "F1 2018 GP/TT Addon"),
        ],
    );
    e.off = off.iter().map(|id| (*id).to_owned()).collect();
    e
}

#[test]
fn a_career_crash_setting_up_the_track_names_the_addon_that_is_off() {
    let d = diagnose(&career_off(CAREER_TRACK, &["mod.bahrain24"])).unwrap();
    assert_eq!(d.id, "career_content_off");
    assert_eq!(d.title, "Your career uses Sepang, which is off");
    assert!(
        d.what
            .ends_with("A race in your career uses Sepang, which is off in this profile."),
        "{}",
        d.what
    );
    assert_eq!(
        d.steps[0],
        "Turn Sepang back on. In a My Team career you can also change that round with the Career Mode Editor."
    );

    let d = diagnose(&career_off(
        CAREER_TRACK,
        &["mod.bahrain24", "swofty.classic-seasons-2018"],
    ))
    .unwrap();
    assert_eq!(d.id, "career_content_off");
    assert_eq!(d.title, "Your career uses a track or season that is off");
    assert!(
        d.what.ends_with("which has Sepang and F1 2018 GP/TT Addon off."),
        "{}",
        d.what
    );
    assert!(!d.what.contains("uses Sepang"), "{}", d.what);
    assert_eq!(
        d.steps[0],
        "Turn Sepang and F1 2018 GP/TT Addon back on. In a My Team career you can also change that round with the Career Mode Editor."
    );
}

#[test]
fn a_career_track_crash_needs_an_addon_off_a_fresh_career_load_and_the_track_setup() {
    assert_eq!(diagnose(&career_off(CAREER_TRACK, &[])), None, "nothing off");
    let late = CAREER_TRACK.replace("20:58:46.394", "20:58:30.000");
    assert_eq!(
        diagnose(&career_off(&late, &["mod.bahrain24"])),
        None,
        "long after the career loaded"
    );
    let unloaded = CAREER_TRACK.replace(
            "2026-10-28 20:58:47.933",
            "2026-10-28 20:58:47.000 [4936] [career] career.unloaded (mode driver_career)\n2026-10-28 20:58:47.933",
        );
    assert_eq!(
        diagnose(&career_off(&unloaded, &["mod.bahrain24"])),
        None,
        "the career unloaded"
    );
    let grid = CAREER_TRACK.replace(
        ".xtls fn base+0x7a1310+0xbe",
        ".xtls Game_GridSetup_DriverBasedGrid+0x847",
    );
    assert_eq!(
        diagnose(&career_off(&grid, &["mod.bahrain24"])),
        None,
        "a nearer named frame"
    );
    let no_career = CAREER_TRACK.replace("career.loaded: ", "career.starting: ");
    assert_eq!(
        diagnose(&career_off(&no_career, &["mod.bahrain24"])),
        None,
        "no career"
    );
}

const SEQUENCER: &str = "\
2026-11-03 09:03:51.484 [12700] [era] session trace Sess_Setup_VehicleComponents (driver_career_progressive): session vehicles 22
2026-11-03 09:03:51.486 [12700] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 12700, 119 s after load, at F1_25.exe+0xc72ea0 .xtls Game_CareerEventsSequencer_Initialise+0x1020
2026-11-03 09:03:51.486 [12700] [crash]   reading 0x0
2026-11-03 09:03:51.486 [12700] [crash]   stack:
2026-11-03 09:03:51.486 [12700] [crash]   #0  F1_25.exe+0xc72ea0 .xtls Game_CareerEventsSequencer_Initialise+0x1020
2026-11-03 09:03:51.486 [12700] [crash]   #1  F1_25.exe+0x7bef38 .xtls fn base+0x7bec10+0x328
2026-11-03 09:03:51.487 [12700] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0xc72ea0 .xtls Game_CareerEventsSequencer_Initialise+0x1020 reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_crash_reading_the_first_calendar_event_record_names_the_calendar_addon() {
    let mut e = with_names(
        ev(SEQUENCER, &[]),
        &[
            ("dbedit.new-calender", "New calender"),
            ("swofty.progressive", "Progressive Career"),
        ],
    );
    e.enabled = vec!["swofty.progressive".to_owned(), "dbedit.new-calender".to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "calendar_event_missing");
    assert_eq!(d.title, "New calender changed your career's calendar");
    assert!(d.steps[0].starts_with("Turn off New calender"), "{}", d.steps[0]);

    e.enabled = vec!["swofty.progressive".to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "calendar_event_missing");
    assert_eq!(d.title, "Your career's calendar has a round F1 25 can't find");

    let other = SEQUENCER.replace("Initialise+0x1020", "Initialise+0x44");
    assert_eq!(
        diagnose(&ev(&other, &[])).map(|d| d.id),
        None,
        "another offset of the function"
    );
}

const MORPH: &str = "\
2026-11-12 19:34:23.598 [ 3048] [silly-season] session start: race session, weekend round 4
2026-11-12 19:34:23.598 [ 3048] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 3048, 111 s after load, at F1_25.exe+0xc80962 .xtls Game_CareerEventResourceManager_AddDriverMorphData+0x82
2026-11-12 19:34:23.598 [ 3048] [crash]   reading 0x8
2026-11-12 19:34:23.599 [ 3048] [crash]   stack:
2026-11-12 19:34:23.599 [ 3048] [crash]   #0  F1_25.exe+0xc80962 .xtls Game_CareerEventResourceManager_AddDriverMorphData+0x82
2026-11-12 19:34:23.599 [ 3048] [crash]   #1  F1_25.exe+0xc72ebf .xtls Game_CareerEventsSequencer_Initialise+0x103f
2026-11-12 19:34:23.599 [ 3048] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0xc80962 .xtls Game_CareerEventResourceManager_AddDriverMorphData+0x82 reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_crash_reading_a_missing_team_driver_names_no_addon() {
    let mut e = with_names(
        ev(MORPH, &[]),
        &[
            ("alexs.silly-season", "Silly Season"),
            ("custom.team-names", "Team & Driver Editor"),
            ("mildtdesign.more-driver-models", "More Driver Models"),
        ],
    );
    e.enabled = vec![
        "mildtdesign.more-driver-models".to_owned(),
        "alexs.silly-season".to_owned(),
        "custom.team-names".to_owned(),
    ];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "career_driver_missing");
    assert_eq!(d.title, "Your career has a driver F1 25 can't find");
    for addon in ["Silly Season", "Team & Driver Editor", "More Driver Models"] {
        assert!(!d.text().contains(addon), "{addon} is named: {}", d.text());
    }

    e.enabled = vec![];
    assert_eq!(diagnose(&e).unwrap(), d, "the same diagnosis with no addon on");

    let other = MORPH.replace("AddDriverMorphData+0x82", "AddDriverMorphData+0x44");
    assert_eq!(
        diagnose(&ev(&other, &[])).map(|d| d.id),
        None,
        "another offset of the function"
    );
}

const RETIREMENT: &str = "\
2026-11-12 20:54:48.502 [29072] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 29072, 612 s after load, at F1_25.exe+0xcb8a5e1 .data1 fn base+0xcb8a5d0+0x11
2026-11-12 20:54:48.502 [29072] [crash]   reading 0x1f0
2026-11-12 20:54:48.502 [29072] [crash]   rax=0000000000000000 rbx=0000000000000018 rcx=0000000000000018 rdx=0000000067dce2c0
2026-11-12 20:54:48.502 [29072] [crash]   stack:
2026-11-12 20:54:48.502 [29072] [crash]   #0  F1_25.exe+0xcb8a5e1 .data1 fn base+0xcb8a5d0+0x11
2026-11-12 20:54:48.503 [29072] [crash]   #1  F1_25.exe+0x2d624ee .xtls Sess_InGameRace_Update+0x77e
2026-11-12 20:54:48.503 [29072] [crash]   #2  F1_25.exe+0x2d56e08 .xtls Sess_InGame_UpdateBase+0x768
2026-11-12 20:54:48.511 [29072] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0xcb8a5e1 .data1 fn base+0xcb8a5d0+0x11 reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_crash_naming_a_retired_car_without_a_driver_names_no_addon() {
    let mut e = with_names(
        ev(RETIREMENT, &[]),
        &[
            ("kizi.canned", "Canned DNFs"),
            ("progressive.career", "Progressive Career"),
            ("custom.team-names", "Team & Driver Editor"),
        ],
    );
    e.enabled = vec![
        "kizi.canned".to_owned(),
        "progressive.career".to_owned(),
        "custom.team-names".to_owned(),
    ];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "race_driver_missing");
    assert_eq!(d.title, "A car in your race has a driver F1 25 can't find");
    for addon in ["Canned DNFs", "Progressive Career", "Team & Driver Editor"] {
        assert!(!d.text().contains(addon), "{addon} is named: {}", d.text());
    }

    e.enabled = vec![];
    assert_eq!(diagnose(&e).unwrap(), d, "the same diagnosis with no addon on");

    let other = RETIREMENT.replace("Sess_InGameRace_Update+0x77e", "Sess_InGameRace_Update+0x6a1");
    assert_eq!(
        diagnose(&ev(&other, &[])).map(|d| d.id),
        None,
        "another call in the race update"
    );
    let read = RETIREMENT.replace("reading 0x1f0", "reading 0x8");
    assert_eq!(
        diagnose(&ev(&read, &[])).map(|d| d.id),
        None,
        "another read under the same call"
    );
}

const WEEKENDS: &str = "\
2026-11-15 16:15:41.332 [ 9736] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 9736, 5051 s after load, at F1_25.exe+0x181b87a .xtls Career_Weekends_BuildResultNodes+0x4da
2026-11-15 16:15:41.332 [ 9736] [crash]   reading 0x1110000008
2026-11-15 16:15:41.332 [ 9736] [crash]   rax=0000001110000000 rbx=0000000067cff500 rcx=0000001110000008 rdx=0000000000000005
2026-11-15 16:15:41.332 [ 9736] [crash]   stack:
2026-11-15 16:15:41.332 [ 9736] [crash]   #0  F1_25.exe+0x181b87a .xtls Career_Weekends_BuildResultNodes+0x4da
2026-11-15 16:15:41.332 [ 9736] [crash]   #1  F1_25.exe+0x181b37b .xtls fn base+0x181b280+0xfb
2026-11-15 16:15:41.341 [ 9736] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x181b87a .xtls Career_Weekends_BuildResultNodes+0x4da reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_crash_reading_a_round_s_track_node_names_no_addon() {
    let mut e = with_names(ev(WEEKENDS, &[]), &[("progressive.career", "Progressive Career")]);
    e.enabled = vec!["progressive.career".to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "weekends_track_record_bad_index");
    assert_eq!(d.title, "A round of your career has a track F1 25 can't find");
    assert!(
        !d.text().contains("Progressive Career"),
        "an addon is named: {}",
        d.text()
    );

    e.enabled = vec![];
    assert_eq!(diagnose(&e).unwrap(), d, "the same diagnosis with no addon on");

    let other = WEEKENDS.replace(
        "Career_Weekends_BuildResultNodes+0x4da",
        "Career_Weekends_BuildResultNodes+0x4c1",
    );
    assert_eq!(
        diagnose(&ev(&other, &[])).map(|d| d.id),
        None,
        "another offset of the function"
    );
}

const THIRD_CAR_LINE: &str = "\
2026-11-03 10:27:30.114 [3516] [garage] garage 8: team 8 car #3 (driver 41) is the third car of its garage; its crew is not spawned, the car still races
2026-11-03 10:27:56.399 [3516] [session] vehicle slot 22 (hash 5c0e1d2a) is in no AI asset group; using 3b1f0a77 for its AI record
2026-11-03 10:27:56.420 [3516] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 3516, 217 s after load, at F1_25.exe+0x686a9e .xtls Ai_LineCache_UpdateIfChanged+0x5e
2026-11-03 10:27:56.420 [3516] [crash]   reading 0x178
2026-11-03 10:27:56.420 [3516] [crash]   stack:
2026-11-03 10:27:56.420 [3516] [crash]   #0  F1_25.exe+0x686a9e .xtls Ai_LineCache_UpdateIfChanged+0x5e
2026-11-03 10:27:56.420 [3516] [crash]   #1  F1_25.exe+0x63ece8 .xtls Ai_Job_UpdateCarsParallel+0x978
2026-11-03 10:27:56.421 [3516] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x686a9e .xtls Ai_LineCache_UpdateIfChanged+0x5e reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_line_cache_crash_with_a_third_car_names_the_addon_that_adds_it() {
    let mut e = with_names(
        ev(THIRD_CAR_LINE, &[]),
        &[
            ("mildtdesign.more-driver-models", "More Driver Models"),
            ("swofty.progressive", "Progressive Career"),
        ],
    );
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "third_car_line");
    assert!(
        d.what.contains("More Driver Models giving a team a third car"),
        "{}",
        d.what
    );
    assert!(
        d.steps[0].starts_with("Turn off More Driver Models"),
        "{}",
        d.steps[0]
    );

    e.enabled = vec!["swofty.progressive".to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "third_car_line");
    assert!(d.what.contains("a team with three cars"), "{}", d.what);

    let no_third = THIRD_CAR_LINE.replace("is the third car of its garage", "is a car of its garage");
    assert_ne!(
        diagnose(&ev(&no_third, &[])).map(|d| d.id),
        Some("third_car_line")
    );
    let other = THIRD_CAR_LINE.replace("Ai_LineCache_UpdateIfChanged+0x5e", "Ai_LineCache_Other+0x5e");
    assert_ne!(diagnose(&ev(&other, &[])).map(|d| d.id), Some("third_car_line"));
}

#[test]
fn a_track_stat_id_with_no_row_is_a_missing_track_even_with_nothing_off() {
    let line =
        "2026-10-28 20:58:47.900 [5060] [moments] track stat id 48 has no tracks row or track record\n";
    let loader = CAREER_TRACK.replace(
        "2026-10-28 20:58:47.933",
        &format!("{line}2026-10-28 20:58:47.933"),
    );
    assert_eq!(missing_track_stat(&loader), Some(48));
    let d = diagnose(&career_off(&loader, &[])).unwrap();
    assert_eq!(d.id, "career_content_off");
    assert_eq!(d.title, "Your career uses a track that is not in this profile");
    assert!(d.what.contains("track stat id 48"), "{}", d.what);
    assert_eq!(missing_track_stat(CAREER_TRACK), None);
}

#[test]
fn a_career_track_crash_reads_the_crash_reports_stack() {
    let loader = CAREER_TRACK
        .lines()
        .filter(|l| !l.contains("[crash]   ") && !l.contains("first-chance"))
        .map(|l| format!("{l}\n"))
        .collect::<String>();
    let mut e = career_off(&loader, &["mod.bahrain24"]);
    e.report = Some(
            "== Exception\n  ACCESS_VIOLATION (0xc0000005), flags 0x0, at F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b\n\
             == Stack (unwound with the modules' exception data)\n  \
             #0  0000000142e57efb rsp=000000006cfa7020  F1_25.exe+0x2e57efb .xtls fn base+0x2e57e90+0x6b\n  \
             #1  0000000142d48bb7 rsp=000000006cfae970  F1_25.exe+0x2d48bb7 .xtls Sess_Loader_SetupTrackAssets+0x47\n"
                .to_owned(),
        );
    assert_eq!(diagnose(&e).map(|d| d.id), Some("career_content_off"));
}

#[test]
fn the_profiles_track_and_season_addons_that_are_off_are_read() {
    let root = std::env::temp_dir().join(format!("swofty-kp-off-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let manifest = |id: &str, category: &str| {
        let m = AddonManifest::parse(&format!(
            r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"1.0.0","author":"t",
                "compat":{{"game":"f1_25"}},"category":"{category}"}}"#
        ))
        .unwrap();
        (id.to_owned(), m)
    };
    let addons = BTreeMap::from([
        manifest("t.track-on", "tracks"),
        manifest("t.track-off", "tracks"),
        manifest("t.season-off", "seasons"),
        manifest("t.cars-off", "cars"),
    ]);
    let mut p = crate::profile::Profile::new("main");
    for (id, enabled) in [
        ("t.track-on", true),
        ("t.track-off", false),
        ("t.cars-off", false),
        ("t.season-off", false),
        ("t.gone", false),
    ] {
        p.addons.push(crate::profile::ProfileAddon {
            id: id.to_owned(),
            enabled,
            unknown: BTreeMap::new(),
        });
    }
    store.save_profile(&p).unwrap();
    store.set_active_profile("main").unwrap();
    let e = Evidence::default().with_profile(&store, &addons);
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(e.off, ["t.track-off", "t.season-off"]);
}

/// `SWOFTY_KP_CASES=<folder of extracted Send Logs bundles> cargo test -p swofty-core --lib
/// diagnoses_real_bundles -- --ignored --nocapture` prints each crashed session's diagnosis,
/// and each other session a rule recognises.
#[test]
fn unmatched_crash_on_old_build_names_the_update() {
    let mut e = ev("", &[]);
    assert_eq!(diagnose(&e), None);
    e.game_version = Some("1.0.136.6820".to_owned());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "outdated_exe");
    assert!(d.what.contains("1.0.136.6820"));
    assert!(d.text().contains("Steam or the EA app"));
    e.game_version = Some(LATEST_GAME_VERSION.to_owned());
    assert_eq!(diagnose(&e), None);
}

#[test]
#[ignore]
fn diagnoses_real_bundles() {
    let Some(root) = std::env::var_os("SWOFTY_KP_CASES") else {
        return;
    };
    let dirs = |p: &Path| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        v.sort();
        v
    };
    let json = |p: &Path| -> serde_json::Value {
        read_capped(p)
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    };
    for case in dirs(Path::new(&root)) {
        let logs = case.join("logs");
        let addons = json(&logs.join("addons.json"));
        let installed = addons["installed"].as_array().cloned().unwrap_or_default();
        let names = installed
            .iter()
            .filter_map(|a| Some((a["id"].as_str()?.to_owned(), a["name"].as_str()?.to_owned())))
            .collect();
        let versions: BTreeMap<String, String> = installed
            .iter()
            .filter_map(|a| Some((a["id"].as_str()?.to_owned(), a["version"].as_str()?.to_owned())))
            .collect();
        let conflicts: crate::conflicts::Report =
            serde_json::from_value(json(&logs.join("conflicts.json"))).unwrap_or_default();
        let tracks = conflicts
            .order
            .iter()
            .filter(|id| {
                installed
                    .iter()
                    .any(|a| a["id"].as_str() == Some(id.as_str()) && a["category"] == "tracks")
            })
            .cloned()
            .collect();
        let off: Vec<String> = addons["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["active"] == true)
            .flat_map(|p| p["addons"].as_array().cloned().unwrap_or_default())
            .filter(|a| a["enabled"] == false)
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .filter(|id| {
                installed.iter().any(|a| {
                    a["id"].as_str() == Some(id.as_str())
                        && (a["category"] == "tracks" || a["category"] == "seasons")
                })
            })
            .collect();
        for session in dirs(&logs.join("sessions")) {
            let record = SessionRecord::read(&session);
            let loader = ["loader.log", "game-logs/loader.log"]
                .iter()
                .find_map(|rel| read_capped(&session.join(rel)))
                .unwrap_or_default();
            let crashed = record.crashed()
                || loader.contains("reached the top-level filter")
                || loader.contains("[crash] crash report ")
                || loader.contains(LOW_DISK);
            let report = report_name(&loader).and_then(|n| read_capped(&logs.join("crash").join(n)));
            let game_files = report.as_deref().map(game_folder_modules).unwrap_or_default();
            let mut e = Evidence::read(&record, None, BTreeMap::new(), None);
            let game_version = read_capped(&logs.join("summary.txt")).and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("F1 25 "))
                    .map(|v| v.trim().to_owned())
            });
            e = Evidence {
                game_version,
                loader,
                report,
                game_files,
                names: BTreeMap::clone(&names),
                versions: versions.clone(),
                enabled: conflicts.order.clone(),
                tracks: Vec::clone(&tracks),
                off: off.clone(),
                conflicts: conflicts.pairs.clone(),
                kept_lists: conflicts.kept_lists.clone(),
                ..e
            };
            let found = match crashed {
                true => diagnose(&e),
                false => stalled(&e),
            };
            if !crashed && found.is_none() {
                continue;
            }
            let found = found.map_or_else(
                || "-".to_owned(),
                |d| format!("{}\t{}\t{}", d.id, d.title, d.text()),
            );
            println!(
                "KP\t{}\t{}\t{found}",
                case.file_name().unwrap().to_string_lossy(),
                session.file_name().unwrap().to_string_lossy()
            );
        }
    }
}

const APPLY_FAILED: &str = "applying swofty.classic-seasons: eaid://ui_package/databases/overrides.uidb: \
                                overrides.Contents.teams.Contents.mp_team_select_logos.Contents: no entry \
                                [Name=vanamersfoort_26]";

fn classic_seasons() -> BTreeMap<String, String> {
    BTreeMap::from([("swofty.classic-seasons".to_owned(), "Classic Seasons".to_owned())])
}

#[test]
fn game_versions_compare_by_number() {
    assert_eq!(older_game("1.0.138.5601", LATEST_GAME_VERSION), Some(true));
    assert_eq!(older_game("1.0.136.6820", "1.0.141.7865"), Some(true));
    assert_eq!(older_game("1.0.141.7865", LATEST_GAME_VERSION), Some(false));
    assert_eq!(older_game("1.0.142.1", LATEST_GAME_VERSION), Some(false));
    assert_eq!(older_game("1.0.141.10000", "1.0.141.7865"), Some(false));
    assert_eq!(older_game("1.0.99.9999", "1.0.141.7865"), Some(true));
    assert_eq!(older_game(" 1.0.137.2811 ", LATEST_GAME_VERSION), Some(true));
    for unknown in [
        "",
        "1.0.138",
        "1.0.138.5601.2",
        "1.0.x.5601",
        "25251167",
        "1..138.5601",
        "1.0.+8.5601",
    ] {
        assert_eq!(older_game(unknown, LATEST_GAME_VERSION), None, "{unknown}");
    }
}

#[test]
fn an_addon_that_does_not_apply_to_an_old_game_names_the_update() {
    let d = outdated_game(APPLY_FAILED, Some("1.0.136.6820"), &classic_seasons()).unwrap();
    assert_eq!(d.id, "outdated_game");
    assert_eq!(d.title, "Classic Seasons needs the latest F1 25");
    assert_eq!(
        d.text(),
        "Classic Seasons needs F1 25 1.0.141.7865, the latest update. Your F1 25 is 1.0.136.6820. Update F1 \
             25 in Steam or the EA app, then press Play again."
    );
    let wrapped = format!("the wrapper exited (exit code: 1) before the game started: {APPLY_FAILED}");
    let d = outdated_game(&wrapped, Some("1.0.138.5601"), &BTreeMap::new()).unwrap();
    assert!(
        d.what
            .starts_with("swofty.classic-seasons needs F1 25 1.0.141.7865"),
        "{}",
        d.what
    );
    let item = APPLY_FAILED.replace("classic-seasons:", "classic-seasons (2018):");
    let d = outdated_game(&item, Some("1.0.137.2811"), &classic_seasons()).unwrap();
    assert!(d.what.starts_with("Classic Seasons needs"), "{}", d.what);
}

#[test]
fn other_games_and_other_errors_keep_their_own_message() {
    let names = classic_seasons();
    assert_eq!(
        outdated_game(APPLY_FAILED, Some(LATEST_GAME_VERSION), &names),
        None
    );
    assert_eq!(outdated_game(APPLY_FAILED, Some("1.0.142.1"), &names), None);
    assert_eq!(outdated_game(APPLY_FAILED, None, &names), None);
    assert_eq!(outdated_game(APPLY_FAILED, Some("unknown"), &names), None);
    for other in [
        "version.dll already exists in the game folder; run swofty restore",
        "building profile default: reading x: missing",
        "downloading the offline play extension: offline play 0.3.11: the download was blocked",
        "applying : no entry",
    ] {
        assert_eq!(
            outdated_game(other, Some("1.0.136.6820"), &names),
            None,
            "{other}"
        );
    }
}

#[test]
fn an_older_game_is_told_to_update_before_play() {
    let d = outdated_exe(Some("1.0.138.5601")).unwrap();
    assert_eq!(d.id, "outdated_exe");
    assert!(d.what.starts_with("Your F1 25 is 1.0.138.5601."), "{}", d.what);
    assert!(
        d.text().contains("Update F1 25 in Steam or the EA app"),
        "{}",
        d.text()
    );
    assert!(d.text().contains("steam://validate/3059520"), "{}", d.text());
    assert_eq!(outdated_exe(Some(LATEST_GAME_VERSION)), None);
    assert_eq!(outdated_exe(Some("1.0.142.1")), None);
    assert_eq!(outdated_exe(Some("unknown")), None);
    assert_eq!(outdated_exe(None), None);
}

#[test]
fn modules_the_offline_hook_lists_are_read() {
    let log = "2026-10-25 19:18:28.000 [1] [offline] module: RTSSHooks64.dll @ 0x7dd50000\n";
    assert_eq!(loaded_modules(log), ["RTSSHooks64.dll"]);
}
