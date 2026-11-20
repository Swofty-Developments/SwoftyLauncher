use super::*;

#[test]
fn reads_features_and_routes_each_kind() {
    let dir = std::env::temp_dir().join(format!("swofty-features-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(load_features(&dir).unwrap(), Features::default());
    let path = dir.join(FEATURES_FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{ "schema": 1, "features": { "group": "stable", "erp": "experimental", "db": "experimental",
                 "lang": "experimental", "file": "experimental", "audio": "stable", "tex": "soon" } }"#,
    )
    .unwrap();
    let features = load_features(&dir).unwrap();
    assert_eq!(features.stable, BTreeSet::from([Kind::Group]));
    assert_eq!(features.experimental.len(), 4);
    let opt_in = BTreeSet::from([Kind::Erp, Kind::Group]);
    let p = Planner::new(&features, &opt_in);
    assert_eq!(p.route(Kind::Erp), Route::Runtime);
    assert_eq!(p.route(Kind::Db), Route::Build);
    assert_eq!(p.route(Kind::File), Route::Build);
    assert_eq!(p.route(Kind::Lang), Route::Waiting);
    assert_eq!(p.route(Kind::Group), Route::Runtime);
    assert_eq!(p.features(), [Kind::Erp]);
    assert_eq!(
        waiting_lines(
            &[op(
                "a",
                r#"{"op":"lang.set","language":"*","key":"k","value":"v"}"#
            )],
            &features
        ),
        [
            "a: 1 lang op(s) wait; the runtime lang feature is experimental and \"runtime_assets\" in config.json does not turn it on"
        ]
    );
    std::fs::write(
        &path,
        r#"{ "schema": 1, "features": { "group": "stable", "erp": "stable", "db": "stable", "lang": "stable", "file": "experimental" } }"#,
    )
    .unwrap();
    let shipped = load_features(&dir).unwrap();
    let none = BTreeSet::new();
    let p = Planner::new(&shipped, &none);
    for kind in [Kind::Group, Kind::Erp, Kind::Db, Kind::Lang] {
        assert_eq!(p.route(kind), Route::Runtime);
    }
    assert_eq!(p.route(Kind::File), Route::Build);
    assert!(p.features().is_empty());
    let file = BTreeSet::from([Kind::File]);
    let p = Planner::new(&shipped, &file);
    assert_eq!(p.route(Kind::File), Route::Runtime);
    assert_eq!(p.features(), [Kind::File]);
    let forced = BTreeSet::from([Kind::Erp, Kind::Db, Kind::Lang]);
    let p = Planner::new(&shipped, &none).build_only(&forced);
    assert_eq!(p.route(Kind::Erp), Route::Build);
    assert_eq!(p.route(Kind::Db), Route::Build);
    assert_eq!(p.route(Kind::Lang), Route::Runtime);
    assert_eq!(p.route(Kind::Group), Route::Runtime);
    std::fs::remove_dir_all(&dir).ok();
}
