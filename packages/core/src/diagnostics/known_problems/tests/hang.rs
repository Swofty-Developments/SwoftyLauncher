use super::*;

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
