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
