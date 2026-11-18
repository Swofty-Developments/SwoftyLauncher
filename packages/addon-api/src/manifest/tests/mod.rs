mod compat;
mod hud;
mod kind;
mod launcher;
mod operations;
mod options;
mod postfx;
mod shader;

use semver::{Version, VersionReq};

use super::*;

const GRID24: &str = r#"{
      "schema_version": 1,
      "id": "swofty.f1-2026-grid24",
      "name": "F1 2026 - 24-Car Grid",
      "version": "1.0.0",
      "author": "swofty",
      "kind": ["database"],
      "compat": { "game": "f1_25", "game_builds": [">=25251167"], "launcher": ">=0.1.0", "seasons": ["f1_2026"] },
      "requires": [{ "id": "swofty.core", "version": ">=0.1.0" }],
      "conflicts": [{ "id": "*", "touches": "db:entrants.entrant_lists.entrant_list_34.*" }],
      "load_after": ["swofty.track.*"],
      "options": [{ "id": "team_livery", "type": "choice", "default": "titlesponsor",
                    "values": ["titlesponsor", { "value": "tier01", "label": "Tier 1" }, "replica"], "binds": "db" }],
      "operations": { "database": ["db/grid24.json"] }
    }"#;

#[test]
fn source_url_is_the_repository_when_it_is_https() {
    let with = |value: &str| {
        let field = format!(r#""repository": {value}, "operations":"#);
        AddonManifest::parse(&GRID24.replace(r#""operations":"#, &field)).unwrap()
    };
    let m = with(r#""https://github.com/me/my-addon""#);
    assert_eq!(m.source_url(), Some("https://github.com/me/my-addon"));
    for not_a_link in [r#""http://example.com/a""#, r#""javascript:alert(1)""#, r#""  ""#] {
        assert_eq!(with(not_a_link).source_url(), None, "{not_a_link}");
    }
    assert_eq!(AddonManifest::parse(GRID24).unwrap().source_url(), None);
}

#[test]
fn a_manifest_without_a_repository_writes_no_repository_field() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert!(!serde_json::to_string(&m).unwrap().contains("repository"));
}

#[test]
fn parses_design_example() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert_eq!(m.version, Version::new(1, 0, 0));
    assert!(m.compat.accepts_build(25_251_167));
    assert!(!m.compat.accepts_build(25_000_000));
    assert!(m.compat.accepts_launcher(&Version::new(1, 0, 9)));
    let newer = Compat {
        launcher: Some(VersionReq::parse(">=1.0.9").unwrap()),
        ..m.compat.clone()
    };
    assert!(newer.accepts_launcher(&Version::new(1, 0, 9)));
    assert!(!newer.accepts_launcher(&Version::new(1, 0, 8)));
    assert_eq!(m.options[0].binds, Binds::Db);
}

#[test]
fn rejects_bad_ids_and_paths() {
    assert!(!valid_id("Grid24"));
    assert!(!valid_id("single"));
    assert!(valid_id("author.mod-name"));
    assert!(!relative_inside("../escape.json"));
    assert!(!relative_inside("/abs.json"));
    assert!(relative_inside("db/ops.json"));
    let bad = GRID24.replace("db/grid24.json", "../grid24.json");
    assert!(AddonManifest::parse(&bad).is_err());
}

#[test]
fn rejects_unknown_fields() {
    let bad = GRID24.replace("\"kind\"", "\"kinds\"");
    assert!(AddonManifest::parse(&bad).is_err());
}
