use super::*;

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
