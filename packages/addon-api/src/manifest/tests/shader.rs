use super::*;

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
