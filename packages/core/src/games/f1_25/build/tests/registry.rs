use super::*;

#[test]
fn only_a_core_with_the_rows_feature_takes_the_replaced_operations_of_addons_with_data_files() {
    let root = std::env::temp_dir().join(format!("swofty-replaced-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let features = root.join(crate::planner::FEATURES_FILE);
    std::fs::create_dir_all(features.parent().unwrap()).unwrap();
    let std = |id: &str, data: bool, registry: Option<&str>| crate::stdlib::deps::Std {
        id: id.into(),
        name: id.into(),
        version: semver::Version::new(1, 0, 0),
        registry: registry.map(|r| crate::stdlib::manifest::RegistryManifest::parse(r).unwrap()),
        lock: None,
        data_files: match data {
            true => vec!["registry/content.json".into()],
            false => Vec::new(),
        },
        records: Vec::new(),
        files: Default::default(),
        unreadable: Vec::new(),
        older_tracks: Vec::new(),
    };
    let replaces = r#"{ "replaced_operations": ["db/records.json"] }"#;
    let std16 = BTreeMap::from([
        ("a.season".to_owned(), std("a.season", true, Some(replaces))),
        ("b.nodata".to_owned(), std("b.nodata", false, Some(replaces))),
        (
            "c.plain".to_owned(),
            std("c.plain", true, Some(r#"{ "requires": [] }"#)),
        ),
        ("d.old".to_owned(), std("d.old", true, None)),
    ]);
    assert!(replaced_operations(&std16, None).is_empty(), "no loader");
    std::fs::write(&features, r#"{ "schema": 1, "features": { "db": "stable" } }"#).unwrap();
    assert!(
        replaced_operations(&std16, Some(&root)).is_empty(),
        "a core without the rows feature"
    );
    std::fs::write(
        &features,
        r#"{ "schema": 1, "features": { "db": "stable", "registry_rows": "stable" } }"#,
    )
    .unwrap();
    let r = replaced_operations(&std16, Some(&root));
    assert_eq!(r.keys().copied().collect::<Vec<_>>(), ["a.season"]);
    assert!(r["a.season"].replaces("db/records.json") && !r["a.season"].replaces("db/season.json"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn registry_records_turn_the_early_stage_on_whatever_the_database_route() {
    let std = |id: &str, data: bool, lock: Option<&str>, registry: bool| crate::stdlib::deps::Std {
        id: id.into(),
        name: format!("{id} name"),
        version: semver::Version::new(1, 0, 0),
        registry: registry.then(|| crate::stdlib::manifest::RegistryManifest::parse("{}").unwrap()),
        lock: lock.map(|l| crate::stdlib::lock::Lock::parse(l).unwrap()),
        data_files: match data {
            true => vec!["registry/teams.json".into()],
            false => Vec::new(),
        },
        records: Vec::new(),
        files: Default::default(),
        unreadable: Vec::new(),
        older_tracks: Vec::new(),
    };
    let registers = r#"{ "provides": { "team.identity": [ { "id": "dll.teams/hrt" } ] } }"#;
    let std16 = BTreeMap::from([
        ("a.data".to_owned(), std("a.data", true, None, true)),
        ("b.code".to_owned(), std("b.code", false, Some(registers), false)),
        ("c.needs".to_owned(), std("c.needs", false, Some("{}"), true)),
        ("d.off".to_owned(), std("d.off", true, None, false)),
    ]);
    let order: Vec<String> = ["swofty.core", "c.needs", "b.code", "a.data", "e.old"]
        .map(str::to_owned)
        .to_vec();
    let on = registering_addons(&order, &std16);
    assert_eq!(on, ["b.code name", "a.data name"]);
    assert_eq!(
        early_stage_line(&on),
        "registry: the core loads at the early stage for the registry records of b.code name, a.data name"
    );
    assert!(
        registering_addons(&order[..2], &std16).is_empty(),
        "a registry.json or a lock without registrations leaves it off"
    );
}

#[test]
fn the_tables_a_records_file_adds_rows_to_are_the_add_row_tables() {
    let root = std::env::temp_dir().join(format!("swofty-added-tables-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("records.json");
    std::fs::write(
        &file,
        r#"{ "schema_version": 1, "ops": [
             { "op": "set", "select": "a.b[_metadata_=x]", "field": "f", "value": "v" },
             { "op": "add-row", "table": "entrants.entrant_lists", "from": "entrants.entrant_lists.entrant_list[_metadata_=f1]", "set": {} },
             { "op": "add-row", "table": "entrants.drivers", "from": "entrants.drivers.driver[_metadata_=d]", "set": {} },
             { "op": "add-row", "table": "entrants.drivers", "from": "entrants.drivers.driver[_metadata_=d]", "set": {} } ] }"#,
    )
    .unwrap();
    assert_eq!(
        added_tables(&file).into_iter().collect::<Vec<_>>(),
        ["entrants.drivers", "entrants.entrant_lists"]
    );
    assert!(added_tables(&root.join("gone.json")).is_empty());
    std::fs::remove_dir_all(&root).ok();
}
