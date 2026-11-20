use super::*;

/// szariq_'s session on a save written inside the round 12 weekend (launcher 1.10.4, 20 Nov 2026): Team HQ twice,
/// then the game closed from Team HQ and hit its exit fault.
const STUCK_LOG: &str = "\
2026-11-20 11:57:41.140 [ 6676] [career] career.hub entered (mode driver_career)
2026-11-20 11:57:41.141 [ 6676] [career] weekend already started, session never ran: round 12/22, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/hungaroring
2026-11-20 11:57:48.584 [20544] [career] weekend already started, session never ran: round 12/22, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/hungaroring
2026-11-20 11:58:34.222 [21836] [crash] crash report swofty\\crash-20261120-115833.txt: TerminateProcess(0 = 0x0) on thread 5600, 0.0 s after a fatal-class exception; earlier exception's stack F1_25.exe+0x53804e8 .tls$ fn base+0x53804d8+0x10
";

const EXIT_FAULT_REPORT: &str = "== Game state (from the black box)\n  \
        last flow      #e11edb87 Exiting (1.4 s before this report)\n  \
        last screen    driver_hq_panel (5.1 s before this report)\n  \
        last session   none recorded\n  \
        last race      none recorded\n  \
        last career    hub left (3.4 s before this report)\n  \
        last heap      none recorded\n  \
        last exception first-chance ACCESS_VIOLATION at F1_25.exe+0x53804e8 .tls$ fn base+0x53804d8+0x10 (0.0 s before this report)\n";

#[test]
fn a_session_that_ended_in_a_started_weekend_names_the_round() {
    let d = stuck_weekend(&ev(STUCK_LOG, &[])).expect("the never ran line is the last weekend line");
    assert_eq!(d.id, "stuck_weekend");
    assert!(
        d.what
            .contains("saved inside its round 12 of 22 (hungaroring) weekend before any session of it ran"),
        "{}",
        d.what
    );
    assert!(d.steps[0].starts_with("Open Career saves"), "{:?}", d.steps);
    let loaded = format!(
        "{STUCK_LOG}2026-11-20 11:59:00.000 [1] [career] flow \"StateLoadSession\" None -> Entering\n"
    );
    assert!(stuck_weekend(&ev(&loaded, &[])).is_none(), "practice 1 loaded");
    assert!(stuck_weekend(&ev("", &[])).is_none());
}

#[test]
fn the_exit_fault_of_a_stuck_session_names_the_stuck_weekend() {
    let mut e = ev(STUCK_LOG, &[]);
    e.report = Some(EXIT_FAULT_REPORT.to_owned());
    assert_eq!(diagnose(&e).unwrap().id, "stuck_weekend");
    e.loader = STUCK_LOG.replace("weekend already started, session never ran", "Team HQ weekend");
    assert_eq!(
        diagnose(&e).unwrap().id,
        "quit_fault",
        "a healthy weekend keeps the exit fault"
    );
}

/// The hub opening as practice 1 of round 12 opened, the session ending, and the crash after it (launcher 1.9.88, 19
/// Nov 2026): a season break runs over the race weekend.
const SENT_TO_HUB: &str = "\
2026-11-19 21:58:03.186 [ 5548] [career] hub opened: day 170, season 0, 2 of 29 calendar items running today (driver 0x00090000, calendar 0x345515990); the calendar check reads: go to Team HQ (item 1, SeasonBreak, decides)
2026-11-19 21:58:03.186 [ 5548] [career]   item 1/2: SeasonBreak (0x0c) days 166-180 state 1 id 36 at 0x16ff2c20 vt 0x1456d31f0
2026-11-19 21:58:03.186 [ 5548] [career]   item 2/2: RaceWeekend (0x0a) days 169-172 state 1 id 41 at 0x16ff2c40 vt 0x1456d31f0
2026-11-19 21:58:03.513 [26532] [career] flow \"StateSession\" Updating -> Exiting
2026-11-19 21:58:03.513 [26532] [career] session ended 1484 ms after start (StateSession exited; the career hub opened 1171 ms after start): round 12/21, weekend in progress, current session 1 of 9 (practice_1), 0 of 9 sessions complete, track game/zandvoort; day 170, season 0, 2 of 29 calendar items running today (driver 0x00090000, calendar 0x345515990); the calendar check reads: go to Team HQ (item 1, SeasonBreak, decides)
2026-11-19 21:58:04.967 [17232] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 17232, 538 s after load, at F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c
2026-11-19 21:58:04.967 [17232] [crash]   reading 0x9c58
2026-11-19 21:58:04.968 [17232] [crash]   stack:
2026-11-19 21:58:04.969 [17232] [crash]   #0  F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c
2026-11-19 21:58:04.974 [17232] [crash]   #1  F1_25.exe+0x42c28bb .tls$ fn base+0x42c26b0+0x20b
2026-11-19 21:58:04.974 [17232] [crash]   #2  F1_25.exe+0x398e8bc .tls$ fn base+0x398e800+0xbc
2026-11-19 21:58:05.002 [17232] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c reached the top-level filter; passing it to the game's filter
";

/// The same crash on a core whose calendar reads compiled out (szariq_, launcher 1.10.x, 20 Nov 2026).
const SENT_TO_HUB_UNREAD: &str = "\
2026-11-20 13:26:27.830 [ 2404] [career] hub opened: the calendar is not readable (this core was built without the calendar layout)
2026-11-20 13:26:27.859 [14572] [career] flow \"StateSession\" Updating -> Exiting
2026-11-20 13:26:27.859 [14572] [career] session ended 578 ms after start (StateSession exited; the career hub opened 562 ms after start): round 12/22, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/hungaroring; the calendar is not readable (this core was built without the calendar layout)
2026-11-20 13:26:28.555 [20792] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 20792, 1437 s after load, at F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c
2026-11-20 13:26:28.555 [20792] [crash]   reading 0x9c58
2026-11-20 13:26:28.556 [20792] [crash]   stack:
2026-11-20 13:26:28.556 [20792] [crash]   #0  F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c
2026-11-20 13:26:28.556 [20792] [crash]   #1  F1_25.exe+0x42c28bb .tls$ fn base+0x42c26b0+0x20b
2026-11-20 13:26:28.557 [20792] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x1f8be5c .tls$ fn base+0x1f8aff0+0xe6c reached the top-level filter; passing it to the game's filter
";

#[test]
fn a_session_the_calendar_check_sent_to_team_hq_names_the_season_break() {
    let d = diagnose(&ev(SENT_TO_HUB, &[])).expect("a diagnosis");
    assert_eq!(d.id, "sent_to_team_hq");
    assert!(
        d.what.contains("the round 12 of 21 (zandvoort) weekend"),
        "{}",
        d.what
    );
    assert!(
        d.what.contains("a season break running over this race weekend"),
        "{}",
        d.what
    );
    assert!(d.steps[0].starts_with("Update Swofty Launcher"), "{:?}", d.steps);
}

#[test]
fn the_same_crash_on_a_core_without_the_calendar_reads_asks_for_an_update() {
    let d = diagnose(&ev(SENT_TO_HUB_UNREAD, &[])).expect("a diagnosis");
    assert_eq!(d.id, "sent_to_team_hq");
    assert!(
        d.what.contains("the round 12 of 22 (hungaroring) weekend"),
        "{}",
        d.what
    );
    assert!(!d.what.contains("season break"), "{}", d.what);
    assert!(d.steps[0].starts_with("Update Swofty Launcher"), "{:?}", d.steps);
}

#[test]
fn after_the_season_break_fix_the_crash_asks_for_logs_only() {
    let fixed = SENT_TO_HUB.replacen(
        "vt 0x1456d31f0\n2026-11-19 21:58:03.513",
        "vt 0x1456d31f0\n2026-11-19 21:58:03.190 [ 5548] [career] season break over race weekend: SeasonBreak (0x0c) days 166-180 state 1 id 36 at 0x16ff2c20 vt 0x1456d31f0 runs over the race weekend days 169-172 (id 41) that the session is in; set to ignored (state 4), so the race weekend decides\n2026-11-19 21:58:03.513",
        1,
    );
    let d = diagnose(&ev(&fixed, &[])).expect("a diagnosis");
    assert_eq!(d.id, "sent_to_team_hq");
    assert!(!d.steps.iter().any(|s| s.starts_with("Update")), "{:?}", d.steps);
}

#[test]
fn the_crash_alone_is_not_a_session_sent_to_team_hq() {
    let alone: String = SENT_TO_HUB
        .lines()
        .filter(|l| !l.contains("session ended"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(diagnose(&ev(&alone, &[])).map(|d| d.id), Some("sent_to_team_hq"));
}
