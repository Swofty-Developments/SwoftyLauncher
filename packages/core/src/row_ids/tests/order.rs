use super::*;

#[test]
fn new_ids_are_the_next_free_ids_in_load_order_as_builds_before_1_6_10_gave_them() {
    let first = build(
        VEHICLES,
        10,
        &[("a", &["a1", "a2"]), ("b", &["b1"])],
        Store::default(),
    );
    assert_eq!(
        first.ids,
        BTreeMap::from([("a1".into(), 11), ("a2".into(), 12), ("b1".into(), 13)])
    );
    assert!(
        first
            .lines
            .iter()
            .any(|l| l.ends_with(": stat_id 13 (the source's is 10)")),
        "{:?}",
        first.lines
    );
    assert_eq!(first.store.from[&key("b", VEHICLES, "b1")], IdFrom::Next);
    let again = build(
        VEHICLES,
        10,
        &[("a", &["a1", "a2"]), ("b", &["b1"])],
        Store::default(),
    );
    assert_eq!(
        again.ids, first.ids,
        "identical setups built fresh get identical ids"
    );
}

// minik8824 (1565755736410161313): a fresh data folder at 1.6.11 hashed every id, and his save from before
// 1.6.10 held Classic Seasons 2019's teams at 550-559, the ids those builds gave them in load order.
#[test]
fn a_fresh_install_gives_a_save_from_before_1_6_10_its_sequential_ids_back() {
    let years: [(&str, &str); 5] = [
        ("swofty.classic-seasons-2017", "cs17"),
        ("swofty.classic-seasons-2018", "cs18"),
        ("swofty.classic-seasons-2020", "cs20"),
        ("swofty.classic-seasons-2021", "cs21"),
        ("swofty.classic-seasons-2019", "cs19"),
    ];
    let teams = [
        "mercedes",
        "ferrari",
        "redbull",
        "renault",
        "haas",
        "mclaren",
        "racingpoint",
        "alfaromeo",
        "tororosso",
        "williams",
    ];
    let labels: Vec<Vec<String>> = years
        .iter()
        .map(|(_, y)| teams.iter().map(|t| format!("{y}_{t}")).collect())
        .collect();
    let refs: Vec<Vec<&str>> = labels
        .iter()
        .map(|l| l.iter().map(String::as_str).collect())
        .collect();
    let set: Vec<(&str, &[&str])> = years
        .iter()
        .zip(&refs)
        .map(|((a, _), l)| (*a, l.as_slice()))
        .collect();
    let fresh = try_build_on(table_to(TEAMS, 509), TEAMS, &set, Store::default()).unwrap();
    let cs19: Vec<i64> = labels[4].iter().map(|l| fresh.ids[l]).collect();
    assert_eq!(cs19, (550..560).collect::<Vec<_>>());
    assert_eq!(fresh.ids["cs17_mercedes"], 510);
    let cs16: Vec<String> = teams.iter().map(|t| format!("cs16_{t}")).collect();
    let cs16: Vec<&str> = cs16.iter().map(String::as_str).collect();
    let mut more: Vec<(&str, &[&str])> = vec![("swofty.classic-seasons-2016", cs16.as_slice())];
    more.extend(set.iter().copied());
    let later = try_build_on(table_to(TEAMS, 509), TEAMS, &more, fresh.store).unwrap();
    let cs19_later: Vec<i64> = labels[4].iter().map(|l| later.ids[l]).collect();
    assert_eq!(
        cs19_later, cs19,
        "a year added in front later leaves the kept ids where they are"
    );
    assert_eq!(later.ids["cs16_mercedes"], 560);
}

#[test]
fn an_order_swap_keeps_each_rows_id() {
    let first = build(
        VEHICLES,
        10,
        &[("a", &["a1", "a2"]), ("b", &["b1"])],
        Store::default(),
    );
    let swapped = build(VEHICLES, 10, &[("b", &["b1"]), ("a", &["a1", "a2"])], first.store);
    assert_eq!(swapped.ids, first.ids);
    assert!(
        swapped.lines.iter().any(|l| l.ends_with(&format!(
            ": stat_id {} (the source's is 10; kept from row-ids.json)",
            first.ids["b1"]
        ))),
        "{:?}",
        swapped.lines
    );
    assert_eq!(swapped.store.summary(), None);
}

#[test]
fn an_update_that_adds_a_row_keeps_the_old_ids_and_gives_the_new_row_a_fresh_one() {
    let first = build(
        VEHICLES,
        10,
        &[("a", &["a1", "a2"]), ("b", &["b1"])],
        Store::default(),
    );
    let updated = build(
        VEHICLES,
        10,
        &[("a", &["a0", "a1", "a2"]), ("b", &["b1"])],
        first.store,
    );
    for label in ["a1", "a2", "b1"] {
        assert_eq!(updated.ids[label], first.ids[label]);
    }
    assert_eq!(updated.ids["a0"], 14);
    assert_eq!(
        updated.store.summary().as_deref(),
        Some("database: row-ids.json keeps 4 added-row id(s) (1 new)")
    );
}

#[test]
fn a_removed_addon_keeps_its_ids_and_gets_them_back() {
    let first = build(VEHICLES, 10, &[("a", &["a1"]), ("b", &["b1"])], Store::default());
    let without = build(VEHICLES, 10, &[("b", &["b1"]), ("c", &["c1"])], first.store);
    assert_eq!(without.ids["b1"], first.ids["b1"]);
    assert_ne!(without.ids["c1"], first.ids["a1"]);
    let back = build(
        VEHICLES,
        10,
        &[("a", &["a1"]), ("b", &["b1"]), ("c", &["c1"])],
        without.store,
    );
    assert_eq!(back.ids["a1"], first.ids["a1"]);
    assert_eq!(back.ids["b1"], first.ids["b1"]);
    assert_eq!(back.ids["c1"], without.ids["c1"]);
}

#[test]
fn two_profiles_share_a_rows_id_through_the_install_store() {
    let data = data_dir("profiles");
    let (store, warnings) = Store::load(&data, None, "one", None);
    assert!(warnings.is_empty(), "{warnings:?}");
    let one = build(VEHICLES, 10, &[("a", &["a1"]), ("b", &["b1"])], store);
    one.store.save(&data).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(path(&data)).unwrap()).unwrap();
    assert_eq!(
        saved["rows"]["b/entrants.vehicles.vehicle/b1"]["id"],
        one.ids["b1"]
    );
    assert_eq!(saved["rows"]["b/entrants.vehicles.vehicle/b1"]["from"], "next");
    let (store, _) = Store::load(&data, None, "two", None);
    let two = build(VEHICLES, 10, &[("b", &["b1"])], store);
    assert_eq!(two.ids["b1"], one.ids["b1"]);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_clash_in_one_profile_leaves_another_profiles_ids_alone() {
    let data = data_dir("clash-profile");
    let (store, _) = Store::load(&data, None, "a", None);
    let a = build(VEHICLES, 10, &[("x", &["x1", "x2"]), ("y", &["y1"])], store);
    a.store.save(&data).unwrap();
    let x1 = key("x", VEHICLES, "x1");

    let (store, _) = Store::load(&data, None, "b", None);
    let b = build(
        VEHICLES,
        a.ids["x1"],
        &[("x", &["x1", "x2"]), ("y", &["y1"])],
        store,
    );
    assert_ne!(b.ids["x1"], a.ids["x1"]);
    b.store.save(&data).unwrap();
    let file = saved(&data);
    assert_eq!(file["rows"][&x1]["id"], a.ids["x1"]);
    assert_eq!(file["profiles"]["b"]["rows"][&x1]["id"], b.ids["x1"]);
    assert!(file["profiles"].get("a").is_none());

    let (store, _) = Store::load(&data, None, "a", None);
    let again_a = build(VEHICLES, 10, &[("x", &["x1", "x2"]), ("y", &["y1"])], store);
    assert_eq!(again_a.ids, a.ids);
    assert_eq!(again_a.store.summary(), None);
    again_a.store.save(&data).unwrap();
    assert_eq!(saved(&data), file);

    let (store, _) = Store::load(&data, None, "b", None);
    let again_b = build(
        VEHICLES,
        a.ids["x1"],
        &[("x", &["x1", "x2"]), ("y", &["y1"])],
        store,
    );
    assert_eq!(again_b.ids, b.ids);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_file_from_before_per_profile_ids_loads_and_is_written_back_whole() {
    let data = data_dir("old-format");
    let x1 = key("x", VEHICLES, "x1");
    let doc = serde_json::json!({
        "version": 1,
        "rows": { x1.clone(): { "id": 11, "from": "next" } },
        "seeded": ["default"],
        "recovered": true,
        "aliases": { x1.clone(): [7] },
    });
    put(&path(&data), &doc.to_string());
    let (store, warnings) = Store::load(&data, None, "default", None);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(store.get(&x1), Some(11));
    let built = build(VEHICLES, 10, &[("x", &["x1", "x2"])], store);
    assert_eq!(built.ids["x1"], 11);
    built.store.save(&data).unwrap();
    let file = saved(&data);
    assert_eq!(file["version"], VERSION);
    assert_eq!(file["rows"][&x1], doc["rows"][&x1]);
    assert_eq!(file["aliases"], doc["aliases"]);
    assert_eq!(file["seeded"], doc["seeded"]);
    assert!(file.get("profiles").is_none());
    assert_eq!(file["rows"].as_object().unwrap().len(), 2);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_profiles_rebuilds_keep_its_ids_and_a_move_there_keeps_the_old_id_as_an_alias() {
    let data = data_dir("rebuilds");
    let (store, _) = Store::load(&data, None, "a", None);
    let first = build(VEHICLES, 10, &[("x", &["x1"])], store);
    first.store.save(&data).unwrap();
    let (store, _) = Store::load(&data, None, "b", None);
    let moved = build(VEHICLES, first.ids["x1"], &[("x", &["x1"])], store);
    moved.store.save(&data).unwrap();
    for _ in 0..2 {
        let (store, _) = Store::load(&data, None, "b", None);
        let again = build(VEHICLES, first.ids["x1"], &[("x", &["x1"])], store);
        assert_eq!(again.ids, moved.ids);
        assert_eq!(again.store.summary(), None);
        assert_eq!(
            again.store.id_aliases().tables["vehicles"][&first.ids["x1"]],
            moved.ids["x1"]
        );
        again.store.save(&data).unwrap();
    }
    let (store, _) = Store::load(&data, None, "a", None);
    assert!(store.id_aliases().is_empty());
    std::fs::remove_dir_all(&data).ok();
}
