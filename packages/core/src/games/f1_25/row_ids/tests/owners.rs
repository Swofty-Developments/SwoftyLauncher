use super::*;

#[test]
fn save_owners_are_the_addons_with_vehicle_team_engine_or_head_rows() {
    let data = data_dir("owners");
    put(
        &path(&data),
        r#"{"version":2,"rows":{
              "a.engines/entrants.vehicles.vehicle/v1":{"id":1,"from":"next"},
              "b.heads/entrants.animation.driver_heads.driver_head/h":{"id":2,"from":"next"},
              "c.tracks/tracks.Track/t":{"id":3,"from":"next"},
              "d.lists/entrants.entrant_lists.entrant_list/l":{"id":4,"from":"next"}},
              "profiles":{"p":{"rows":{"e.teams/entrants.teams.team/t":{"id":5,"from":"next"}}}}}"#,
    );
    let owners: Vec<String> = save_owners(&data).into_iter().collect();
    assert_eq!(owners, ["a.engines", "b.heads", "e.teams"]);
    assert!(save_owners(&data_dir("none")).is_empty());
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn id_owners_name_the_addon_of_a_kept_id_and_of_an_older_one() {
    let data = data_dir("id-owners");
    put(
        &path(&data),
        r#"{"version":2,"rows":{
              "a.season/entrants.teams.team/t":{"id":501,"from":"next"},
              "a.season/tracks.Track/x":{"id":9,"from":"next"}},
              "aliases":{"a.season/entrants.teams.team/t":[480]},
              "profiles":{"p":{"rows":{"b.cars/entrants.vehicles.vehicle/v":{"id":510,"from":"next"}}}}}"#,
    );
    let owners = id_owners(&data);
    let of = |table: &str, id: i64| owners.of("c", table, id).cloned().unwrap_or_default();
    assert_eq!(
        of("entrants.teams.team", 501),
        BTreeSet::from(["a.season".to_owned()])
    );
    assert_eq!(
        of("entrants.teams.team", 480),
        BTreeSet::from(["a.season".to_owned()])
    );
    assert_eq!(
        of("entrants.vehicles.vehicle", 510),
        BTreeSet::from(["b.cars".to_owned()])
    );
    assert!(of("tracks.Track", 9).is_empty());
    assert_eq!(
        label_owners(&data, "entrants.vehicles.vehicle").get("v"),
        Some(&BTreeSet::from(["b.cars".to_owned()]))
    );
    assert!(!label_owners(&data, "entrants.vehicles.vehicle").contains_key("t"));
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn id_owners_name_the_addon_of_a_registry_id_and_of_a_careers_pin() {
    let data = data_dir("id-owners-registry");
    put(
        &path(&data),
        r#"{"version":2,"rows":{"b.cars/entrants.vehicles.vehicle/v":{"id":545,"from":"next"}}}"#,
    );
    put(
        &reports::path(&data, reports::IDS),
        r#"{"version":1,"ids":{
              "team.identity":{"x.season/t":{"id":536,"since":"2026-11-13"}},
              "vehicle":{"x.season/v":{"id":545},"z.season/w":528,"old.season/o":{"id":825,"pin":529}},
              "driver.identity":{"x.season/d":{"id":255}},
              "driver.head":{"x.season/h":{"id":300}},
              "vehicle.package":{"x.season/p":{"id":600}},
              "helmet":{"x.season/k":{"id":537}}},
              "careers":{"c":{"vehicle":{"y.season/w":528},"team.identity":{"y.season/u":700}}}}"#,
    );
    let owners = id_owners(&data);
    let of = |career: &str, table: &str, id: i64| -> Vec<String> {
        owners
            .of(career, table, id)
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    };
    assert_eq!(of("c", "entrants.teams.team", 536), ["x.season"]);
    assert_eq!(of("c", "entrants.teams.team", 700), ["y.season"]);
    assert_eq!(of("other", "entrants.teams.team", 700), ["y.season"]);
    assert_eq!(of("c", "entrants.vehicles.vehicle", 545), ["b.cars", "x.season"]);
    assert_eq!(of("c", "entrants.vehicles.vehicle", 528), ["y.season"]);
    assert_eq!(of("other", "entrants.vehicles.vehicle", 528), ["z.season"]);
    assert_eq!(of("c", "entrants.vehicles.vehicle", 825), ["old.season"]);
    assert!(of("c", "entrants.vehicles.vehicle", 529).is_empty());
    assert_eq!(of("c", "entrants.drivers.driver", 255), ["x.season"]);
    assert!(of("c", "entrants.drivers.driver", 300).is_empty());
    assert!(of("c", "entrants.vehicles.vehicle", 600).is_empty());
    assert!(of("c", "entrants.teams.team", 537).is_empty());
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn registry_rows_are_the_team_car_and_driver_ids_of_the_records_of_addons_on() {
    let data = data_dir("registry-rows");
    assert!(registry_rows(&data, None, &["x.season".into()]).is_empty());
    put(
        &reports::path(&data, reports::IDS),
        r#"{"version":1,"ids":{
              "team.identity":{"x.season/t":{"id":536},"off.season/t":{"id":537}},
              "vehicle":{"x.season/mercedes":{"id":509,"since":"2026-11-02"},"z.season/w":528,"off.season/v":{"id":530}},
              "driver.identity":{"x.season/d":{"id":255}},
              "driver.head":{"x.season/h":{"id":300}},
              "vehicle.package":{"x.season/p":{"id":600}}},
              "careers":{"c":{"vehicle":{"x.season/mercedes":541,"off.season/v":542}}}}"#,
    );
    let on = ["x.season".to_owned(), "z.season".to_owned()];
    let rows = registry_rows(&data, None, &on);
    let ids = |table: &str| -> Vec<i64> { rows.get(table).into_iter().flatten().copied().collect() };
    assert_eq!(ids("entrants.teams.team"), [536]);
    assert_eq!(ids("entrants.vehicles.vehicle"), [509, 528, 541]);
    assert_eq!(ids("entrants.drivers.driver"), [255]);
    assert_eq!(
        rows.len(),
        3,
        "heads, packages and other registries hold no row a save names"
    );
    assert!(
        registry_rows(&data, None, &[]).is_empty(),
        "an addon that is off adds no row"
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn stock_max_ids_are_the_highest_stat_id_of_each_stock_table() {
    use swofty_formats::gvdb::Record;
    let mut records = vec![Record::new(
        &instance_count_path("entrants.teams", "team"),
        0,
        "3",
    )];
    for (i, id) in [7, 159, 12].into_iter().enumerate() {
        records.push(Record::new(
            &format!("entrants.teams.team_{i}.stat_id"),
            0,
            id.to_string(),
        ));
    }
    let max = stock_max_ids(&Gvdb::new(records));
    assert_eq!(max.get("entrants.teams.team"), Some(&159));
    assert_eq!(max.get("entrants.vehicles.vehicle"), None);
}
