use super::super::logos::logos_unwritable;
use super::*;

const LOGOS_LOG: &str = "\
2026-11-17 19:04:28.900 [19524] [apply] team.logo: 33 input archive(s) (389 MB) hashed in 266 ms
2026-11-17 19:04:28.928 [19524] [apply] team.logo: the game can't create files in swofty/addons/swofty.logos/ (error 5), so no team logo is built this session
2026-11-17 19:04:28.928 [19524] [apply] team.identity boot:   team.logo: the game can't create files in swofty/addons/swofty.logos/ (error 5), so no team logo is built this session
2026-11-17 19:04:30.548 [19524] [apply] team.logo read: 12 applied, 0 put back, 0 refused
";

#[test]
fn logos_the_game_could_not_write_name_the_folder_and_the_error() {
    let d = unbuilt_logos(&ev(LOGOS_LOG, &[]), || false, "").expect("the logo line is recognised");
    assert_eq!(d.id, "logos_not_built");
    assert!(
        d.what
            .contains(r"swofty\addons\swofty.logos in its game folder (error 5)"),
        "{}",
        d.what
    );
    assert!(d.steps[0].contains("administrator"), "{:?}", d.steps);
    assert_eq!(
        logos_unwritable(LOGOS_LOG),
        Some(("swofty/addons/swofty.logos".to_owned(), "5".to_owned()))
    );
}

#[test]
fn a_launcher_that_already_runs_as_administrator_says_to_move_the_game() {
    let hint = " with EA app > F1 25 > Manage > Move, not File Explorer";
    let d = unbuilt_logos(&ev(LOGOS_LOG, &[]), || true, hint).expect("the logo line is recognised");
    assert_eq!(d.id, "logos_not_built");
    assert!(
        !d.steps[0].contains("run Swofty Launcher as administrator"),
        "{:?}",
        d.steps
    );
    assert!(
        d.steps[0].starts_with("Swofty Launcher already runs as administrator")
            && d.steps[0].ends_with(
                "Move F1 25 to a folder outside Program Files with EA app > F1 25 > Manage > Move, not File \
                 Explorer, then press Play."
            ),
        "{}",
        d.steps[0]
    );
}

#[test]
fn built_or_cached_logos_are_not_a_problem() {
    for log in [
        "2026-11-17 19:04:28.928 [1] [apply] team.logo: 713 slice(s) built into 34 archive(s) in 8183 ms\n",
        "2026-11-17 19:04:28.928 [1] [apply] team.logo: 34 cached archive(s) of 0123456789abcdef reused\n",
        "2026-11-17 19:04:28.928 [1] [apply] team.logo: 2025_asset_groups/ui_package/flow_race.erp could not be rewritten\n",
        "",
    ] {
        assert!(
            unbuilt_logos(&ev(log, &[]), || panic!("elevation is read only for a match"), "").is_none(),
            "{log}"
        );
    }
}
