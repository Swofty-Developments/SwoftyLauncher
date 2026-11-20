use super::*;

/// `SWOFTY_KP_CASES=<folder of extracted Send Logs bundles> cargo test -p swofty-core --lib
/// diagnoses_real_bundles -- --ignored --nocapture` prints each crashed session's diagnosis,
/// and each other session a rule recognises.
#[test]
fn unmatched_crash_on_old_build_names_the_update() {
    let mut e = ev("", &[]);
    assert_eq!(diagnose(&e), None);
    e.game_version = Some("1.0.136.6820".to_owned());
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "outdated_exe");
    assert!(d.what.contains("1.0.136.6820"));
    assert!(d.text().contains("Steam or the EA app"));
    e.game_version = Some(LATEST_GAME_VERSION.to_owned());
    assert_eq!(diagnose(&e), None);
}

const APPLY_FAILED: &str = "applying swofty.classic-seasons: eaid://ui_package/databases/overrides.uidb: \
                                overrides.Contents.teams.Contents.mp_team_select_logos.Contents: no entry \
                                [Name=vanamersfoort_26]";

fn classic_seasons() -> BTreeMap<String, String> {
    BTreeMap::from([("swofty.classic-seasons".to_owned(), "Classic Seasons".to_owned())])
}

#[test]
fn game_versions_compare_by_number() {
    assert_eq!(older_game("1.0.138.5601", LATEST_GAME_VERSION), Some(true));
    assert_eq!(older_game("1.0.136.6820", "1.0.141.7865"), Some(true));
    assert_eq!(older_game("1.0.141.7865", LATEST_GAME_VERSION), Some(true));
    assert_eq!(older_game("1.0.145.2472", LATEST_GAME_VERSION), Some(false));
    assert_eq!(older_game("1.0.146.1", LATEST_GAME_VERSION), Some(false));
    assert_eq!(older_game("1.0.141.10000", "1.0.141.7865"), Some(false));
    assert_eq!(older_game("1.0.99.9999", "1.0.141.7865"), Some(true));
    assert_eq!(older_game(" 1.0.137.2811 ", LATEST_GAME_VERSION), Some(true));
    for unknown in [
        "",
        "1.0.138",
        "1.0.138.5601.2",
        "1.0.x.5601",
        "25251167",
        "1..138.5601",
        "1.0.+8.5601",
    ] {
        assert_eq!(older_game(unknown, LATEST_GAME_VERSION), None, "{unknown}");
    }
}

#[test]
fn an_addon_that_does_not_apply_to_an_old_game_names_the_update() {
    let d = outdated_game(APPLY_FAILED, Some("1.0.136.6820"), &classic_seasons()).unwrap();
    assert_eq!(d.id, "outdated_game");
    assert_eq!(d.title, "Classic Seasons needs the latest F1 25");
    assert_eq!(
        d.text(),
        "Classic Seasons needs F1 25 1.0.145.2472, the latest update. Your F1 25 is 1.0.136.6820. Update F1 \
             25 in Steam or the EA app, then press Play again."
    );
    let wrapped = format!("the wrapper exited (exit code: 1) before the game started: {APPLY_FAILED}");
    let d = outdated_game(&wrapped, Some("1.0.138.5601"), &BTreeMap::new()).unwrap();
    assert!(
        d.what
            .starts_with("swofty.classic-seasons needs F1 25 1.0.145.2472"),
        "{}",
        d.what
    );
    let item = APPLY_FAILED.replace("classic-seasons:", "classic-seasons (2018):");
    let d = outdated_game(&item, Some("1.0.137.2811"), &classic_seasons()).unwrap();
    assert!(d.what.starts_with("Classic Seasons needs"), "{}", d.what);
}

#[test]
fn other_games_and_other_errors_keep_their_own_message() {
    let names = classic_seasons();
    assert_eq!(
        outdated_game(APPLY_FAILED, Some(LATEST_GAME_VERSION), &names),
        None
    );
    assert_eq!(outdated_game(APPLY_FAILED, Some("1.0.146.1"), &names), None);
    assert_eq!(outdated_game(APPLY_FAILED, None, &names), None);
    assert_eq!(outdated_game(APPLY_FAILED, Some("unknown"), &names), None);
    for other in [
        "version.dll already exists in the game folder; run swofty restore",
        "building profile default: reading x: missing",
        "downloading the offline play extension: offline play 0.3.11: the download was blocked",
        "applying : no entry",
    ] {
        assert_eq!(
            outdated_game(other, Some("1.0.136.6820"), &names),
            None,
            "{other}"
        );
    }
}

#[test]
fn an_older_game_is_told_to_update_before_play() {
    let d = outdated_exe(Some("1.0.138.5601")).unwrap();
    assert_eq!(d.id, "outdated_exe");
    assert!(d.what.starts_with("Your F1 25 is 1.0.138.5601."), "{}", d.what);
    assert!(
        d.text().contains("Update F1 25 in Steam or the EA app"),
        "{}",
        d.text()
    );
    assert!(d.text().contains("steam://validate/3059520"), "{}", d.text());
    let d = outdated_exe(Some("1.0.141.7865")).unwrap();
    assert!(
        d.what
            .contains("Mods are made for 1.0.145.2472, the latest update"),
        "{}",
        d.what
    );
    assert_eq!(outdated_exe(Some(LATEST_GAME_VERSION)), None);
    assert_eq!(outdated_exe(Some("1.0.146.1")), None);
    assert_eq!(outdated_exe(Some("unknown")), None);
    assert_eq!(outdated_exe(None), None);
}
