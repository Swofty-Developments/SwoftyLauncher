use semver::{Version, VersionReq};

use super::*;

#[test]
fn compat_platforms_list_the_tested_stores() {
    let with = |value: &str| {
        let compat = format!(r#""compat": {{ "game": "f1_25", "platforms": {value} }},"#);
        let (start, _) = GRID24.split_once(r#""compat":"#).unwrap();
        let (_, rest) = GRID24.split_once(r#""requires":"#).unwrap();
        AddonManifest::parse(&format!(r#"{start}{compat} "requires":{rest}"#))
    };
    let m = with(r#"["ea", "steam", "ea"]"#).unwrap();
    assert_eq!(m.compat.tested_platforms(), vec![Platform::Steam, Platform::Ea]);
    assert!(with("[]").unwrap().compat.tested_platforms().is_empty());
    let err = with(r#"["steam", "gog"]"#).unwrap_err().to_string();
    assert!(err.contains(r#"compat.platforms "gog" is not one of"#), "{err}");
    assert!(with(r#""steam""#).is_err());
    let none = AddonManifest::parse(GRID24).unwrap();
    assert!(none.compat.tested_platforms().is_empty());
    assert!(!serde_json::to_string(&none).unwrap().contains("platforms"));
}

#[test]
fn an_addon_list_takes_one_addon_or_several() {
    let one = AddonNeed::parse_list(&serde_json::json!("track.istanbul_park")).unwrap();
    assert_eq!(one.len(), 1);
    let two =
        AddonNeed::parse_list(&serde_json::json!(["track.a", { "id": "track.b", "version": "^2" }])).unwrap();
    assert_eq!(two[1].version, VersionReq::parse("^2").unwrap());
    assert_eq!(
        AddonNeed::parse_list(&serde_json::json!([])).unwrap_err(),
        "lists no addon"
    );
    assert_eq!(
        AddonNeed::parse_list(&serde_json::json!(["track.a", "nope"])).unwrap_err(),
        r#"[1]: "nope" is not an addon id"#
    );
}

#[test]
fn a_manifest_without_compat_is_for_f1_25_on_any_build_and_launcher() {
    let m = AddonManifest::parse(
            r#"{"schema_version":1,"id":"t.nocompat","name":"No compat","version":"1.0.1","author":"t","kind":["erp"]}"#,
        )
        .unwrap();
    assert_eq!(m.compat, Compat::default());
    assert_eq!(m.compat.game, crate::GAME_ID);
    assert!(m.compat.accepts_build(25251167));
    assert!(m.compat.accepts_launcher(&Version::new(1, 7, 3)));
}
