use super::*;

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
