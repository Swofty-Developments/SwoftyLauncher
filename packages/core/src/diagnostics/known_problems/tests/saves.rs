use super::super::saves::{failed_saves, unsaved_profile};
use super::*;

/// The shape of session 20261121-134011's log: a race highlights save the session before had gone into the plain store
/// as a profile save, the load put its replay container back, and each profile save then overflowed the game's buffer.
const STUCK_LOG: &str = "\
2026-11-22 00:41:12.100 [616] [saves] put back ReplayHighlights_ReplayHighlights: 0 career(s) from generation 63
2026-11-22 00:41:12.300 [616] [saves] profile load 1: kept 5 save container(s) no loaded game mode claims (30331577 bytes); the next profile save writes them back
2026-11-22 00:52:03.410 [604] [saves] profile save: writing back the kept save containers failed with code 55; the save fails as on any write error
2026-11-22 00:58:44.020 [604] [saves] profile save: writing back the kept save containers failed with code 55; the save fails as on any write error
";

#[test]
fn saves_that_failed_after_the_highlights_were_put_back_say_to_update() {
    let d = unsaved_profile(&ev(STUCK_LOG, &[])).expect("the failed saves are recognised");
    assert_eq!(d.id, "profile_not_saved");
    assert!(
        d.what.contains("2 times") && d.what.contains("race highlights"),
        "{}",
        d.what
    );
    assert!(d.what.contains("Your save file was not changed"), "{}", d.what);
    assert!(d.steps[0].starts_with("Update Swofty Launcher"), "{:?}", d.steps);
    assert!(
        d.steps[1].contains("Continue Without Saving and Cancel"),
        "{:?}",
        d.steps
    );
    assert_eq!(failed_saves(STUCK_LOG), [55, 55]);
}

#[test]
fn a_failed_save_without_the_highlights_names_its_code() {
    let log = "2026-11-23 10:00:00.000 [1] [saves] profile save: writing back the kept save containers failed with code \
               88 at gm_driver_career_1990 (4 kept, 5300000 bytes); the save fails as on any write error and the game \
               writes no file\n";
    let d = unsaved_profile(&ev(log, &[])).expect("the failed save is recognised");
    assert!(d.what.contains("once, code 88"), "{}", d.what);
    assert!(!d.steps[0].starts_with("Update"), "{:?}", d.steps);
    assert_eq!(failed_saves(log), [88]);
}

#[test]
fn saves_that_worked_are_not_a_problem() {
    for log in [
        "2026-11-21 13:41:44.488 [608] [saves] profile save: wrote back 6 save container(s) no loaded game mode claims: gm_driver_career_progressive\n",
        "2026-11-22 00:41:12.100 [616] [saves] put back ReplayHighlights_ReplayHighlights: 0 career(s) from generation 63\n",
        "",
    ] {
        assert!(unsaved_profile(&ev(log, &[])).is_none(), "{log}");
    }
}
