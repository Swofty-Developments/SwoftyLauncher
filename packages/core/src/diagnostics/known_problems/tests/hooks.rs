use super::*;

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
