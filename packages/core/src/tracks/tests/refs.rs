use super::*;

#[test]
fn records_naming_a_track_not_in_the_build_get_their_shipped_track_back() {
    let mut shipped = db();
    shipped
        .set(
            "schedule.activities._activity_race_weekend_instancecount_",
            "2",
            None,
        )
        .unwrap();
    shipped.records.extend([
        rec(&format!("{ACTIVITIES}_1._metadata_"), "okayama"),
        rec(&format!("{ACTIVITIES}_1.track"), "okayama"),
        rec("track_data.game_setup._TrackDataSet_instancecount_", "1"),
        rec(&format!("{TRACK_DATA}_0._metadata_"), "f1"),
        rec(&format!("{TRACK_DATA}_0._TrackData_instancecount_"), "1"),
        rec(&format!("{TRACK_DATA}_0.TrackData_0._metadata_"), "suzuka"),
        rec(&format!("{TRACK_DATA}_0.TrackData_0.track"), "suzuka"),
        rec("game_setup.game_setup_modes._game_setup_mode_instancecount_", "1"),
        rec(&format!("{GAME_MODES}_0._metadata_"), "f1"),
        rec(
            &format!("{GAME_MODES}_0.restricted_tracks"),
            "okayama,shipped_only",
        ),
    ]);
    let mut untouched = shipped.clone();
    assert!(keep_track_refs(&mut untouched, &shipped, &[], &[]).is_empty());
    let mut built = shipped.clone();
    let mut set = |path: &str, value: &str| built.set(path, value, None).unwrap();
    set("schedule.activities._activity_race_weekend_instancecount_", "4");
    set(&format!("{ACTIVITIES}_0.track"), "istanbul_park");
    set(&format!("{TRACK_DATA}_0.TrackData_0.track"), "paul_ricard");
    set(
        "content_sets.content_set_1.available_tracks_0.tracks",
        "suzuka,istanbul_park,okayama",
    );
    set(
        &format!("{GAME_MODES}_0.restricted_tracks"),
        "okayama,shipped_only,hockenheim,ring",
    );
    built.records.extend([
        rec(&format!("{ACTIVITIES}_2._metadata_"), "rw_added"),
        rec(&format!("{ACTIVITIES}_2.track"), "hockenheim"),
        rec(&format!("{ACTIVITIES}_3._metadata_"), "rw_left"),
        rec(&format!("{ACTIVITIES}_3.track"), "portimao"),
    ]);
    let missing = keep_track_refs(&mut built, &shipped, &["ring"], &["portimao"]);
    let got: Vec<(&str, &str, &MissingFix)> = missing
        .iter()
        .map(|m| (m.at.as_str(), m.track.as_str(), &m.fix))
        .collect();
    assert_eq!(
        got,
        [
            (
                "race weekend suzuka",
                "istanbul_park",
                &MissingFix::Stock("suzuka".into())
            ),
            ("race weekend rw_added", "hockenheim", &MissingFix::Kept),
            (
                "track settings f1/suzuka",
                "paul_ricard",
                &MissingFix::Stock("suzuka".into())
            ),
            ("content set 2026", "istanbul_park", &MissingFix::Dropped),
            ("game mode f1", "hockenheim", &MissingFix::Dropped),
        ]
    );
    assert_eq!(missing[0].key, fnv1a64(format!("{ACTIVITIES}_0.track")));
    let text = |path: &str| built.get_text(path).unwrap();
    assert_eq!(text(&format!("{ACTIVITIES}_0.track")), "suzuka");
    assert_eq!(text(&format!("{ACTIVITIES}_2.track")), "hockenheim");
    assert_eq!(text(&format!("{ACTIVITIES}_3.track")), "portimao");
    assert_eq!(text(&format!("{TRACK_DATA}_0.TrackData_0.track")), "suzuka");
    assert_eq!(
        text("content_sets.content_set_1.available_tracks_0.tracks"),
        "suzuka,okayama"
    );
    assert_eq!(
        text(&format!("{GAME_MODES}_0.restricted_tracks")),
        "okayama,shipped_only,ring"
    );
    let again = keep_track_refs(&mut built, &shipped, &["ring"], &["portimao"]);
    assert_eq!(again, [missing[1].clone()]);
    assert!(keep_track_refs(&mut Gvdb::default(), &shipped, &[], &[]).is_empty());
}

#[test]
fn missing_track_lines_name_the_addon_that_set_it_and_the_one_to_turn_on() {
    let missing = |at: &str, fix: MissingFix| MissingTrack {
        key: 0,
        at: at.into(),
        track: "istanbul_park".into(),
        fix,
    };
    let weekend = missing("race weekend bahrain", MissingFix::Stock("bahrain".into()));
    assert_eq!(
        weekend.line(Some("My Calendar"), Some("Istanbul Park")),
        "My Calendar sets race weekend bahrain to istanbul_park, which is off: turn Istanbul Park on; \
         until then this build puts back bahrain"
    );
    assert_eq!(
        weekend.line(None, None),
        "race weekend bahrain is set to istanbul_park, which isn't in this build; until then this build puts \
         back bahrain"
    );
    assert_eq!(
        missing("content set 2026", MissingFix::Dropped).line(Some("My Calendar"), Some("Istanbul Park")),
        "My Calendar adds istanbul_park to content set 2026, which is off: turn Istanbul Park on; until then \
         this build leaves it out"
    );
    let added = missing("race weekend rw_added", MissingFix::Kept);
    assert_eq!(
        added.line(Some("My Calendar"), Some("Istanbul Park")),
        "My Calendar sets race weekend rw_added to istanbul_park, which is off: turn Istanbul Park on or \
         change it in DB Editor; the game won't start like this"
    );
    assert_eq!(
        added.line(Some("My Calendar"), None),
        "My Calendar sets race weekend rw_added to istanbul_park, which isn't in this build: change it in DB \
         Editor; the game won't start like this"
    );
}

#[test]
fn a_track_list_held_twice_is_read_and_fixed_in_its_first_record() {
    let shipped = db();
    let mut built = shipped.clone();
    let path = "content_sets.content_set_0.available_tracks_0.tracks";
    built.set(path, "bahrain24,gone", None).unwrap();
    built.records.push(rec(path, "bahrain24"));
    let missing = keep_track_refs(&mut built, &shipped, &[], &[]);
    let got: Vec<(&str, &MissingFix)> = missing.iter().map(|m| (m.track.as_str(), &m.fix)).collect();
    assert_eq!(got, [("gone", &MissingFix::Dropped)]);
    assert_eq!(built.get_text(path).as_deref(), Some("bahrain24"));
}
