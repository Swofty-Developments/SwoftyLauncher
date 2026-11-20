use super::*;

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

const RTSS: &str = "\
2026-10-27 17:37:40.100 [1] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 1, 3 s after load, at ntdll.dll+0x4a612
2026-10-27 17:37:40.100 [1] [crash]   reading 0xffffffffffffffff
2026-10-27 17:37:40.100 [1] [crash]   stack:
2026-10-27 17:37:40.100 [1] [crash]   #0  ntdll.dll+0x4a612
2026-10-27 17:37:40.100 [1] [crash]   #1  ntdll.dll+0x1d3b2
2026-10-27 17:37:40.100 [1] [crash]   #2  RTSSHooks64.dll+0x2a1b0
2026-10-27 17:37:40.100 [1] [crash]   #3  RTSSHooks64.dll+0x2a0f0
";

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

#[test]
fn modules_the_offline_hook_lists_are_read() {
    let log = "2026-10-25 19:18:28.000 [1] [offline] module: RTSSHooks64.dll @ 0x7dd50000\n";
    assert_eq!(loaded_modules(log), ["RTSSHooks64.dll"]);
}
