use super::*;

#[test]
fn rows_the_store_has_no_id_for_take_the_profiles_last_build_ids() {
    let data = data_dir("seed");
    let overlays = data.join("cache/overlay/p");
    let (table, row) = VEHICLES;
    let mut last = db(VEHICLES, 10);
    last.set(&format!("{table}._{row}_instancecount_"), "3", None)
        .unwrap();
    for (i, (label, id)) in [("a1", "40"), ("b1", "41")].iter().enumerate() {
        last.set(&format!("{table}.{row}_{}._metadata_", i + 1), *label, Some(4))
            .unwrap();
        last.set(&format!("{table}.{row}_{}.stat_id", i + 1), *id, Some(0))
            .unwrap();
    }
    let rel = "game_package/db.bin";
    put(&overlays.join("old").join(rel), "");
    std::fs::write(overlays.join("old").join(rel), last.to_bytes()).unwrap();
    assert_eq!(
        last_build(&overlays, "new", rel),
        Some(LastBuild {
            db: Some(overlays.join("old").join(rel)),
            edits: None
        })
    );
    assert_eq!(last_build(&overlays, "old", rel), None);
    let (store, _) = Store::load(&data, None, "p", last_build(&overlays, "new", rel));
    let first = build(
        VEHICLES,
        10,
        &[("b", &["b1"]), ("a", &["a1"]), ("c", &["c1"])],
        store,
    );
    assert_eq!(
        first.ids,
        BTreeMap::from([("a1".into(), 40), ("b1".into(), 41), ("c1".into(), 42)])
    );
    assert!(
        first
            .lines
            .iter()
            .any(|l| l.ends_with("stat_id 41 (the source's is 10; as the last build gave it)")),
        "{:?}",
        first.lines
    );
    assert_eq!(first.store.from[&key("a", VEHICLES, "a1")], IdFrom::Seeded);
    first.store.save(&data).unwrap();
    let seed = |profile: &str| Store::load(&data, None, profile, last_build(&overlays, "new", rel)).0;
    assert_eq!(seed("p").seed.last, None);
    assert!(seed("q").seed.last.is_some());
    let kept = kept_rows(&data, "a").unwrap();
    assert_eq!(
        kept,
        [Kept {
            table: "entrants.vehicles.vehicle".into(),
            label: "a1".into(),
            id: 40,
            from: IdFrom::Seeded
        }]
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_new_profile_seeds_from_another_profiles_last_build() {
    let data = data_dir("other-profile");
    let overlay = data.join("cache/overlay");
    let mut old = table_to(MODES, 20);
    let (table, row) = MODES;
    old.set(&format!("{table}._{row}_instancecount_"), "22", None)
        .unwrap();
    old.set(&format!("{table}.{row}_21._metadata_"), "bdx_2012_mode", Some(4))
        .unwrap();
    old.set(&format!("{table}.{row}_21.stat_id"), "25", Some(0))
        .unwrap();
    let rel = "game_package/db.bin";
    put(&overlay.join("ig10/older").join(rel), "");
    std::fs::write(overlay.join("ig10/older").join(rel), old.to_bytes()).unwrap();
    std::fs::create_dir_all(overlay.join("fresh")).unwrap();
    let last = last_build(&overlay.join("fresh"), "new", rel);
    assert_eq!(
        last.as_ref().and_then(|l| l.db.clone()),
        Some(overlay.join("ig10/older").join(rel))
    );
    let (store, _) = Store::load(&data, None, "fresh", last);
    let built = try_build_on(
        table_to(MODES, 20),
        MODES,
        &[("bydanix", &["bdx_2012_mode"])],
        store,
    )
    .unwrap();
    assert_eq!(built.ids["bdx_2012_mode"], 25);
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.ends_with("stat_id 25 (the source's is 0; as the last build gave it)")),
        "{:?}",
        built.lines
    );
    assert_eq!(
        built.store.from[&key("bydanix", MODES, "bdx_2012_mode")],
        IdFrom::Seeded
    );
    std::fs::remove_dir_all(&data).ok();
}
