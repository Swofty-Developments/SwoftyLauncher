use super::*;

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
                  "when": { "views": ["cockpit", "trackside", "reverse_pod"], "skip": ["paused", "menu"] }, "enabled": "{{on}}" },
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
        at(
            "when.views \"sky\" is not one of cockpit, tcam, tcam_offset, chase_near, chase_far, other, trackside, reverse_pod"
        )
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
