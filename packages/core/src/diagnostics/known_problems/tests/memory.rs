use super::*;

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
    assert!(
        d.steps[0].starts_with("For Grand Prix and Time Trial, turn off Sepang"),
        "{all}"
    );
    assert!(d.steps[0].contains("Keep it on for a career"), "{all}");
}

const SMALL_RAM: &str = "\
2026-11-25 10:00:00.000 [4000] [heap] heap 'GAME_DEFAULT' (virtual 1, pagesize 1): size 408944640 -> 671088640 bytes (640 MiB, default for a machine under 12 GiB)
";

#[test]
fn a_full_game_heap_in_swofty_progressive_keeps_the_season_packs() {
    let mut e = with_names(
        ev(GAME_HEAP, &[]),
        &[
            ("mod.bahrain24", "Sepang"),
            ("swofty.swofty-seasons-progressive", "Progressive Career"),
            ("swofty.swofty-seasons-2010", "SWOFTY 2010"),
        ],
    );
    e.tracks = vec!["mod.bahrain24".into()];
    let d = diagnose(&e).unwrap();
    let all = d.steps.join(" ");
    assert!(all.contains("Keep the SWOFTY season packs on"), "{all}");
    assert!(!all.contains("fewer imported tracks and season addons"), "{all}");
    assert!(!all.contains("Career Mode Editor"), "{all}");
    assert!(
        d.steps[0].starts_with("For Grand Prix and Time Trial, turn off Sepang"),
        "{all}"
    );
}

const SMALL_RAM_ARMED: &str = "\
2026-11-25 16:43:15.077 [19700] [heap] GAME_DEFAULT heap armed: 640 MiB instead of the game's 390 MiB when system/memory_map.xml is read
";

#[test]
fn a_full_game_heap_after_the_small_pc_armed_line_offers_the_heap_setting() {
    let loader = format!("{SMALL_RAM_ARMED}{GAME_HEAP}");
    let d = diagnose(&ev(&loader, &[])).unwrap();
    assert!(d.what.contains("under 12 GB of RAM"), "{}", d.what);
    assert!(
        d.steps.last().unwrap().contains("%LOCALAPPDATA%\\Swofty"),
        "{:?}",
        d.steps
    );
}

#[test]
fn a_full_game_heap_after_a_1024_mib_armed_line_keeps_its_text() {
    let loader = format!(
        "2026-11-25 16:43:15.077 [19700] [heap] GAME_DEFAULT heap armed: 1024 MiB instead of the game's 390 MiB when system/memory_map.xml is read\n{GAME_HEAP}"
    );
    let d = diagnose(&ev(&loader, &[])).unwrap();
    assert!(d.what.contains("doesn't change that"), "{}", d.what);
}

#[test]
fn a_full_game_heap_on_a_small_pc_says_so_and_offers_the_heap_setting() {
    let loader = format!("{SMALL_RAM}{GAME_HEAP}");
    let d = diagnose(&ev(&loader, &[])).unwrap();
    assert_eq!(d.id, "game_heap");
    assert!(d.what.contains("under 12 GB of RAM"), "{}", d.what);
    assert!(!d.what.contains("doesn't change that"), "{}", d.what);
    assert!(
        d.steps
            .last()
            .unwrap()
            .contains("\"SWOFTY_GAME_HEAP_MB\": \"1024\""),
        "{:?}",
        d.steps
    );
}

#[test]
fn a_full_game_heap_on_a_normal_pc_keeps_its_text() {
    let d = diagnose(&ev(GAME_HEAP, &[])).unwrap();
    assert!(
        d.what.contains("More memory in your PC doesn't change that."),
        "{}",
        d.what
    );
    assert!(
        !d.steps.join(" ").contains("SWOFTY_GAME_HEAP_MB"),
        "{:?}",
        d.steps
    );
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
