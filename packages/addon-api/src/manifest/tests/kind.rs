use super::*;

#[test]
fn kind_takes_text_beside_the_older_values() {
    assert_eq!(AddonManifest::parse(GRID24).unwrap().kind, [Kind::Database]);
    let m = AddonManifest::parse(&GRID24.replace(
        r#""kind": ["database"]"#,
        r#""kind": ["database", "erp", "text", "ui", "files", "native", "simplified"]"#,
    ))
    .unwrap();
    assert_eq!(m.kind[2], Kind::Text);
    assert_eq!(m.kind[6], Kind::Simplified);
    assert_eq!(serde_json::to_value(&m).unwrap()["kind"][2], "text");
    assert_eq!(serde_json::to_value(&m).unwrap()["kind"][6], "simplified");
    assert!(AddonManifest::parse(&GRID24.replace(r#""kind": ["database"]"#, r#""kind": ["lang"]"#)).is_err());
}

#[test]
fn category_and_origin_are_optional_and_round_trip() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert_eq!((m.category, &m.origin), (None, &None));
    let json = serde_json::to_value(&m).unwrap();
    assert!(json.get("category").is_none() && json.get("origin").is_none());
    let with = GRID24.replace(
            r#""author": "swofty","#,
            r#""author": "swofty", "category": "helmets",
               "origin": { "tool": "teamsimplified.mods", "tool_version": "2.0.0", "file": "grid.zip", "sha256": "ab" },"#,
        );
    let m = AddonManifest::parse(&with).unwrap();
    assert_eq!(m.category, Some(Category::Helmets));
    assert_eq!(m.category.unwrap().label(), "Helmets");
    let origin = m.origin.as_ref().unwrap();
    assert_eq!(
        (origin.tool.as_str(), origin.file.as_deref()),
        ("teamsimplified.mods", Some("grid.zip"))
    );
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(json["category"], "helmets");
    assert_eq!(AddonManifest::parse(&json.to_string()).unwrap(), m);
    let bare = with.replace(
        r#", "tool_version": "2.0.0", "file": "grid.zip", "sha256": "ab""#,
        "",
    );
    let json = serde_json::to_value(AddonManifest::parse(&bare).unwrap()).unwrap();
    assert_eq!(
        json["origin"],
        serde_json::json!({ "tool": "teamsimplified.mods" })
    );
    for c in Category::ALL {
        let v = serde_json::to_value(c).unwrap();
        assert_eq!(serde_json::from_value::<Category>(v).unwrap(), c);
    }
    let bad = GRID24.replace(
        r#""author": "swofty","#,
        r#""author": "swofty", "origin": { "tool": "x", "extra": 1 },"#,
    );
    assert!(AddonManifest::parse(&bad).is_err(), "{bad}");
}

#[test]
fn earlier_categories_read_as_the_nearest_or_none() {
    let read = |c: &str| {
        let with = GRID24.replace(
            r#""author": "swofty","#,
            &format!(r#""author": "swofty", "category": {c},"#),
        );
        AddonManifest::parse(&with).unwrap().category
    };
    assert_eq!(read(r#""liveries""#), Some(Category::Cars));
    assert_eq!(read(r#""tracks""#), Some(Category::Tracks));
    assert_eq!(read("null"), None);
    assert_eq!(read(r#""maps""#), None);
    assert_eq!(read(r#""career""#), Some(Category::Career));
    assert_eq!(read(r#""ai""#), Some(Category::Ai));
    assert_eq!(Category::Ai.label(), "AI");
    for c in Category::RETIRED {
        assert_eq!(read(&format!("{c:?}")), None, "{c}");
    }
    let m = AddonManifest::parse(&GRID24.replace(
        r#""author": "swofty","#,
        r#""author": "swofty", "category": "liveries","#,
    ))
    .unwrap();
    assert_eq!(serde_json::to_value(&m).unwrap()["category"], "cars");
}

#[test]
fn experimental_features_name_the_core_engines_an_addon_turns_on() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert!(m.experimental_features.is_empty());
    assert!(
        serde_json::to_value(&m)
            .unwrap()
            .get("experimental_features")
            .is_none()
    );
    let with = |f: &str| {
        GRID24.replace(
            r#""author": "swofty","#,
            &format!(r#""author": "swofty", "experimental_features": {f},"#),
        )
    };
    let m = AddonManifest::parse(&with(r#"["overlay"]"#)).unwrap();
    assert_eq!(m.experimental_features, [ExperimentalFeature::Overlay]);
    let f = m.experimental_features[0];
    assert_eq!((f.name(), f.label()), ("overlay", "Overlay"));
    assert_eq!(
        serde_json::to_value(&m).unwrap()["experimental_features"][0],
        "overlay"
    );
    assert!(AddonManifest::parse(&with(r#"["hud"]"#)).is_err());
    let m = AddonManifest::parse(&with(r#"["shaders"]"#)).unwrap();
    let f = m.experimental_features[0];
    assert_eq!((f.name(), f.label()), ("shaders", "Shaders"));
}
