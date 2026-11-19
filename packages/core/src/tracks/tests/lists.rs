use super::*;

#[test]
fn the_track_lists_hold_the_stock_tracks_with_track_data_but_madrid() {
    let listed = ListedTracks::of(&stock_lists());
    assert_eq!(listed.stock(), 35);
    assert!(!listed.lists("madrid"));
    assert!(!listed.lists("bahrain24"));
    assert!(listed.lists("texas_nat"));
    assert_eq!(ListedTracks::of(&Gvdb::default()).stock(), 0);
}

#[test]
fn only_enough_added_tracks_to_fill_the_lists_read_the_database() {
    assert!(!may_fill_track_lists(21));
    assert!(may_fill_track_lists(22));
}

#[test]
fn the_tracks_past_the_track_lists_room_are_left_out_in_load_order() {
    let listed = ListedTracks::of(&stock_lists());
    let mut tracks = Vec::new();
    for slot in ["bahrain24", "miami24", "suzuka24", "paul_ricard"] {
        tracks.push(build_track(&format!("mod.{slot}"), slot, slot));
    }
    tracks.push(build_track("mod.hanoi", "hanoi", "hanoi"));
    tracks.push(build_track("mod.sochi", "sochi", "melbourne"));
    let added: Vec<String> = (0..36).map(|i| format!("added_{i:02}")).collect();
    for code in &added {
        tracks.push(build_track(&format!("mod.{code}"), code, "melbourne"));
    }
    assert_eq!(tracks.len(), 42);
    let (kept, left_out) = fit_track_lists(&listed, tracks.clone(), &BTreeSet::new());
    let mut want: Vec<&str> = vec![
        "bahrain24",
        "miami24",
        "suzuka24",
        "paul_ricard",
        "hanoi",
        "sochi",
    ];
    want.extend(added[..29].iter().map(String::as_str));
    assert_eq!(codes(&kept), want);
    let gone: Vec<&str> = left_out
        .iter()
        .map(|((_, p, _), _)| p.track_code.as_str())
        .collect();
    assert_eq!(gone, added[29..].iter().map(String::as_str).collect::<Vec<_>>());
    for ((_, pkg, origin), why) in &left_out {
        assert_eq!(why.code.as_deref(), Some(pkg.track_code.as_str()));
        assert_eq!(origin.addon, format!("mod.{}", pkg.track_code));
    }
    assert_eq!(
        left_out[0].1.text(),
        "track added_29 circuit is left out; the game has room for 64 tracks, 35 of its own and 29 added ones, \
         and the track addons before it in the load order fill them. Turn some track addons off to make room \
         for it"
    );
    assert_eq!(
        fit_track_lists(&listed, tracks, &BTreeSet::new()),
        (kept, left_out)
    );
}

#[test]
fn a_track_with_a_base_that_has_no_track_data_takes_no_room() {
    let listed = ListedTracks::of(&stock_lists());
    let mut tracks: Vec<BuildTrack> = (0..29)
        .map(|i| build_track(&format!("mod.r{i}"), &format!("r{i}"), "melbourne"))
        .collect();
    tracks.push(build_track("mod.ricard", "magny_cours", "paul_ricard"));
    tracks.push(build_track("mod.sepang", "bahrain24", "bahrain"));
    tracks.push(build_track("mod.late", "late", "suzuka"));
    let (kept, left_out) = fit_track_lists(&listed, tracks, &BTreeSet::new());
    assert_eq!(kept.len(), 30);
    assert!(codes(&kept).contains(&"magny_cours"));
    let gone: Vec<&str> = left_out
        .iter()
        .map(|((_, p, _), _)| p.track_code.as_str())
        .collect();
    assert_eq!(gone, ["bahrain24", "late"]);
}

#[test]
fn the_tracks_the_core_adds_take_their_room_first() {
    let listed = ListedTracks::of(&stock_lists());
    let mut tracks: Vec<BuildTrack> = (0..29)
        .map(|i| build_track(&format!("mod.r{i}"), &format!("r{i}"), "melbourne"))
        .collect();
    tracks.push(build_track("std.recorded", "recorded_a", "suzuka"));
    tracks.push(build_track("std.recorded", "recorded_b", "suzuka"));
    let recorded: BTreeSet<String> = ["std.recorded".to_owned()].into();
    let (kept, left_out) = fit_track_lists(&listed, tracks, &recorded);
    let gone: Vec<&str> = left_out
        .iter()
        .map(|((_, p, _), _)| p.track_code.as_str())
        .collect();
    assert_eq!(gone, ["r27", "r28"]);
    assert_eq!(&codes(&kept)[27..], ["recorded_a", "recorded_b"]);
}

#[test]
fn the_track_limit_lists_each_addon_once_and_is_remembered_per_profile() {
    let listed = ListedTracks::of(&stock_lists());
    assert_eq!(TrackLimit::new(&listed, Vec::new(), Vec::new()), None);
    let limit = TrackLimit::new(
        &listed,
        vec!["A".into(), "B".into(), "C".into()],
        vec!["x.two".into(), "x.one".into(), "x.two".into()],
    )
    .unwrap();
    assert_eq!((limit.room, limit.stock), (64, 35));
    assert_eq!(limit.addons, ["x.one", "x.two"]);
    let root = std::env::temp_dir().join(format!("swofty-track-limit-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    assert!(recorded_track_limit(&root, "p").is_empty());
    record_track_limit(&root, "p", Some(&limit)).unwrap();
    assert_eq!(recorded_track_limit(&root, "p").len(), 2);
    assert!(recorded_track_limit(&root, "other").is_empty());
    record_track_limit(&root, "p", None).unwrap();
    assert!(recorded_track_limit(&root, "p").is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_full_lists_notice_names_the_tracks_left_out() {
    let listed = ListedTracks::of(&stock_lists());
    assert_eq!(track_lists_full_warning(&listed, &[]), None);
    assert_eq!(
        track_lists_full_warning(&listed, &["Mugello".into(), "Kymi Ring (Kymiring)".into()]).unwrap(),
        "Too many tracks: 2 track(s) are left out of this build because the game has room for 64 tracks, 35 \
         of its own and 29 added ones, and more stop it at a black screen after the EA logo: Mugello, Kymi \
         Ring (Kymiring). Turn some track addons off to play them"
    );
    let many: Vec<String> = (0..10).map(|i| format!("T{i}")).collect();
    assert!(
        track_lists_full_warning(&listed, &many)
            .unwrap()
            .contains(": T0, T1, T2, T3, T4, T5, T6, T7 and 2 more. Turn")
    );
}

#[test]
fn a_track_row_label_held_twice_is_read_from_its_first_record() {
    let db = Gvdb::new(vec![
        Record::new("tracks._Track_instancecount_", 0, "1"),
        rec("tracks.Track_0._metadata_", "first"),
        rec("tracks.Track_0._metadata_", "later"),
        Record::new("track_data.game_setup._TrackDataSet_instancecount_", 0, "1"),
        Record::new(&format!("{TRACK_DATA}_0._TrackData_instancecount_"), 0, "1"),
        rec(&format!("{TRACK_DATA}_0.TrackData_0.track"), "first"),
    ]);
    let listed = ListedTracks::of(&db);
    assert!(listed.lists("first"));
    assert!(!listed.lists("later"));
}
