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

const SESSION_START: &str = "2026-11-03 09:59:22.218 [1] [career] flow \"StateSession\" None -> Entering\n";
const RESTORE: &str =
    "2026-11-03 09:59:25.100 [1] [saves] profile load 2: read from Steam native; savegame@midsessions0\n";

#[test]
fn unplaced_tyres_after_a_mid_session_load_say_restart() {
    let log = format!("{SESSION_START}{RESTORE}{TYRES_LOG}");
    let d = unplaced_tyres(&ev(&log, &[])).unwrap();
    assert!(d.what.contains("after loading a mid-session save"), "{}", d.what);
    assert!(!d.what.contains("addon"), "{}", d.what);
    assert!(d.steps[0].contains("Restart the session"), "{:?}", d.steps);
    assert!(d.steps.iter().all(|s| !s.contains("turn it off")));
}

#[test]
fn unplaced_tyres_in_a_fresh_session_keep_the_addon_advice() {
    let boot_load = "2026-11-03 09:58:00.000 [1] [saves] profile load 1: read from Steam native\n";
    let log = format!("{boot_load}{SESSION_START}{TYRES_LOG}");
    let d = unplaced_tyres(&ev(&log, &[])).unwrap();
    assert!(d.what.contains("addon's car or handling data"), "{}", d.what);
    assert!(d.steps[0].contains("turn it off"), "{:?}", d.steps);
}
