use super::*;

#[test]
fn a_tests_lend_never_reaches_a_build_in_another_test() {
    let host = Overrides {
        order: vec!["a".into()],
        ..Default::default()
    };
    lend(Some(host.clone()));
    let elsewhere = std::thread::spawn(lent).join().unwrap();
    assert_eq!((lent(), elsewhere), (Some(host), None));
    lend(None);
}

#[test]
fn the_hosts_order_fills_the_places_its_addons_held_and_is_part_of_the_cache_key() {
    let host = Overrides {
        order: vec!["c".into(), "a".into()],
        options: BTreeMap::from([(
            "a".to_owned(),
            BTreeMap::from([("laps".to_owned(), serde_json::json!(5))]),
        )]),
        ..Default::default()
    };
    let own: Vec<String> = ["a", "x", "c"].map(String::from).to_vec();
    assert_eq!(host.arranged(&own), ["c", "x", "a"]);
    assert!(!host.is_empty());
    let stamp = host.stamp(&own).unwrap();
    assert!(
        stamp.contains("room settings") && stamp.contains("laps"),
        "{stamp}"
    );
    assert_eq!(
        host.stamp(&["x".to_owned()]),
        None,
        "an addon outside the build adds nothing"
    );
    let other = Overrides {
        options: BTreeMap::from([(
            "a".to_owned(),
            BTreeMap::from([("laps".to_owned(), serde_json::json!(6))]),
        )]),
        ..host.clone()
    };
    assert_ne!(other.stamp(&own), host.stamp(&own));
}

#[test]
fn the_hosts_ids_are_used_for_one_build_and_never_written() {
    let (data, own) = players_store("lend-apply");
    assert_eq!(
        own.ids,
        BTreeMap::from([("a1".into(), 11), ("a2".into(), 12), ("b1".into(), 13)])
    );
    let before = std::fs::read(path(&data)).unwrap();
    let (mut store, _) = Store::load(&data, None, "default", None);
    let host = host_ids(&[("a", "a1", 21), ("a", "a2", 22), ("b", "b1", 23)]);
    assert_eq!(store.lend(&host, &order(&["a", "b"])), Ok(3));
    let lent = build(VEHICLES, 10, &[("a", &["a1", "a2"]), ("b", &["b1"])], store);
    assert_eq!(
        lent.ids,
        BTreeMap::from([("a1".into(), 21), ("a2".into(), 22), ("b1".into(), 23)])
    );
    assert_eq!(lent.store.lent_missed(), None);
    lent.store.save(&data).unwrap();
    assert_eq!(
        std::fs::read(path(&data)).unwrap(),
        before,
        "row-ids.json is as the player's own build left it"
    );
    let (own_again, _) = Store::load(&data, None, "default", None);
    let again = build(VEHICLES, 10, &[("a", &["a1", "a2"]), ("b", &["b1"])], own_again);
    assert_eq!(
        again.ids, own.ids,
        "the next normal build has the player's own ids"
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn two_addons_that_swap_ids_with_the_host_are_not_a_collision() {
    let (data, _) = players_store("lend-swap");
    let (mut store, _) = Store::load(&data, None, "default", None);
    let host = host_ids(&[("a", "a1", 13), ("b", "b1", 11)]);
    assert_eq!(store.lend(&host, &order(&["a", "b"])), Ok(2));
    let lent = build(VEHICLES, 10, &[("a", &["a1", "a2"]), ("b", &["b1"])], store);
    assert_eq!(lent.ids["a1"], 13);
    assert_eq!(lent.ids["b1"], 11);
    assert_eq!(lent.store.lent_missed(), None);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn an_id_another_row_of_the_build_uses_is_refused_and_one_of_an_addon_left_out_is_not() {
    let (data, _) = players_store("lend-collide");
    let (mut store, _) = Store::load(&data, None, "default", None);
    let host = host_ids(&[("a", "a1", 13)]);
    let err = store.lend(&host, &order(&["a", "b"])).unwrap_err();
    assert!(
        err.contains("stat id 13") && err.contains("a1") && err.contains("b1 of b"),
        "{err}"
    );
    let (mut store, _) = Store::load(&data, None, "default", None);
    assert_eq!(store.lend(&host, &order(&["a"])), Ok(1));
    let lent = build(VEHICLES, 10, &[("a", &["a1", "a2"])], store);
    assert_eq!(lent.ids["a1"], 13);
    assert_eq!(lent.store.lent_missed(), None);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_host_id_the_game_registry_holds_is_refused() {
    let data = data_dir("lend-pinned");
    let (mut store, _) = Store::load(&data, None, "default", None);
    store.pinned.insert(
        "entrants.vehicles.vehicle".into(),
        HashMap::from([(40, "a/a_record".to_owned())]),
    );
    let err = store
        .lend(&host_ids(&[("a", "a1", 40)]), &order(&["a"]))
        .unwrap_err();
    assert!(err.contains("registry record a/a_record"), "{err}");
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_lent_id_an_outside_addon_pins_is_the_one_the_build_gives() {
    let data = data_dir("lend-pinned-build");
    let (mut store, _) = Store::load(&data, None, "default", None);
    store.pinned.insert(
        "entrants.vehicles.vehicle".into(),
        HashMap::from([(13, "z.indycar/z_record".to_owned())]),
    );
    assert_eq!(store.lend(&host_ids(&[("a", "a1", 13)]), &order(&["a"])), Ok(1));
    let lent = build(VEHICLES, 10, &[("a", &["a1", "a2"])], store);
    assert_eq!(lent.ids["a1"], 13);
    assert_eq!(lent.store.lent_missed(), None);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_host_id_held_by_a_registry_record_of_an_addon_outside_the_build_is_lent() {
    let data = data_dir("lend-pinned-off");
    let (mut store, _) = Store::load(&data, None, "default", None);
    store.pinned.insert(
        "entrants.vehicles.vehicle".into(),
        HashMap::from([(40, "z.indycar/z_record".to_owned())]),
    );
    assert_eq!(store.lend(&host_ids(&[("a", "a1", 40)]), &order(&["a"])), Ok(1));
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn the_cache_key_names_only_the_overrides_of_addons_in_the_build() {
    let host = host_ids(&[("a", "a1", 21), ("z", "z1", 5)]);
    let with = host.stamp(&order(&["a", "b"])).unwrap();
    assert!(
        with.contains("a/entrants.vehicles.vehicle/a1") && !with.contains("z1"),
        "{with}"
    );
    assert_ne!(
        Some(with),
        host_ids(&[("a", "a1", 22)]).stamp(&order(&["a", "b"]))
    );
    assert_eq!(host.stamp(&order(&["b"])), None);
    assert_eq!(Overrides::default().stamp(&order(&["a"])), None);
}

#[test]
fn a_lent_row_leaves_the_id_aliases_alone() {
    let (data, _) = players_store("lend-aliases");
    let (mut store, _) = Store::load(&data, None, "default", None);
    let key = key("a", VEHICLES, "a1");
    store.claimed.insert(key.clone());
    store.aliases.insert(key.clone(), BTreeSet::from([5]));
    assert!(
        store
            .id_aliases()
            .tables
            .get("vehicles")
            .is_some_and(|t| t.get(&5) == Some(&11))
    );
    store
        .lend(&host_ids(&[("a", "a1", 21)]), &order(&["a", "b"]))
        .unwrap();
    assert!(store.id_aliases().is_empty(), "{:?}", store.id_aliases());
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn the_hosts_registry_ids_become_the_cores_room_file_for_the_addons_in_the_build() {
    let lent = Overrides {
        registry: BTreeMap::from([
            (
                "a".to_owned(),
                BTreeMap::from([
                    ("content.package:a/season".to_owned(), 116),
                    ("content.package.scenario:a/season".to_owned(), 26),
                ]),
            ),
            (
                "gone".to_owned(),
                BTreeMap::from([("vehicle:gone/car".to_owned(), 9)]),
            ),
        ]),
        ..Default::default()
    };
    assert!(!lent.is_empty());
    assert_eq!(
        lent.room_stat_ids(&order(&["a", "b"])).unwrap(),
        serde_json::json!({
            "version": 1,
            "ids": {
                "content.package": { "a/season": 116 },
                "content.package.scenario": { "a/season": 26 }
            }
        })
    );
    assert!(lent.room_stat_ids(&order(&["b"])).is_none());
    let stamp = lent.stamp(&order(&["a"])).unwrap();
    assert!(stamp.contains("116"), "{stamp}");
    assert_ne!(lent.stamp(&order(&["a"])), lent.stamp(&order(&["gone"])));
    let newer = Overrides {
        registry: BTreeMap::from([("a".to_owned(), BTreeMap::new())]),
        ..Default::default()
    };
    assert!(lent.merged(newer).registry["a"].is_empty());
}

#[test]
fn overrides_round_trip_through_their_file() {
    let data = data_dir("lend-file");
    let file = data.join("room.json");
    let host = host_ids(&[("a", "a1", 21)]);
    host.write(&file).unwrap();
    assert_eq!(Overrides::read(&file).unwrap(), host);
    assert_eq!(
        Overrides::read(&data.join("none.json")).unwrap(),
        Overrides::default()
    );
    std::fs::remove_dir_all(&data).ok();
}
