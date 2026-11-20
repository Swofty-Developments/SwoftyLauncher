use semver::VersionReq;

use super::*;
use crate::Result;

#[test]
fn an_option_can_be_hidden_from_addon_settings() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert!(!m.options[0].hidden);
    assert!(
        serde_json::to_value(&m).unwrap()["options"][0]
            .get("hidden")
            .is_none()
    );
    let m =
        AddonManifest::parse(&GRID24.replace(r#""binds": "db" }]"#, r#""binds": "db", "hidden": true }]"#))
            .unwrap();
    assert!(m.options[0].hidden);
    assert_eq!(serde_json::to_value(&m).unwrap()["options"][0]["hidden"], true);
}

fn with_toggle(option: &str) -> Result<AddonManifest> {
    AddonManifest::parse(&GRID24.replace(r#""binds": "db" }]"#, &format!(r#""binds": "db" }}, {option}]"#)))
}

#[test]
fn a_toggle_needs_addons_by_id_or_with_a_version_range() {
    let m = with_toggle(
        r#"{ "id": "istanbul_park", "type": "toggle", "default": true, "binds": "db",
                 "needs": ["track.istanbul_park", { "id": "swofty.std", "version": ">=1.8.0" }] }"#,
    )
    .unwrap();
    let needs = &m.options[1].needs;
    assert_eq!(needs[0].id, "track.istanbul_park");
    assert_eq!(needs[0].version, VersionReq::STAR);
    assert_eq!(needs[1].to_string(), "swofty.std >=1.8.0");
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(
        json["options"][1]["needs"],
        serde_json::json!(["track.istanbul_park", { "id": "swofty.std", "version": ">=1.8.0" }])
    );
    assert_eq!(AddonManifest::parse(&json.to_string()).unwrap(), m);
    assert!(json["options"][0].get("needs").is_none());
}

#[test]
fn a_bad_need_is_a_manifest_error() {
    let err = |needs: &str| {
        with_toggle(&format!(
                r#"{{ "id": "istanbul_park", "type": "toggle", "default": false, "binds": "db", "needs": {needs} }}"#
            ))
            .unwrap_err()
            .to_string()
    };
    assert!(err(r#"["Istanbul Park"]"#).contains(r#""Istanbul Park" is not an addon id"#));
    assert!(
        err(r#"[{ "id": "track.istanbul_park", "versoin": "1" }]"#).contains(r#""versoin" is not a key"#)
    );
    assert!(err(r#"[{ "id": "track.istanbul_park", "version": "soon" }]"#).contains(r#"version "soon""#));
    assert!(err(r#"[{ "version": "1" }]"#).contains("an addon needs its id"));
    assert!(err("[3]").contains("3 is not an addon id"));
    assert!(
        err(r#"["swofty.f1-2026-grid24"]"#).contains("option istanbul_park: needs names the addon itself")
    );
    let choice = AddonManifest::parse(&GRID24.replace(
        r#""binds": "db" }]"#,
        r#""binds": "db", "needs": ["track.istanbul_park"] }]"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(
        choice.contains("option team_livery: needs is only for a toggle"),
        "{choice}"
    );
    let reserved = with_toggle(r#"{ "id": "addon", "type": "toggle", "default": false, "binds": "db" }"#)
        .unwrap_err()
        .to_string();
    assert!(
        reserved.contains("option addon: the id addon is reserved"),
        "{reserved}"
    );
}
