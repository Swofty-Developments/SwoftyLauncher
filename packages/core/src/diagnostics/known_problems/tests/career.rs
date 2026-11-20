use super::*;

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

const NO_WEEKEND: &str = "\
2026-11-17 10:25:53.419 [ 2100] [Progressive season] Imported 12 rounds into the Career picker.
2026-11-17 10:25:53.773 [ 2100] [Progressive season] Advancing imported calendar.
2026-11-17 10:25:53.773 [ 2100] [Free Calendar] Confirmed 12 selected rounds.
2026-11-17 10:31:07.402 [ 1500] [view] pause reason bit 0 cleared, mask 00000000
2026-11-17 10:31:07.453 [ 1500] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 1500, 412 s after load, at F1_25.exe+0x240bacb .tls$ fn base+0x240b9b0+0x11b
2026-11-17 10:31:07.453 [ 1500] [crash]   reading 0x250
2026-11-17 10:31:07.453 [ 1500] [crash]   r12=0000000001000000 r13=0000000002000000 r14=0000000000000000 r15=0000000000000000
2026-11-17 10:31:07.454 [ 1500] [crash]   stack:
2026-11-17 10:31:07.454 [ 1500] [crash]   #0  F1_25.exe+0x240bacb .tls$ fn base+0x240b9b0+0x11b
2026-11-17 10:31:07.454 [ 1500] [crash]   #1  F1_25.exe+0x2da9fb1 .tls$ fn base+0x2da9e00+0x1b1
2026-11-17 10:31:07.456 [ 1500] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x240bacb .tls$ fn base+0x240b9b0+0x11b reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_first_session_without_a_weekend_after_a_calendar_import_names_the_picker() {
    let mut e = with_names(
        ev(NO_WEEKEND, &[]),
        &[(CUSTOM_CALENDAR, "Custom Calendar"), (F2_REVAMP, "F2 Revamp")],
    );
    e.enabled = vec![CUSTOM_CALENDAR.to_owned(), F2_REVAMP.to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "imported_calendar_empty_season");
    assert_eq!(
        d.title,
        "Custom Calendar's imported calendar left your career without a first round"
    );
    assert!(
        d.what.contains("F2 Revamp's career, which starts in F2"),
        "{}",
        d.what
    );
    assert!(
        d.steps[0].contains("without the Custom Calendar picker (F9)"),
        "{}",
        d.steps[0]
    );

    e.enabled = vec![CUSTOM_CALENDAR.to_owned()];
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "imported_calendar_empty_season");
    assert!(!d.what.contains("F2 Revamp"), "{}", d.what);

    let named = NO_WEEKEND.replace(
        " fn base+0x240b9b0+0x11b",
        " Sess_WeekendManager_PrepareTrackLoad+0x11b",
    );
    assert_eq!(
        diagnose(&ev(&named, &[])).map(|d| d.id),
        Some("imported_calendar_empty_season"),
        "the frame by name"
    );
    let no_import = NO_WEEKEND.replace(
        "[Progressive season] Imported 12",
        "[Progressive season] Accepted 12",
    );
    assert_eq!(diagnose(&ev(&no_import, &[])).map(|d| d.id), None, "no import");
    let late = format!(
        "{NO_WEEKEND}2026-11-17 10:40:00.000 [ 2100] [Progressive season] Imported 3 rounds into the Career picker.\n"
    );
    let late = late.replacen(
        "[Progressive season] Imported 12",
        "[Progressive season] Accepted 12",
        1,
    );
    assert_eq!(
        diagnose(&ev(&late, &[])).map(|d| d.id),
        None,
        "an import after the crash"
    );
    let other = NO_WEEKEND.replace("+0x240b9b0+0x11b", "+0x240b9b0+0x1d2");
    assert_eq!(
        diagnose(&ev(&other, &[])).map(|d| d.id),
        None,
        "another offset of the function"
    );
    let read = NO_WEEKEND.replace("reading 0x250", "reading 0x268");
    assert_eq!(diagnose(&ev(&read, &[])).map(|d| d.id), None, "another read");
}

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
