use semver::{Version, VersionReq};

use crate::Result;

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
fn a_manifest_without_a_repository_writes_no_repository_field() {
    let m = AddonManifest::parse(GRID24).unwrap();
    assert!(!serde_json::to_string(&m).unwrap().contains("repository"));
}

#[test]
fn hud_mfd_rows_name_toggle_or_choice_options() {
    let with = |hud: &str| GRID24.replace(r#""operations":"#, &format!(r#""hud": {hud}, "operations":"#));
    let m = AddonManifest::parse(&with(
        r#"{ "mfd": { "title": "LIVERY", "rows": ["team_livery"] } }"#,
    ))
    .unwrap();
    let mfd = m.hud.unwrap().mfd.unwrap();
    assert_eq!(mfd.title.as_deref(), Some("LIVERY"));
    assert_eq!(mfd.rows, ["team_livery"]);
    for (bad, why) in [
        (r#"{ "mfd": { "rows": [] } }"#, "rows must list"),
        (r#"{ "mfd": { "rows": ["missing"] } }"#, "names no option"),
        (
            r#"{ "mfd": { "rows": ["team_livery", "team_livery"] } }"#,
            "listed twice",
        ),
        (
            r#"{ "mfd": { "title": "", "rows": ["team_livery"] } }"#,
            "title must be",
        ),
        (
            r#"{ "mfd": { "rows": ["team_livery"], "page": 2 } }"#,
            "unknown field",
        ),
    ] {
        let e = AddonManifest::parse(&with(bad)).unwrap_err().to_string();
        assert!(e.contains(why), "{bad}: {e}");
    }
    let slider = GRID24
        .replace(
            r#""type": "choice", "default": "titlesponsor","#,
            r#""type": "slider", "default": 1, "min": 0, "max": 2, "step": 1,"#,
        )
        .replace(
            r#""values": ["titlesponsor", { "value": "tier01", "label": "Tier 1" }, "replica"], "#,
            "",
        );
    let e = AddonManifest::parse(&slider.replace(
        r#""operations":"#,
        r#""hud": { "mfd": { "rows": ["team_livery"] } }, "operations":"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("only toggle and choice"), "{e}");
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

#[test]
fn parses_the_saves_flag() {
    let with = |saves: &str| {
        GRID24.replace(
            r#""author": "swofty","#,
            &format!(r#""author": "swofty", {saves}"#),
        )
    };
    let m = AddonManifest::parse(GRID24).unwrap();
    assert_eq!(m.saves, Saves::default());
    assert!(!m.saves.affects);
    assert!(serde_json::to_value(&m).unwrap().get("saves").is_none());
    let m = AddonManifest::parse(&with(r#""saves": { "affects": true },"#)).unwrap();
    assert!(m.saves.affects);
    assert_eq!(m.saves.message(), None);
    let m = AddonManifest::parse(&with(
        r#""saves": { "affects": true, "message": "Retire drivers instead of removing the addon." },"#,
    ))
    .unwrap();
    assert!(m.saves.affects);
    assert_eq!(
        m.saves.message(),
        Some("Retire drivers instead of removing the addon.")
    );
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(json["saves"]["affects"], true);
    assert_eq!(AddonManifest::parse(&json.to_string()).unwrap(), m);
    let m = AddonManifest::parse(&with(r#""saves": {},"#)).unwrap();
    assert!(!m.saves.affects);
    assert_eq!(
        AddonManifest::parse(&with(r#""saves": { "affects": true, "message": " " },"#))
            .unwrap()
            .saves
            .message(),
        None
    );
    for bad in [
        r#""saves": { "affects": true, "extra": 1 },"#,
        r#""saves": { "affects": "yes" },"#,
        r#""saves": true,"#,
    ] {
        assert!(AddonManifest::parse(&with(bad)).is_err(), "{bad}");
    }
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

const LOOKS: &str = r#"{
        "schema_version": 1, "id": "author.looks", "name": "Looks", "version": "1.0.0", "author": "a",
        "category": "shaders", "experimental_features": ["shaders"], "compat": { "game": "f1_25" },
        "options": [
            { "id": "look", "type": "choice", "default": "mono", "values": ["mono", "noir"], "binds": "db" },
            { "id": "strength", "type": "slider", "min": 0, "max": 100, "step": 10, "default": 100, "binds": "db" },
            { "id": "live", "type": "toggle", "default": true, "binds": "native" }
        ],
        "shader": SHADER
    }"#;

fn looks(shader: &str) -> std::result::Result<AddonManifest, String> {
    AddonManifest::parse(&LOOKS.replace("SHADER", shader)).map_err(|e| e.to_string())
}

#[test]
fn a_shader_pack_names_its_lut_and_strength() {
    let m = looks(r#"{ "lut": "looks/{{look}}.cube", "strength": "{{ strength }}" }"#).unwrap();
    assert_eq!(m.category, Some(Category::Shaders));
    assert_eq!(Category::Shaders.label(), "Shaders");
    let s = m.shader.as_ref().unwrap();
    assert_eq!(s.lut, "looks/{{look}}.cube");
    assert_eq!(s.strength, Some(Strength::Option("{{ strength }}".into())));
    assert_eq!(placeholders(&s.lut), ["look"]);
    let m = looks(r#"{ "lut": "looks/mono.CUBE", "strength": 40 }"#).unwrap();
    assert_eq!(m.shader.unwrap().strength, Some(Strength::Percent(40.0)));
    assert_eq!(
        looks(r#"{ "lut": "mono.cube" }"#)
            .unwrap()
            .shader
            .unwrap()
            .strength,
        None
    );
}

#[test]
fn a_postfx_pass_is_checked_before_it_installs() {
    let with = |postfx: &str, features: &str| {
        AddonManifest::parse(&format!(
                r#"{{ "schema_version": 1, "id": "author.fx", "name": "Fx", "version": "1.0.0", "author": "a",
                    "experimental_features": {features}, "compat": {{ "game": "f1_25" }},
                    "options": [{{ "id": "amount", "type": "slider", "min": 0, "max": 100, "step": 1, "default": 50, "binds": "db" }}],
                    "postfx": {postfx} }}"#
            ))
            .map_err(|e| e.to_string())
    };
    let m = with(
        r#"[{ "shader": "fx/grain.hlsl", "options": [0.5, "{{amount}}"], "order": -1 }]"#,
        r#"["postfx"]"#,
    )
    .unwrap();
    assert_eq!(m.postfx[0].label(), "grain");
    assert_eq!(m.postfx[0].order, -1);
    assert_eq!(ExperimentalFeature::Postfx.name(), "postfx");
    let err = |postfx: &str| with(postfx, r#"["postfx"]"#).unwrap_err();
    let at = |msg: &str| format!("invalid manifest: author.fx: postfx[0]: {msg}");
    assert_eq!(
        with(r#"[{ "shader": "fx/grain.hlsl" }]"#, "[]").unwrap_err(),
        at(r#"list "postfx" in experimental_features to use it"#)
    );
    assert_eq!(
        err(r#"[{ "shader": "../grain.hlsl" }]"#),
        at("shader ../grain.hlsl must be a relative path inside the addon")
    );
    assert_eq!(
        err(r#"[{ "shader": "fx/grain.fx" }]"#),
        at("shader fx/grain.fx must be a .hlsl file")
    );
    assert_eq!(
        err(r#"[{ "shader": "fx/g.hlsl", "options": ["{{nope}}"] }]"#),
        at("{{nope}} names no option")
    );
    assert_eq!(
        err(r#"[{ "shader": "fx/g.hlsl", "budget_ms": 0 }]"#),
        at("budget_ms 0 is not above 0 and at most 16")
    );
    assert_eq!(
        err(&format!(
            r#"[{{ "shader": "fx/g.hlsl", "options": [{}] }}]"#,
            vec!["1"; 17].join(",")
        )),
        at("17 options is more than 16")
    );
}

#[test]
fn a_v2_postfx_graph_is_checked_before_it_installs() {
    let with = |extra: &str, postfx: &str| {
        AddonManifest::parse(&format!(
                r#"{{ "schema_version": 1, "id": "author.fx", "name": "Fx", "version": "1.0.0", "author": "a",
                    "experimental_features": ["postfx"], "compat": {{ "game": "f1_25" }},
                    "options": [
                        {{ "id": "amount", "type": "slider", "min": 0, "max": 100, "step": 1, "default": 50, "binds": "native" }},
                        {{ "id": "on", "type": "toggle", "default": true, "binds": "native" }},
                        {{ "id": "key", "type": "choice", "default": "F9", "binds": "native",
                           "values": ["F9", "Page Up", "None"] }},
                        {{ "id": "db_key", "type": "choice", "default": "F9", "binds": "db", "values": ["F9"] }},
                        {{ "id": "bad_key", "type": "choice", "default": "F25", "binds": "native", "values": ["F25"] }}
                    ],
                    {extra}
                    "postfx": {postfx} }}"#
            ))
            .map_err(|e| e.to_string())
    };
    let textures = r#""postfx_textures": [
            { "name": "half", "scale": 0.5 },
            { "name": "accum", "width": 512, "height": 288, "format": "r32f", "persistent": true },
            { "name": "dirt", "file": "tex/dirt.png" }
        ],"#;
    let ok = with(
            &format!(r#"{textures} "postfx_toggle": "{{{{key}}}}","#),
            r#"[
                { "shader": "fx/b.hlsl", "entry": "down", "name": "down", "inputs": ["original", "dirt"], "output": "half" },
                { "shader": "fx/b.hlsl", "entry": "up", "options": ["{{amount}}"], "inputs": ["half", "previous"],
                  "when": { "views": ["cockpit"], "skip": ["paused", "menu"] }, "enabled": "{{on}}" },
                { "shader": "fx/c.hlsl", "raw": true, "threads": [16, 16], "output": "accum" }
            ]"#,
        )
        .unwrap();
    assert_eq!(m_threads(&ok), Some([16, 16]));
    assert_eq!(ok.postfx[1].when.as_ref().unwrap().skip, ["paused", "menu"]);
    assert!(ok.postfx_textures[1].persistent);
    let pass = |p: &str| with(textures, &format!("[{p}]")).unwrap_err();
    let at = |msg: &str| format!("invalid manifest: author.fx: postfx[0]: {msg}");
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "entry": "1x" }"#),
        at(r#"entry "1x" must be an identifier"#)
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "raw": true }"#),
        at("a raw pass needs threads")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "threads": [8, 8] }"#),
        at("threads is for a raw pass")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "raw": true, "threads": [64, 32] }"#),
        at("threads [64, 32] must be 1 to 1024 in total")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "inputs": ["nope"] }"#),
        at("input nope names no postfx_textures entry")
    );
    assert_eq!(
        pass(&format!(
            r#"{{ "shader": "a.hlsl", "inputs": [{}] }}"#,
            [r#""original""#; 9].join(",")
        )),
        at("9 inputs is more than 8")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "output": "dirt" }"#),
        at("output dirt is an image, which is read-only")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "output": "original" }"#),
        at("output original names no postfx_textures entry")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "when": { "views": ["sky"] } }"#),
        at("when.views \"sky\" is not one of cockpit, tcam, tcam_offset, chase_near, chase_far, other")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "when": { "skip": ["lunch"] } }"#),
        at("when.skip \"lunch\" is not one of paused, replay, photo, menu, cutscene, spectating")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "enabled": "{{amount}}" }"#),
        at("enabled option amount must be a toggle")
    );
    assert_eq!(
        pass(r#"{ "shader": "a.hlsl", "enabled": "on" }"#),
        at(r#"enabled "on" must be one {{option}}"#)
    );
    let tex = |t: &str| {
        with(
            &format!(r#""postfx_textures": [{t}],"#),
            r#"[{ "shader": "a.hlsl" }]"#,
        )
        .unwrap_err()
    };
    let tat = |msg: &str| format!("invalid manifest: author.fx: postfx_textures[0]: {msg}");
    assert_eq!(
        tex(r#"{ "name": "Half", "scale": 1 }"#),
        tat(r#"name "Half" must be 1 to 32 of a-z, 0-9 and _"#)
    );
    assert_eq!(
        tex(r#"{ "name": "scene", "scale": 1 }"#),
        tat("name scene is reserved")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "scale": 3 }"#),
        tat("scale 3 is not above 0 and at most 2")
    );
    assert_eq!(
        tex(r#"{ "name": "a" }"#),
        tat("give exactly one of scale, width and height, or file")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "scale": 1, "file": "a.png" }"#),
        tat("give exactly one of scale, width and height, or file")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "width": 10 }"#),
        tat("width and height go together")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "width": 9000, "height": 1 }"#),
        tat("width and height must be 1 to 8192")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "scale": 1, "format": "rgb8" }"#),
        tat(r#"format "rgb8" is not one of rgba8, rgba16f, rg16f, r16f, r32f, rgb10a2, r11g11b10f"#)
    );
    assert_eq!(
        tex(r#"{ "name": "a", "file": "a.jpg" }"#),
        tat("file a.jpg must be a .png file")
    );
    assert_eq!(
        tex(r#"{ "name": "a", "file": "../a.png" }"#),
        tat("file ../a.png must be a relative path inside the addon")
    );
    assert_eq!(
        with(
            r#""postfx_textures": [{ "name": "a", "scale": 1 }, { "name": "a", "scale": 1 }],"#,
            r#"[{ "shader": "a.hlsl" }]"#
        )
        .unwrap_err(),
        "invalid manifest: author.fx: postfx_textures[1]: name a is listed twice"
    );
    let toggle = |t: &str| {
        with(
            &format!(r#""postfx_toggle": "{t}","#),
            r#"[{ "shader": "a.hlsl" }]"#,
        )
        .unwrap_err()
    };
    let gat = |msg: &str| format!("invalid manifest: author.fx: postfx_toggle: {msg}");
    assert_eq!(toggle("{{db_key}}"), gat("option db_key must be a native choice"));
    assert_eq!(toggle("{{amount}}"), gat("option amount must be a native choice"));
    assert_eq!(
        toggle("{{bad_key}}"),
        gat(r#"option bad_key value "F25" is not a key name"#)
    );
    assert_eq!(toggle("{{nope}}"), gat("{{nope}} names no option"));
    assert_eq!(toggle("F9"), gat(r#"value "F9" must be one {{option}}"#));
    assert!(
        ["A", "Z", "0", "F1", "F24", "Insert", "Page Down", "None"]
            .iter()
            .all(|k| postfx_key_ok(k))
    );
    assert!(
        !["a", "F0", "F01", "F25", "AB", "", "Space"]
            .iter()
            .any(|k| postfx_key_ok(k))
    );
}

fn m_threads(m: &AddonManifest) -> Option<[u32; 2]> {
    m.postfx[2].threads
}

#[test]
fn a_shader_pack_is_checked_before_it_installs() {
    let err = |shader: &str| looks(shader).unwrap_err();
    let at = |msg: &str| format!("invalid manifest: author.looks: shader: {msg}");
    assert_eq!(
        err(r#"{ "lut": "mono.png" }"#),
        at("lut mono.png must be a .cube file")
    );
    assert_eq!(
        err(r#"{ "lut": "../mono.cube" }"#),
        at("lut ../mono.cube must be a relative path inside the addon")
    );
    assert_eq!(
        err(r#"{ "lut": "{{look.cube" }"#),
        at("lut {{look.cube has an unclosed {{")
    );
    assert_eq!(
        err(r#"{ "lut": "{{shade}}.cube" }"#),
        at("{{shade}} names no option")
    );
    assert_eq!(
        err(r#"{ "lut": "{{live}}.cube" }"#),
        at("option live must bind db")
    );
    assert_eq!(
        err(r#"{ "lut": "a.cube", "strength": 150 }"#),
        at("strength 150 is not from 0 to 100")
    );
    assert_eq!(
        err(r#"{ "lut": "a.cube", "strength": "{{look}}" }"#),
        at("option look must be a slider")
    );
    assert_eq!(
        err(r#"{ "lut": "a.cube", "strength": "x{{strength}}" }"#),
        at(r#"strength "x{{strength}}" must be a number or one {{option}}"#)
    );
    let no_feature = LOOKS
        .replace(r#""experimental_features": ["shaders"],"#, "")
        .replace("SHADER", r#"{ "lut": "a.cube" }"#);
    assert_eq!(
        AddonManifest::parse(&no_feature).unwrap_err().to_string(),
        at(r#"list "shaders" in experimental_features to use it"#)
    );
}

#[test]
fn rejects_unknown_fields() {
    let bad = GRID24.replace("\"kind\"", "\"kinds\"");
    assert!(AddonManifest::parse(&bad).is_err());
}

#[test]
fn a_career_setup_names_a_route_and_options_of_its_addon() {
    let with = |setup: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "database": ["db/grid24.json"] }}, "career_setup": {setup}"#),
        )
    };
    let m = AddonManifest::parse(&with(
            r#"{ "route": "career_wizard_driver_career_grid", "title": "Grid Career", "options": ["team_livery"] }"#,
        ))
        .unwrap();
    let c = m.career_setup.as_ref().unwrap();
    assert_eq!(c.next, CAREER_SETUP_NEXT);
    assert_eq!(c.screen(), "swofty_setup_career_wizard_driver_career_grid");
    assert_eq!(c.back_route(), "back_career_wizard_driver_career_grid");
    let json = serde_json::to_value(&m).unwrap();
    assert!(json["career_setup"].get("next").is_none());
    assert_eq!(json["career_setup"]["options"][0], "team_livery");
    let m = AddonManifest::parse(&with(
        r#"{ "route": "grid", "options": ["team_livery"], "next": "career_wizard_settings_select" }"#,
    ))
    .unwrap();
    let c = m.career_setup.unwrap();
    assert_eq!(
        (c.title, c.next.as_str()),
        (None, "career_wizard_settings_select")
    );

    let at = |m: &str| format!("invalid manifest: swofty.f1-2026-grid24: career_setup: {m}");
    for (setup, why) in [
        (
            r#"{ "route": "Grid", "options": ["team_livery"] }"#,
            r#"route "Grid" must be [a-z0-9_], at most 48 characters"#.to_owned(),
        ),
        (
            &format!(
                r#"{{ "route": "{}", "options": ["team_livery"] }}"#,
                "r".repeat(49)
            ),
            format!(
                r#"route "{}" must be [a-z0-9_], at most 48 characters"#,
                "r".repeat(49)
            ),
        ),
        (
            r#"{ "route": "grid", "options": [] }"#,
            "options names no option".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["nope"] }"#,
            "option nope is not one of the addon's options".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["team_livery", "team_livery"] }"#,
            "option team_livery is listed twice".to_owned(),
        ),
        (
            r#"{ "route": "grid", "title": " ", "options": ["team_livery"] }"#,
            "title is empty".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["team_livery"], "next": "a-b" }"#,
            r#"next "a-b" must be [a-z0-9_]"#.to_owned(),
        ),
    ] {
        assert_eq!(
            AddonManifest::parse(&with(setup)).unwrap_err().to_string(),
            at(&why),
            "{setup}"
        );
    }
    assert!(
        AddonManifest::parse(&with(
            r#"{ "route": "grid", "options": ["team_livery"], "back": "fe_career" }"#
        ))
        .is_err()
    );
    let hidden = with(r#"{ "route": "grid", "options": ["team_livery"] }"#)
        .replace(r#""binds": "db" }]"#, r#""binds": "db", "hidden": true }]"#);
    let m = AddonManifest::parse(&hidden).unwrap();
    assert!(m.options[0].hidden);
    assert_eq!(m.career_setup.unwrap().options, ["team_livery"]);
}

#[test]
fn a_hub_tab_names_its_template_view_model_and_shown_options() {
    let with = |tab: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "database": ["db/grid24.json"] }}, "hub_tab": {tab}"#),
        )
    };
    let m = AddonManifest::parse(&with(
        r#"{ "name": "progressive", "title": "Progressive Settings", "options": ["team_livery"],
                 "data_group": "f1_career_progressive" }"#,
    ))
    .unwrap();
    let h = m.hub_tab.as_ref().unwrap();
    assert_eq!(h.template(), "swofty_hub_progressive");
    assert_eq!(h.view_model(), "view_model_data.hud_swofty_hub_progressive");
    assert_eq!(
        serde_json::to_value(&m).unwrap()["hub_tab"]["data_group"],
        "f1_career_progressive"
    );
    let at = |m: &str| format!("invalid manifest: swofty.f1-2026-grid24: hub_tab: {m}");
    let tab = |name: &str, title: &str, options: &str, group: &str| {
        format!(
            r#"{{ "name": "{name}", "title": "{title}", "options": {options}, "data_group": "{group}" }}"#
        )
    };
    for (t, why) in [
        (
            tab("Pro", "T", r#"["team_livery"]"#, "g"),
            r#"name "Pro" must be [a-z0-9_], at most 40 characters"#,
        ),
        (tab("p", " ", r#"["team_livery"]"#, "g"), "title is empty"),
        (
            tab("p", "T", r#"["team_livery"]"#, "a.b"),
            r#"data_group "a.b" must be [a-z0-9_]"#,
        ),
        (tab("p", "T", "[]", "g"), "options must name 1 to 6 options"),
        (
            tab("p", "T", r#"["nope"]"#, "g"),
            "option nope is not one of the addon's options",
        ),
        (
            tab("p", "T", r#"["team_livery", "team_livery"]"#, "g"),
            "option team_livery is listed twice",
        ),
    ] {
        assert_eq!(
            AddonManifest::parse(&with(&t)).unwrap_err().to_string(),
            at(why),
            "{t}"
        );
    }
    let hidden = with(&tab("p", "T", r#"["team_livery"]"#, "g"))
        .replace(r#""binds": "db" }]"#, r#""binds": "db", "hidden": true }]"#);
    let m = AddonManifest::parse(&hidden).unwrap();
    assert!(m.options[0].hidden);
    assert_eq!(m.hub_tab.unwrap().options, ["team_livery"]);
}

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

const SHADER_ONLY: &str = r#"{"schema_version":1,"id":"t.look","name":"Look","version":"1.0.0","author":"t",
        "shaders":[{"config":"shaders/look.swfx","force":true}],"effects":["fx/spark.effect.json"]}"#;

#[test]
fn shader_configs_and_effects_parse_and_a_shader_only_addon_is_recognised() {
    let m = AddonManifest::parse(SHADER_ONLY).unwrap();
    assert_eq!(m.shaders[0].stem(), "look");
    assert!(m.shaders[0].force);
    assert!(m.is_shader_only());
    let back = serde_json::to_value(&m).unwrap();
    assert_eq!(back["effects"][0], "fx/spark.effect.json");
    let mixed = SHADER_ONLY.replace(r#""shaders""#, r#""operations":{"erp":["a.erp"]},"shaders""#);
    assert!(!AddonManifest::parse(&mixed).unwrap().is_shader_only());
    let plain = r#"{"schema_version":1,"id":"t.plain","name":"Plain","version":"1.0.0","author":"t"}"#;
    assert!(!AddonManifest::parse(plain).unwrap().is_shader_only());
}

#[test]
fn shader_paths_must_be_relative_and_typed() {
    for bad in [
        SHADER_ONLY.replace("shaders/look.swfx", "../look.swfx"),
        SHADER_ONLY.replace("shaders/look.swfx", "shaders/look.txt"),
        SHADER_ONLY.replace("fx/spark.effect.json", "/fx/spark.effect.json"),
        SHADER_ONLY.replace("fx/spark.effect.json", "fx/spark.json"),
    ] {
        assert!(AddonManifest::parse(&bad).is_err(), "{bad}");
    }
}
