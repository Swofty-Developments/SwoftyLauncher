use super::*;

#[test]
fn a_track_keeps_its_stat_id_when_other_tracks_are_turned_off() {
    let all = sixteen();
    let mut store = Store::default();
    let first = track_build(&mut store, &refs(&all), &[]);
    assert_eq!(first["sokol"], STOCK_TRACKS + 9);
    let few: Vec<_> = all
        .iter()
        .filter(|(_, c)| c == "sokol" || c == "t2" || c == "t13")
        .cloned()
        .collect();
    let second = track_build(&mut store, &refs(&few), &[]);
    assert_eq!(second["sokol"], first["sokol"]);
    assert_eq!(second["t13"], first["t13"]);
    let only: Vec<_> = all.iter().filter(|(_, c)| c == "sokol").cloned().collect();
    assert_eq!(
        track_build(&mut store, &refs(&only), &[])["sokol"],
        first["sokol"]
    );
}

#[test]
fn a_turned_off_tracks_stat_id_is_never_given_to_another_track() {
    let all = sixteen();
    let mut store = Store::default();
    let first = track_build(&mut store, &refs(&all), &[]);
    let off: Vec<_> = all.iter().filter(|(_, c)| c == "t14").cloned().collect();
    let mut with_new = refs(&off);
    with_new.insert(0, ("mod.fresh", "fresh"));
    let second = track_build(&mut store, &with_new, &[]);
    assert!(!first.values().any(|id| *id == second["fresh"]));
    assert_ne!(second["fresh"], first["t14"]);
    assert_ne!(second["fresh"], first["t13"]);
    let again = track_build(&mut store, &refs(&all), &[]);
    assert_eq!(again["t13"], first["t13"]);
    assert_eq!(again["t14"], first["t14"]);
    let ids: BTreeSet<i64> = again.values().copied().collect();
    assert_eq!(ids.len(), again.len());
}

#[test]
fn a_track_without_a_kept_id_takes_the_id_the_last_build_gave_it() {
    let mut store = Store::default();
    let on = [("mod.sokol", "sokol"), ("mod.other", "other")];
    let got = track_build(&mut store, &on, &[("sokol", 48), ("other", 48)]);
    assert_eq!(got["sokol"], 48);
    assert_ne!(got["other"], 48);
    assert!(got["other"] >= STOCK_TRACKS);
}

#[test]
fn a_lent_track_id_is_the_one_the_build_gives_even_where_a_turned_off_track_keeps_it() {
    let mut store = Store::default();
    let own = track_build(
        &mut store,
        &[
            ("mod.lemans", "le_mans_2017"),
            ("track.bahrain_outer", "bahrain_outer"),
            ("mod.brhgp", "brhgp"),
        ],
        &[],
    );
    assert_eq!(own["le_mans_2017"], STOCK_TRACKS);
    assert_eq!(own["brhgp"], STOCK_TRACKS + 2);
    let host = Overrides {
        rows: BTreeMap::from([
            (
                "track.bahrain_outer".to_owned(),
                BTreeMap::from([(track_row("bahrain_outer"), STOCK_TRACKS)]),
            ),
            (
                "mod.brhgp".to_owned(),
                BTreeMap::from([(track_row("brhgp"), STOCK_TRACKS + 6)]),
            ),
        ]),
        ..Overrides::default()
    };
    let on = [("track.bahrain_outer", "bahrain_outer"), ("mod.brhgp", "brhgp")];
    let order: Vec<String> = on.iter().map(|(a, _)| (*a).to_owned()).collect();
    assert_eq!(store.lend(&host, &order), Ok(2));
    let lent = track_build(&mut store, &on, &[]);
    assert_eq!(lent["bahrain_outer"], STOCK_TRACKS);
    assert_eq!(lent["brhgp"], STOCK_TRACKS + 6);
    assert_eq!(store.lent_missed(), None);
    assert!(!store.unsaved(), "row-ids.json keeps the player's own ids");
}

#[test]
fn a_lent_track_id_a_track_of_the_build_keeps_is_refused_by_name() {
    let mut store = Store::default();
    let on = [
        ("mod.lemans", "le_mans_2017"),
        ("track.bahrain_outer", "bahrain_outer"),
    ];
    track_build(&mut store, &on, &[]);
    let host = Overrides {
        rows: BTreeMap::from([(
            "track.bahrain_outer".to_owned(),
            BTreeMap::from([(track_row("bahrain_outer"), STOCK_TRACKS)]),
        )]),
        ..Overrides::default()
    };
    let order: Vec<String> = on.iter().map(|(a, _)| (*a).to_owned()).collect();
    let err = store.lend(&host, &order).unwrap_err();
    assert!(err.contains("le_mans_2017"), "{err}");
}

#[test]
fn track_stat_ids_stay_within_what_the_game_reads() {
    let mut store = Store::default();
    let (id, _) = store
        .track_stat_id("mod.a", "a", Some(300), 250, &|_| false)
        .unwrap();
    assert_eq!(id, 250);
    let err = store.track_stat_id("mod.b", "b", None, 0, &|_| true).unwrap_err();
    assert!(err.contains("254"), "{err}");
}

#[test]
fn a_full_track_pool_lends_the_id_of_a_track_whose_addon_is_off() {
    let mut store = Store::default();
    for i in 0..=TRACK_STAT_MAX {
        store
            .track_stat_id(&format!("mod.t{i}"), "t", None, 0, &|_| false)
            .unwrap();
    }
    let off = store.get("mod.t7/tracks.Track/t").unwrap();
    let on: Vec<String> = (0..=TRACK_STAT_MAX)
        .filter(|i| *i != 7)
        .map(|i| format!("mod.t{i}"))
        .chain(["mod.new".to_owned()])
        .collect();
    store.building(on.clone());
    let before: Vec<i64> = (0..=TRACK_STAT_MAX)
        .filter(|i| *i != 7)
        .map(|i| store.get(&format!("mod.t{i}/tracks.Track/t")).unwrap())
        .collect();
    // The new track asks first: no track of the build has asked yet, and none of them lends its id.
    let (id, why) = store.track_stat_id("mod.new", "t", None, 0, &|_| false).unwrap();
    assert_eq!(id, off);
    assert!(why.contains("mod.t7"), "{why}");
    assert_eq!(store.get("mod.t7/tracks.Track/t"), None);
    let after: Vec<i64> = on
        .iter()
        .filter(|a| *a != "mod.new")
        .map(|a| store.track_stat_id(a, "t", None, 0, &|_| false).unwrap().0)
        .collect();
    assert_eq!(after, before);

    let mut full = Store::default();
    for i in 0..=TRACK_STAT_MAX {
        full.track_stat_id(&format!("mod.t{i}"), "t", None, 0, &|_| false)
            .unwrap();
    }
    full.building(
        (0..=TRACK_STAT_MAX)
            .map(|i| format!("mod.t{i}"))
            .chain(["mod.x".to_owned()]),
    );
    for i in 0..=TRACK_STAT_MAX {
        full.track_stat_id(&format!("mod.t{i}"), "t", None, 0, &|_| false)
            .unwrap();
    }
    let err = full.track_stat_id("mod.x", "t", None, 0, &|_| false).unwrap_err();
    assert!(err.contains("254"), "{err}");
}

#[test]
fn a_kept_track_id_that_stock_now_holds_moves_and_is_aliased() {
    let mut store = Store::default();
    store.track_stat_id("mod.a", "a", None, 46, &|_| false).unwrap();
    store.building(["mod.a".to_owned()]);
    let (id, why) = store
        .track_stat_id("mod.a", "a", None, 47, &|id| id == 46)
        .unwrap();
    assert_eq!(id, 47);
    assert!(why.contains("46"), "{why}");
    assert_eq!(store.get("mod.a/tracks.Track/a"), Some(47));
}
