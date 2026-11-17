use super::*;

#[test]
fn a_stat_id_an_addon_sets_yields_to_the_registry_record_a_career_holds_it_for() {
    let data = data_dir("explicit-registry");
    career_holds_247(&data);
    let (store, _) = Store::load(&data, None, "p", None);
    let rows: &[&str] = &["aarav_amin@247", "alex_gillon@246"];
    let first = build(DRIVERS, 100, &[("mdm", rows)], store);
    let got = first.ids["aarav_amin"];
    assert_ne!(got, 247, "{:?}", first.lines);
    assert_ne!(got, 246);
    assert_eq!(first.ids["alex_gillon"], 246);
    assert!(
        first.lines.iter().any(|l| l.contains(
            "stat_id 247 is kept for sw.2022/sw22_vettel, a record of the core's registry; stat_id"
        )),
        "{:?}",
        first.lines
    );
    assert_eq!(first.store.get(&key("mdm", DRIVERS, "aarav_amin")), Some(got));
    let mut kept = first.store;
    let loaded = Store::load(&data, None, "p", None).0;
    kept.pinned = loaded.pinned;
    kept.career_held = loaded.career_held;
    let again = build(DRIVERS, 100, &[("mdm", rows)], kept);
    assert_eq!(again.ids["aarav_amin"], got);
    assert_eq!(again.ids["alex_gillon"], 246);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_stat_id_an_addon_sets_stays_on_its_own_record_and_where_saves_hold_no_ids() {
    let data = data_dir("explicit-own");
    career_holds_247(&data);
    let (store, _) = Store::load(&data, None, "p", None);
    let own = build(DRIVERS, 100, &[("sw.2022", &["other@247"])], store);
    assert_eq!(own.ids["other"], 247);
    let (store, _) = Store::load(&data, None, "p", None);
    let same_row = build(DRIVERS, 100, &[("grid", &["sw22_vettel@247"])], store);
    assert_eq!(same_row.ids["sw22_vettel"], 247);
    let (store, _) = Store::load(&data, None, "p", None);
    let package = build(PACKAGES, 100, &[("mdm", &["pkg@247"])], store);
    assert_eq!(package.ids["pkg"], 247);
    let free = build(DRIVERS, 100, &[("mdm", &["aarav_amin@247"])], Store::default());
    assert_eq!(free.ids["aarav_amin"], 247);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn ids_the_core_registry_holds_are_passed_over() {
    let data = data_dir("registry");
    let slot = 11;
    put(
        &reports::path(&data, reports::IDS),
        &format!(
            r#"{{"version":1,"ids":{{"vehicle":{{"mod.x/car":{{"id":{slot},"since":"2026-10-29"}},"mod.x/old":12}}}},
                "careers":{{"dc-1":{{"vehicle":{{"mod.y/car":13}}}}}}}}"#
        ),
    );
    let (store, _) = Store::load(&data, None, "p", None);
    let first = build(VEHICLES, 10, &[("a", &["a1"])], store);
    let want = 14;
    assert_eq!(first.ids["a1"], want);
    let mut kept = Store::default();
    kept.put(&key("a", VEHICLES, "a1"), 13, IdFrom::Next);
    let loaded = Store::load(&data, None, "p", None).0;
    kept.pinned = loaded.pinned;
    kept.career_held = loaded.career_held;
    let moved = build(VEHICLES, 10, &[("a", &["a1"])], kept);
    assert_eq!(moved.ids["a1"], want);
    assert!(
        moved.lines.iter().any(|l| l.ends_with(&format!(
            "stat_id {want} (the source's is 10; 13, kept for it, is kept for another row or held by a record of the core's registry)"
        ))),
        "{:?}",
        moved.lines
    );
    assert_eq!(moved.store.get(&key("a", VEHICLES, "a1")), Some(want));
    assert_eq!(moved.store.from[&key("a", VEHICLES, "a1")], IdFrom::Moved);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_full_table_takes_an_id_ids_json_holds_for_an_addon_the_build_leaves_out_even_one_a_career_listed() {
    let data = data_dir("registry-full");
    let records: Vec<String> = (1..=254).map(|id| format!(r#""gone.x/pkg_{id}":{id}"#)).collect();
    put(
        &reports::path(&data, reports::IDS),
        &format!(
            r#"{{"version":1,"ids":{{"vehicle.package":{{{}}}}},
                "careers":{{"dc-1":{{"vehicle.package":{{"gone.x/pkg_1":1}}}}}}}}"#,
            records.join(",")
        ),
    );
    let (store, _) = Store::load(&data, None, "p", None);
    let first = build(PACKAGES, 0, &[("a", &["a1", "a2"])], store);
    assert_eq!((first.ids["a1"], first.ids["a2"]), (1, 2));
    assert_eq!(first.store.from[&key("a", PACKAGES, "a1")], IdFrom::Reused);
    assert!(
        first.lines.iter().any(|l| l.contains("stat_id 1 (the source's is 0; no other id up to 254 is free; reused from gone.x/pkg_1, which this build leaves out)")),
        "{:?}",
        first.lines
    );
    first.store.save(&data).unwrap();
    let (store, _) = Store::load(&data, None, "p", None);
    let again = build(PACKAGES, 0, &[("a", &["a1", "a2"])], store);
    assert_eq!(again.ids, first.ids);
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn careers_hold_ids_of_teams_vehicles_and_drivers_but_not_vehicle_packages() {
    let doc = serde_json::json!({
        "careers": {"c": {
            "vehicle": {"x/v": 7},
            "team.identity": {"x/t": 8},
            "driver.identity": {"x/d": {"id": 9}},
            "vehicle.package": {"x/p": 10}
        }}
    });
    let held = career_ids(&doc);
    assert_eq!(held["entrants.vehicles.vehicle"], HashSet::from([7]));
    assert_eq!(held["entrants.teams.team"], HashSet::from([8]));
    assert_eq!(held["entrants.drivers.driver"], HashSet::from([9]));
    assert!(!held.contains_key("entrants.vehicles.vehicle_packages.vehicle_package"));
    let pinned = registry_ids(&doc);
    assert_eq!(pinned["entrants.vehicles.vehicle"][&7], "x/v");
    assert!(!pinned.contains_key("entrants.vehicles.vehicle_packages.vehicle_package"));
}

#[test]
fn package_ids_a_career_held_return_when_an_addon_provides_fewer_records() {
    let data = data_dir("career-packages-shrink");
    let held: Vec<String> = (1..=89).map(|id| format!(r#""cs/pkg_{id}":{id}"#)).collect();
    put(
        &reports::path(&data, reports::IDS),
        &format!(
            r#"{{"version":1,"ids":{{}},
                "careers":{{"f1_career_progressive.driver_career:new1":{{"vehicle.package":{{{}}}}}}}}}"#,
            held.join(",")
        ),
    );
    let (store, _) = Store::load(&data, None, "p", None);
    assert!(store.career_held.is_empty());
    let labels: Vec<String> = (1..=10).map(|i| format!("p{i}")).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let built = build(PACKAGES, 0, &[("cs", &refs)], store);
    assert_eq!(
        built.ids.values().copied().collect::<BTreeSet<_>>(),
        (1..=10).collect()
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_full_table_leaves_the_ids_ids_json_holds_for_addons_the_build_has() {
    let data = data_dir("registry-full-building");
    let records: Vec<String> = (1..=254).map(|id| format!(r#""a/pkg_{id}":{id}"#)).collect();
    put(
        &reports::path(&data, reports::IDS),
        &format!(
            r#"{{"version":1,"ids":{{"vehicle.package":{{{}}}}}}}"#,
            records.join(",")
        ),
    );
    let (store, _) = Store::load(&data, None, "p", None);
    let e = try_build(PACKAGES, 0, &[("a", &["a1"])], store).err().unwrap();
    assert!(e.to_string().contains("has no free stat_id up to 254"), "{e}");
    std::fs::remove_dir_all(&data).ok();
}
