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
