use super::*;

#[test]
fn a_row_an_earlier_import_renamed_names_the_import() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in).names(swap_and_korea());
    p.push(op("mod.other", &set_round("TrackDataSet_0.TrackData_0")));
    p.push(op(SWAP, &put("TrackDataSet_2._metadata_", "f2_2025")));
    p.push(op(
        KOREA,
        &set_round("TrackDataSet[_metadata_=f2_2026].TrackData[_metadata_=bahrain]"),
    ));
    let e = p.apply_db(&mut track_sets()).unwrap_err().to_string();
    assert_eq!(
        e,
        "applying pajko.korea: track_data.game_setup.TrackDataSet[_metadata_=f2_2026]: no row matches, \
         even after every addon's changes: Korea edits that row, but SERPS Ineos Mercedes Concept renamed it \
         earlier in the load. Turn SERPS Ineos Mercedes Concept off, or move it after Korea"
    );
}

#[test]
fn rows_an_earlier_import_cut_from_the_count_name_the_import() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in).names(swap_and_korea());
    p.push(op(SWAP, &put("_TrackDataSet_instancecount_", "2")));
    p.push(op(
        KOREA,
        &set_round("TrackDataSet[_metadata_=f2_2026].TrackData_0"),
    ));
    let e = p.apply_db(&mut track_sets()).unwrap_err().to_string();
    assert!(
        e.ends_with(
            ": Korea edits that row, but SERPS Ineos Mercedes Concept removed it earlier in the load. \
             Turn SERPS Ineos Mercedes Concept off, or move it after Korea"
        ),
        "{e}"
    );
    assert!(
        e.starts_with(
            "applying pajko.korea: track_data.game_setup.TrackDataSet[_metadata_=f2_2026]: no row matches"
        ),
        "{e}"
    );
}

#[test]
fn a_child_row_under_a_table_an_earlier_import_renamed_names_the_import() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let names = named(&[
        (SWAP, "SERPS Ineos Mercedes Concept"),
        ("mod.lgsgp", "Laguna Seca Grand Prix"),
    ]);
    let mut p = Planner::new(&features, &opt_in).names(names);
    p.push(op(SWAP, &put("TrackDataSet_1._metadata_", "f2_2022")));
    p.push(op(SWAP, &put("TrackDataSet_2._metadata_", "f2_2023")));
    p.push(op(
        "mod.lgsgp",
        &put("TrackDataSet_1.TrackData_0._metadata_", "lgsgp"),
    ));
    p.push(op(
        "mod.lgsgp",
        &set_round("TrackDataSet[_metadata_=f2_2023].TrackData[_metadata_=lgsgp]"),
    ));
    let e = p.apply_db(&mut track_sets()).unwrap_err().to_string();
    assert!(
        e.ends_with(
            ": Laguna Seca Grand Prix edits that row, but SERPS Ineos Mercedes Concept renamed it earlier in the \
             load. Turn SERPS Ineos Mercedes Concept off, or move it after Laguna Seca Grand Prix"
        ),
        "{e}"
    );
}

#[test]
fn any_addons_change_to_a_track_left_out_for_room_is_skipped_with_a_warning() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in).names(named(&[(CAREER, "Classic Career")]));
    p.leave_out_track("anderst", "Anderstorp");
    p.push(op(
        CAREER,
        &set_round("TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=anderst]"),
    ));
    p.push(op(
        CAREER,
        &set_round("TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=bahrain]"),
    ));
    let mut db = track_sets();
    p.apply_db(&mut db).unwrap();
    assert_eq!(
        db.get_text(&format!("{SETS}.TrackDataSet_0.TrackData_0.round"))
            .as_deref(),
        Some("9")
    );
    assert_eq!(
        p.take_skipped(),
        [
            "Classic Career: change skipped: track_data.game_setup.TrackDataSet[_metadata_=f1_2026]\
             .TrackData[_metadata_=anderst]: no row matches in track_data.game_setup.TrackDataSet_0.TrackData; \
             it changes track Anderstorp, which is left out of this build because the game has room for 64 \
             tracks. Turn some track addons off to make room for it"
        ]
    );
}

#[test]
fn a_change_to_a_missing_track_that_is_not_left_out_still_fails_the_build() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in).names(named(&[(CAREER, "Classic Career")]));
    p.leave_out_track("anderst", "Anderstorp");
    p.push(op(
        CAREER,
        &set_round("TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=imola]"),
    ));
    assert_eq!(
        p.apply_db(&mut track_sets()).unwrap_err().to_string(),
        "applying career.classic-career: track_data.game_setup.TrackDataSet[_metadata_=f1_2026]\
         .TrackData[_metadata_=imola]: no row matches in track_data.game_setup.TrackDataSet_0.TrackData, \
         even after every addon's changes"
    );

    let mut p = Planner::new(&features, &opt_in).names(named(&[(CAREER, "Classic Career")]));
    p.push(op(
        CAREER,
        &set_round("TrackDataSet[_metadata_=f1_2026].TrackData[_metadata_=anderst]"),
    ));
    assert_eq!(
        p.apply_db(&mut track_sets()).unwrap_err().to_string(),
        "applying career.classic-career: track_data.game_setup.TrackDataSet[_metadata_=f1_2026]\
         .TrackData[_metadata_=anderst]: no row matches in track_data.game_setup.TrackDataSet_0.TrackData, \
         even after every addon's changes"
    );
}

#[test]
fn a_lenient_addons_skipped_change_names_the_import_too() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let lenient = named(&[("dbedit.mine", "My DB Edits")]);
    let mut p = Planner::new(&features, &opt_in)
        .lenient(lenient)
        .names(named(&[(SWAP, "A"), ("mod.second", "B")]));
    p.push(op(SWAP, &put("TrackDataSet_2._metadata_", "f2_2025")));
    p.push(op("mod.second", &put("_TrackDataSet_instancecount_", "2")));
    p.push(op(
        "dbedit.mine",
        &set_round("TrackDataSet[_metadata_=f2_2026].TrackData_0"),
    ));
    p.apply_db(&mut track_sets()).unwrap();
    assert_eq!(
        p.take_skipped(),
        [
            "My DB Edits: change skipped: track_data.game_setup.TrackDataSet[_metadata_=f2_2026]: no row matches, \
             even after every addon's changes: My DB Edits edits that row, but A renamed it earlier in the load. \
             Turn A off, or move it after My DB Edits"
        ]
    );
}

#[test]
fn a_row_nothing_removed_or_only_a_later_addon_renamed_keeps_the_plain_error() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let plain = "applying pajko.korea: track_data.game_setup.TrackDataSet[_metadata_=f2_2030]: no row matches, \
                 even after every addon's changes";
    let mut p = Planner::new(&features, &opt_in).names(swap_and_korea());
    p.push(op(SWAP, &put("TrackDataSet_1._metadata_", "f2_2025")));
    p.push(op(
        KOREA,
        &set_round("TrackDataSet[_metadata_=f2_2030].TrackData_0"),
    ));
    assert_eq!(p.apply_db(&mut track_sets()).unwrap_err().to_string(), plain);

    let mut p = Planner::new(&features, &opt_in).names(swap_and_korea());
    p.push(op(
        KOREA,
        &set_round("TrackDataSet[_metadata_=f2_2026].TrackData_0"),
    ));
    p.push(op(SWAP, &put("TrackDataSet_2._metadata_", "f2_2030")));
    let mut db = track_sets();
    p.apply_db(&mut db).unwrap();
    assert_eq!(
        db.get_text(&format!("{SETS}.TrackDataSet_2.TrackData_0.round"))
            .as_deref(),
        Some("9")
    );

    let mut p = Planner::new(&features, &opt_in).names(swap_and_korea());
    p.push(op(KOREA, &put("TrackDataSet_2._metadata_", "f2_2030")));
    p.push(op(
        KOREA,
        &set_round("TrackDataSet[_metadata_=f2_2026].TrackData_0"),
    ));
    let e = p.apply_db(&mut track_sets()).unwrap_err().to_string();
    assert_eq!(e, plain.replace("f2_2030", "f2_2026"));
}
