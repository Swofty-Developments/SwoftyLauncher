use super::*;

#[test]
fn operations_hud_lists_record_files_inside_the_addon() {
    let with = |hud: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "hud": {hud} }}"#),
        )
    };
    let m = AddonManifest::parse(&with(r#"["registry/hud.json"]"#)).unwrap();
    assert_eq!(m.operations.hud, ["registry/hud.json"]);
    let json = serde_json::to_string(&m.operations).unwrap();
    assert!(json.contains(r#""hud":["registry/hud.json"]"#), "{json}");
    assert!(AddonManifest::parse(&with(r#"["../hud.json"]"#)).is_err());
    assert!(
        AddonManifest::parse(&with(r#"[]"#))
            .unwrap()
            .operations
            .hud
            .is_empty()
    );
}

#[test]
fn parses_early_native_plugins() {
    let with = |native: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "native": [{native}] }}"#),
        )
    };
    let m = AddonManifest::parse(&with(
        r#"{ "dll": "native/a.dll", "early": true }, { "dll": "native/b.dll" }"#,
    ))
    .unwrap();
    let n = &m.operations.native;
    assert!(n[0].early && !n[1].early);
    let json = serde_json::to_value(&m.operations).unwrap();
    assert_eq!(json["native"][0]["early"], true);
    assert!(json["native"][1].get("early").is_none());
    let m = AddonManifest::parse(&with(
        r#"{ "dll": "native/a.dll", "files": ["lang/", "erp/a.bin"] }"#,
    ))
    .unwrap();
    assert_eq!(m.operations.native[0].files, ["lang/", "erp/a.bin"]);
    assert!(json["native"][1].get("files").is_none());
    assert!(AddonManifest::parse(&with(r#"{ "dll": "native/a.dll", "files": ["../x"] }"#)).is_err());
}

#[test]
fn parses_and_checks_screens() {
    let with = |screens: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "screens": [{screens}] }}"#),
        )
    };
    let m = AddonManifest::parse(&with(
            r#"{ "name": "grid_page", "file": "ui/grid_page.ui", "flow": "flow_options", "anchor": "settings_container", "view_model": true },
               { "name": "grid_info", "file": "ui/grid_info.ui" }"#,
        ))
        .unwrap();
    let s = &m.operations.screens;
    assert_eq!(
        (s[0].flow.as_str(), s[0].anchor.as_deref(), s[0].view_model),
        ("flow_options", Some("settings_container"), true)
    );
    assert_eq!(
        (s[1].flow.as_str(), s[1].anchor.as_deref(), s[1].view_model),
        ("flow_frontend", None, false)
    );
    let m = AddonManifest::parse(&with(
            r#"{ "name": "grid_options", "page": "options", "flow": "flow_options", "anchor": "settings_container" }"#,
        ))
        .unwrap();
    let s = &m.operations.screens[0];
    assert_eq!((s.page.as_deref(), s.file.as_str()), (Some("options"), ""));
    for bad in [
        r#"{ "name": "a" }"#,
        r#"{ "name": "a", "page": "about" }"#,
        r#"{ "name": "a", "file": "../a.ui" }"#,
        r#"{ "name": "Bad", "file": "ui/a.ui" }"#,
        r#"{ "name": "a", "file": "ui/a.ui", "anchor": "x.y" }"#,
        r#"{ "name": "a", "file": "ui/a.ui" }, { "name": "a", "file": "ui/b.ui" }"#,
        r#"{ "name": "a", "file": "ui/a.ui", "extra": 1 }"#,
    ] {
        assert!(AddonManifest::parse(&with(bad)).is_err(), "{bad}");
    }
}
