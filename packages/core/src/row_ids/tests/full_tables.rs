use super::*;

#[test]
fn vehicle_and_team_rows_never_take_an_id_past_16382() {
    let data = data_dir("vehicle-ceiling");
    put(&reports::path(&data, reports::IDS), &pinned_vehicles(16381));
    let (store, _) = Store::load(&data, None, "p", None);
    let built = build(VEHICLES, 0, &[("b", &["v1"])], store);
    assert_eq!(built.ids["v1"], 16382);
    assert!(built.left_out.is_empty());
    let seasons: Vec<String> = (0..20).map(|i| format!("s{i}")).collect();
    let refs: Vec<&str> = seasons.iter().map(String::as_str).collect();
    for table in [VEHICLES, TEAMS] {
        let built = build(table, 16379, &[("b", &refs)], Store::default());
        assert!(
            built.ids.values().all(|id| (16380..=16382).contains(id)),
            "{:?}",
            built.ids
        );
        assert_eq!(built.ids.len(), 3);
        assert_eq!(built.left_out["b"], 17);
    }
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_vehicle_row_that_finds_no_id_is_left_out_without_failing_the_build() {
    let data = data_dir("vehicle-left-out");
    put(&reports::path(&data, reports::IDS), &pinned_vehicles(16382));
    let (store, _) = Store::load(&data, None, "p", None);
    let built = build(VEHICLES, 0, &[("a", &["v1", "v2"]), ("b", &["w1"])], store);
    assert!(built.ids.is_empty());
    assert_eq!(built.left_out["a"], 2);
    assert_eq!(built.left_out["b"], 1);
    assert!(
        !built
            .store
            .rows
            .keys()
            .any(|k| k.contains("entrants.vehicles.vehicle/"))
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn two_addons_never_share_a_vehicle_id() {
    let labels: Vec<String> = (0..10).map(|i| format!("c{i}")).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut store = Store::default();
    let mut all = BTreeSet::new();
    for addon in ["s1972", "s1986", "s2000"] {
        let built = build(VEHICLES, 509, &[(addon, &refs)], store);
        for id in built.ids.values() {
            assert!(all.insert(*id), "{id} given twice");
        }
        store = built.store;
    }
    assert_eq!(all.len(), 30);
}

#[test]
fn driver_ids_above_255_skip_254_255_and_low_byte_0xff() {
    const DRIVERS: (&str, &str) = ("entrants.drivers", "driver");
    let labels: Vec<String> = (0..120).map(|i| format!("d{i}")).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let built = build(DRIVERS, 254, &[("a", &refs)], Store::default());
    let ids: BTreeSet<i64> = built.ids.values().copied().collect();
    assert_eq!(ids.len(), 120);
    assert!(ids.iter().any(|id| *id > 255));
    assert!(
        ids.iter()
            .all(|id| (208..=16382).contains(id) && *id != 254 && id & 0xff != 0xff)
    );
    let again = build(DRIVERS, 254, &[("a", &refs)], built.store);
    assert_eq!(again.ids, built.ids);
}

#[test]
fn a_full_table_reuses_an_id_kept_for_an_addon_the_build_leaves_out_and_logs_it() {
    let first = build(
        PACKAGES,
        250,
        &[("a", &["a1", "a2", "a3", "a4"])],
        Store::default(),
    );
    let mut ids: Vec<i64> = first.ids.values().copied().collect();
    ids.sort();
    assert_eq!(ids, [251, 252, 253, 254]);
    let e = try_build(
        PACKAGES,
        250,
        &[("a", &["a1", "a2", "a3", "a4"]), ("b", &["b1"])],
        Store::default(),
    )
    .err()
    .unwrap();
    assert!(e.to_string().contains("has no free stat_id up to 254"), "{e}");
    let holder = |id: i64| first.ids.iter().find(|(_, v)| **v == id).unwrap().0.clone();
    let b = build(PACKAGES, 250, &[("b", &["b1", "b2"])], first.store);
    assert_eq!((b.ids["b1"], b.ids["b2"]), (251, 252));
    let line = format!(
        "b: add-row entrants.vehicles.vehicle_packages.vehicle_package_1 (from entrants.vehicles.vehicle_packages.vehicle_package_0): stat_id 251 (the source's is 250; no other id up to 254 is free; reused from {}, which this build leaves out)",
        key("a", PACKAGES, &holder(251))
    );
    assert!(b.lines.contains(&line), "{:?}", b.lines);
    assert_eq!(b.store.get(&key("a", PACKAGES, &holder(251))), None);
    assert_eq!(b.store.get(&key("a", PACKAGES, &holder(253))), Some(253));
    assert_eq!(b.store.from[&key("b", PACKAGES, "b1")], IdFrom::Reused);
    assert_eq!(
        b.store.summary().as_deref(),
        Some("database: row-ids.json keeps 4 added-row id(s) (2 new, 2 reused)")
    );
}

#[test]
fn a_full_table_takes_ids_kept_for_rows_an_addon_in_the_build_no_longer_adds() {
    let (ids, store) = leaves_out_rows(&["a1", "a2", "a3", "a4"], None).unwrap();
    assert_eq!(
        ids.values().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from([251, 252, 253, 254])
    );
    assert_eq!(store.reused, 4);
    assert_eq!(store.get(&key("b", PACKAGES, "old0")), None);
}

#[test]
fn a_full_table_never_takes_an_id_a_career_uses_from_an_addon_in_the_build() {
    let (ids, _) = leaves_out_rows(&["a1", "a2", "a3"], Some(254)).unwrap();
    assert!(ids.values().all(|id| *id != 254), "{ids:?}");
    let e = leaves_out_rows(&["a1", "a2", "a3", "a4"], Some(254))
        .err()
        .unwrap();
    assert!(e.to_string().contains("has no free stat_id up to 254"), "{e}");
}

#[test]
fn two_rows_of_one_addon_with_one_label_keep_their_own_ids() {
    let first = build(VEHICLES, 10, &[("a", &["x", "x"])], Store::default());
    let x = first.store.get(&key("a", VEHICLES, "x")).unwrap();
    let x2 = first
        .store
        .get(&format!("{}#2", key("a", VEHICLES, "x")))
        .unwrap();
    assert_ne!(x, x2);
    let again = build(VEHICLES, 10, &[("b", &["b1"]), ("a", &["x", "x"])], first.store);
    assert_eq!(again.store.get(&key("a", VEHICLES, "x")), Some(x));
    assert_eq!(
        again.store.get(&format!("{}#2", key("a", VEHICLES, "x"))),
        Some(x2)
    );
}

#[test]
fn mode_ids_stay_under_the_mask_and_take_the_next_free_id_in_load_order() {
    let labels: Vec<String> = (0..10).map(|i| format!("mode_{i}")).collect();
    let many: Vec<&str> = labels.iter().map(String::as_str).collect();
    let addons: &[(&str, &[&str])] = &[("bydanix", &["bdx_2012_mode"]), ("many", &many)];
    let built = try_build_on(table_to(MODES, 20), MODES, addons, Store::default()).unwrap();
    assert_eq!(built.ids["bdx_2012_mode"], 21);
    assert_eq!(built.ids["mode_9"], 31);
    assert!(built.ids.values().all(|id| *id <= 31), "{:?}", built.ids);
    assert_eq!(
        built.store.from[&key("bydanix", MODES, "bdx_2012_mode")],
        IdFrom::Next
    );
    let full: &[(&str, &[&str])] = &[
        ("bydanix", &["bdx_2012_mode"]),
        ("many", &many),
        ("more", &["one"]),
    ];
    let e = try_build_on(table_to(MODES, 20), MODES, full, Store::default())
        .err()
        .unwrap();
    assert!(e.to_string().contains("has no free stat_id up to 31"), "{e}");
    let mut kept = Store::default();
    kept.put(&key("bydanix", MODES, "bdx_2012_mode"), 44, IdFrom::Hashed);
    let moved = try_build_on(
        table_to(MODES, 20),
        MODES,
        &[("bydanix", &["bdx_2012_mode"])],
        kept,
    )
    .unwrap();
    assert_eq!(moved.ids["bdx_2012_mode"], 21);
    assert!(
        moved.lines.iter().any(|l| l.ends_with(
            "stat_id 21 (the source's is 0; 44, kept for it, is above 31, the highest the game reads)"
        )),
        "{:?}",
        moved.lines
    );
    let core_first: &[(&str, &[&str])] = &[("swofty.core", &["swofty_f1"]), ("bydanix", &["bdx_2012_mode"])];
    let entrants = try_build_on(
        table_to(ENTRANT_MODES, 12),
        ENTRANT_MODES,
        core_first,
        Store::default(),
    )
    .unwrap();
    assert_eq!(
        (entrants.ids["swofty_f1"], entrants.ids["bdx_2012_mode"]),
        (13, 14)
    );
    let seasons = ("game_setup.seasons", "season");
    assert_eq!(stat_id_field(seasons.0, seasons.1), "statId");
    assert_eq!(stat_id_ceiling(seasons.0, seasons.1).map(|c| c.max), Some(31));
}
