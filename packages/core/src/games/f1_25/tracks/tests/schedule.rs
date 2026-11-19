use super::*;

#[test]
fn weekends_at_a_left_out_track_go_back_to_their_stock_track() {
    let weekend = |i: usize, label: &str, track: &str| {
        [
            rec(&format!("{ACTIVITIES}_{i}._metadata_"), label),
            rec(&format!("{ACTIVITIES}_{i}.track"), track),
        ]
    };
    let mut shipped = db();
    shipped
        .set(
            "schedule.activities._activity_race_weekend_instancecount_",
            "2",
            None,
        )
        .unwrap();
    shipped.records.extend(weekend(1, "rw_okayama", "okayama"));
    let mut built = shipped.clone();
    built
        .set(
            "schedule.activities._activity_race_weekend_instancecount_",
            "4",
            None,
        )
        .unwrap();
    built
        .set(&format!("{ACTIVITIES}_0.track"), "portimao", None)
        .unwrap();
    built.records.extend(weekend(2, "rw_added", "portimao"));
    built.records.extend(weekend(3, "rw_other", "monza"));
    let warnings = restore_weekends(&mut built, &shipped, &["portimao", "okayama"]).unwrap();
    assert_eq!(
        warnings,
        [
            "race weekend suzuka: races at suzuka again; track portimao is left out of this build",
            "race weekend rw_added: races at suzuka instead; track portimao is left out of this build and the game won't start with a race weekend at it",
        ]
    );
    let track = |i: usize| built.get_text(&format!("{ACTIVITIES}_{i}.track")).unwrap();
    assert_eq!(
        [track(0), track(1), track(2), track(3)],
        ["suzuka", "okayama", "suzuka", "monza"]
    );
    let mut untouched = shipped.clone();
    assert!(
        restore_weekends(&mut untouched, &shipped, &["monza"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_calendar_addons_own_weekend_at_a_left_out_track_races_at_a_held_track() {
    let shipped = db();
    let mut built = shipped.clone();
    weekends(
        &mut built,
        &[("suzuka", "suzuka"), ("dejw_career_nurburgring", "nurburgring")],
    );
    let said = restore_weekends(&mut built, &shipped, &["nurburgring", "sepang"]).unwrap();
    assert_eq!(
        said,
        [
            "race weekend dejw_career_nurburgring: races at suzuka instead; track nurburgring is left out of \
          this build and the game won't start with a race weekend at it"
        ]
    );
    assert_eq!(
        built.get_text(&format!("{ACTIVITIES}_1.track")).unwrap(),
        "suzuka"
    );
}

#[test]
fn season_addon_weekends_at_left_out_tracks_race_at_melbourne_when_the_build_holds_it() {
    let shipped = db();
    let mut built = shipped.clone();
    built.set("tracks._Track_instancecount_", "4", None).unwrap();
    built
        .set("tracks.Track_3._metadata_", "melbourne", Some(2))
        .unwrap();
    weekends(
        &mut built,
        &[
            ("suzuka", "suzuka"),
            ("cs_brhgp", "brh"),
            ("cs_daytona", "daytona"),
            ("cs_hanoi_ac", "hanoi_ac"),
        ],
    );
    let said = restore_weekends(&mut built, &shipped, &["brh", "daytona", "hanoi_ac"]).unwrap();
    assert_eq!(said.len(), 3);
    for i in 1..4 {
        assert_eq!(
            built.get_text(&format!("{ACTIVITIES}_{i}.track")).unwrap(),
            "melbourne"
        );
    }
    assert_eq!(
        built.get_text(&format!("{ACTIVITIES}_0.track")).unwrap(),
        "suzuka"
    );
}

#[test]
fn a_weekend_with_no_track_to_move_to_stops_the_build() {
    let shipped = db();
    let mut built = shipped.clone();
    built.set("tracks._Track_instancecount_", "0", None).unwrap();
    weekends(&mut built, &[("dejw_career_nurburgring", "nurburgring")]);
    let stuck = restore_weekends(&mut built, &shipped, &["nurburgring"]).unwrap_err();
    assert_eq!(stuck.len(), 1);
    assert!(stuck[0].contains("the game won't start like this"));
}

#[test]
fn a_sorted_track_order_loses_a_left_out_track() {
    let mut built = Gvdb::default();
    let order = format!("{SORTED_ORDER}_0");
    built
        .set(
            "track_data.sorted_track_order._sorted_track_order_instancecount_",
            "1",
            Some(0),
        )
        .unwrap();
    built
        .set(
            &format!("{order}._metadata_"),
            "default_sorted_track_order",
            Some(2),
        )
        .unwrap();
    built
        .set(&format!("{order}._TrackRef_instancecount_"), "3", Some(0))
        .unwrap();
    for (i, t) in ["melbourne", "suzuka_reverse", "monza"].iter().enumerate() {
        built
            .set(&format!("{order}.TrackRef_{i}._metadata_"), *t, Some(2))
            .unwrap();
        built
            .set(&format!("{order}.TrackRef_{i}.track"), *t, Some(2))
            .unwrap();
    }
    let said = drop_left_out_orders(&mut built, &["suzuka_reverse"]);
    assert_eq!(
        said,
        [
            "sorted track order default_sorted_track_order listed suzuka_reverse, which is left out of this build; the build takes it out"
        ]
    );
    assert_eq!(
        built
            .get_text(&format!("{order}._TrackRef_instancecount_"))
            .unwrap(),
        "2"
    );
    assert_eq!(
        built.get_text(&format!("{order}.TrackRef_1.track")).unwrap(),
        "monza"
    );
    assert!(built.get(&format!("{order}.TrackRef_2.track")).is_none());
    assert!(drop_left_out_orders(&mut built, &["suzuka_reverse"]).is_empty());
}

#[test]
fn an_option_adding_rounds_at_a_track_the_build_lacks_is_named_and_present_or_filled_tracks_are_not() {
    let m = AddonManifest::parse(
        r#"{ "schema_version": 1, "id": "t.f2", "name": "F2", "version": "1.0.0", "author": "t",
                 "compat": { "game": "f1_25" },
                 "options": [
                   { "id": "istanbul", "type": "toggle", "default": false, "binds": "db" },
                   { "id": "spa", "type": "toggle", "default": false, "binds": "db" },
                   { "id": "portimao", "type": "toggle", "default": false, "binds": "db" },
                   { "id": "sochi", "type": "toggle", "default": false, "binds": "db" },
                   { "id": "laps", "type": "choice", "default": "a", "binds": "db",
                     "values": [{ "value": "a", "label": "A" }, { "value": "b", "label": "B" }] }
                 ] }"#,
    )
    .unwrap();
    let docs = vec![serde_json::json!({ "schema_version": 1, "ops": [
            { "op": "add-row", "table": "schedule.activities",
              "from": "schedule.activities.activity_race_weekend[_metadata_=spa_francorchamps]",
              "set": { "_metadata_": "istanbul_park", "track": "istanbul_park" }, "when": { "istanbul": true } },
            { "op": "add-row", "table": "schedule.all_track_events",
              "from": "schedule.all_track_events.fixed_event[_metadata_=rw_spa_francorchamps]",
              "set": { "_metadata_": "rw_istanbul_27", "activity": "istanbul_park" }, "when": { "istanbul": true } },
            { "op": "add-row", "table": "schedule.all_track_events",
              "from": "schedule.all_track_events.fixed_event[_metadata_=rw_spa_francorchamps]",
              "set": { "_metadata_": "rw_spa_27" }, "when": { "spa": true } },
            { "op": "add-row", "table": "schedule.all_track_events",
              "from": "schedule.all_track_events.fixed_event[_metadata_=rw_portimao]",
              "set": { "_metadata_": "rw_portimao_27" }, "when": { "portimao": true } },
            { "op": "add-row", "table": "schedule.all_track_events",
              "from": "schedule.all_track_events.fixed_event[_metadata_=rw_sochi]",
              "set": { "_metadata_": "rw_sochi_27" }, "when": { "sochi": true, "laps": "b" } },
            { "op": "add-row", "table": "schedule.all_track_events",
              "from": "schedule.all_track_events.fixed_event[_metadata_=rw_spa_francorchamps]",
              "set": { "_metadata_": "rw_mugello", "activity": "istanbul_park" } },
            { "op": "set", "select": "schedule.track_lists.track_event_list[_metadata_=F1_2027]",
              "field": "_fixed_event_reference_instancecount_", "value": "24",
              "when": { "istanbul": true, "portimao": true } }
        ] })];
    let stock = schedule();
    let found = |held: &[&str]| -> Vec<(String, String)> {
        let held: HashSet<String> = held.iter().map(|c| (*c).to_owned()).collect();
        option_tracks(&m, &docs, &stock, &stock.tracks.union(&held).cloned().collect())
            .into_iter()
            .map(|t| (t.option, t.track))
            .collect()
    };
    let pair = |o: &str, t: &str| (o.to_owned(), t.to_owned());
    assert!(!stock.tracks.contains("portimao") && stock.tracks.contains("spa_francorchamps"));
    assert_eq!(
        found(&[]),
        [
            pair("istanbul", "istanbul_park"),
            pair("portimao", "portimao"),
            pair("sochi", "sochi")
        ]
    );
    assert_eq!(found(&["istanbul_park", "portimao", "sochi"]), []);
    assert_eq!(
        found(&["sochi"]),
        [pair("istanbul", "istanbul_park"), pair("portimao", "portimao")]
    );
    let line = OptionTrack {
        option: "istanbul".into(),
        track: "istanbul_park".into(),
    }
    .line("F2 Revamp", "Turkish GP (Istanbul Park)", Some("Istanbul Park"));
    assert_eq!(
        line,
        "F2 Revamp: option Turkish GP (Istanbul Park) adds rounds at istanbul_park, which isn't in this build, \
         so the build leaves that option's rows out and builds as if it were off; turn Istanbul Park on, or \
         turn the option off"
    );
}

#[test]
fn a_large_database_s_schedule_is_read_in_one_pass_over_its_records() {
    let (weekends, events) = (400, 3000);
    let mut records: Vec<Record> = (0..200_000)
        .map(|i| rec(&format!("filler.row_{i}.value"), "x"))
        .collect();
    records.extend([
        rec("tracks._Track_instancecount_", "2"),
        rec(&format!("{TRACKS}_0._metadata_"), "melbourne"),
        rec(&format!("{TRACKS}_1._metadata_"), " spa_francorchamps "),
        rec(&format!("{TRACKS}_1._metadata_"), "monza"),
        rec(
            "schedule.activities._activity_race_weekend_instancecount_",
            &weekends.to_string(),
        ),
        rec(
            "schedule.all_track_events._fixed_event_instancecount_",
            &events.to_string(),
        ),
    ]);
    for i in 0..weekends {
        records.push(rec(&format!("{ACTIVITIES}_{i}._metadata_"), &format!("rw_{i}")));
        records.push(rec(&format!("{ACTIVITIES}_{i}.track"), "melbourne"));
    }
    for i in 0..events {
        records.push(rec(&format!("{EVENTS}_{i}._metadata_"), &format!("ev_{i}")));
        records.push(rec(
            &format!("{EVENTS}_{i}.activity"),
            &format!("rw_{}", i % weekends),
        ));
    }
    let db = Gvdb::new(records);
    let started = std::time::Instant::now();
    let schedule = Schedule::of(&db);
    let took = started.elapsed();
    assert_eq!(
        schedule.tracks,
        HashSet::from(["melbourne".to_owned(), "spa_francorchamps".to_owned()])
    );
    assert!(took < std::time::Duration::from_secs(1), "{took:?}");
}
