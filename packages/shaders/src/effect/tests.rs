use super::*;

pub(crate) const AMBIENT: &str = r#"{
  "id": "ambient_light", "name": "Ambient Light", "description": "Bloom from bright areas.",
  "params": [
    { "id": "intensity", "type": "float", "default": 1.0, "min": 0, "max": 4, "step": 0.01, "label": "Bloom intensity", "doc": "How bright the bloom is." },
    { "id": "lens", "type": "bool", "default": false, "label": "Lens flare", "doc": "Adds a lens flare." },
    { "id": "adapt_black_level", "type": "enum", "values": ["off", "low", "high"], "default": "off", "label": "Adaptation black level", "doc": "x" },
    { "id": "tint", "type": "color3", "default": [1, 1, 1], "min": 0, "max": 2, "label": "Tint", "doc": "y" },
    { "id": "taps", "type": "int", "default": 4, "min": 1, "max": 16 },
    { "id": "offset", "type": "float2", "default": [0, 0] }
  ],
  "textures": [
    { "name": "half", "scale": 0.5, "format": "rgba16f" },
    { "name": "adapt", "width": 1, "height": 1, "format": "rgba16f", "persistent": true },
    { "name": "dirt", "file": "ambient_light_dirt.png" }
  ],
  "passes": [
    { "name": "bright", "shader": "ambient_light.hlsl", "entry": "bright", "output": "half" },
    { "name": "apply", "shader": "ambient_light.hlsl", "entry": "apply", "inputs": ["half", "adapt", "dirt"] }
  ]
}"#;

pub(crate) fn ambient() -> Effect {
    parse_effect(AMBIENT, Origin::Library).unwrap()
}

#[test]
fn the_design_example_parses_with_slots_in_order() {
    let e = ambient();
    assert_eq!(e.slots, 1 + 1 + 1 + 3 + 1 + 2);
    let slots: Vec<_> = e.params.iter().map(|p| (p.id.as_str(), p.slot)).collect();
    assert_eq!(
        slots,
        [
            ("intensity", 0),
            ("lens", 1),
            ("adapt_black_level", 2),
            ("tint", 3),
            ("taps", 6),
            ("offset", 7)
        ]
    );
    assert_eq!(e.params[0].label, "Bloom intensity");
    assert_eq!(e.params[4].label, "taps");
    assert_eq!(e.passes.len(), 2);
}

#[test]
fn the_accessors_match_the_golden_text() {
    let golden = "#pragma once
#include \"swofty/postfx.hlsli\"

float param_intensity() { return option(0); }
bool param_lens() { return option(1) > 0.5; }
uint param_adapt_black_level() { return uint(round(option(2))); }
#define ADAPT_BLACK_LEVEL_OFF 0
#define ADAPT_BLACK_LEVEL_LOW 1
#define ADAPT_BLACK_LEVEL_HIGH 2
float3 param_tint() { return float3(option(3), option(4), option(5)); }
int param_taps() { return int(round(option(6))); }
float2 param_offset() { return float2(option(7), option(8)); }
";
    assert_eq!(params_hlsl(&ambient()), golden);
}

fn err(json: &str) -> String {
    parse_effect(json, Origin::Library).unwrap_err()
}

fn with(params: &str, rest: &str) -> String {
    format!(r#"{{"id":"e","params":[{params}],{rest}}}"#)
}

const PASS: &str = r#""passes":[{"name":"a","shader":"e.hlsl"}]"#;

#[test]
fn bad_ids_and_duplicates_are_refused() {
    assert!(err(r#"{"id":"Bad","passes":[]}"#).contains("must match"));
    assert!(err(&with(r#"{"id":"A","type":"float"}"#, PASS)).contains("must match"));
    assert!(
        err(&with(
            r#"{"id":"a","type":"float"},{"id":"a","type":"bool"}"#,
            PASS
        ))
        .contains("twice")
    );
    assert!(err(&with(r#"{"id":"a","type":"quux"}"#, PASS)).contains("unknown type"));
}

#[test]
fn defaults_must_fit_their_kind() {
    assert!(err(&with(r#"{"id":"a","type":"float","default":9,"max":4}"#, PASS)).contains("outside"));
    assert!(err(&with(r#"{"id":"a","type":"float","default":"x"}"#, PASS)).contains("number"));
    assert!(err(&with(r#"{"id":"a","type":"int","default":1.5}"#, PASS)).contains("whole"));
    assert!(err(&with(r#"{"id":"a","type":"bool","default":1}"#, PASS)).contains("true or false"));
    assert!(
        err(&with(
            r#"{"id":"a","type":"enum","values":["x"],"default":"y"}"#,
            PASS
        ))
        .contains("not one of")
    );
    assert!(err(&with(r#"{"id":"a","type":"enum"}"#, PASS)).contains("needs values"));
    assert!(err(&with(r#"{"id":"a","type":"color3","default":[1,2]}"#, PASS)).contains("array of 3"));
    assert!(err(&with(r#"{"id":"a","type":"float","min":3,"max":1}"#, PASS)).contains("above max"));
}

#[test]
fn enum_constants_must_not_collide() {
    let p = r#"{"id":"a","type":"enum","values":["b_c"]},{"id":"a_b","type":"enum","values":["c"]}"#;
    assert!(err(&with(p, PASS)).contains("defined twice"));
    let p = r#"{"id":"a","type":"enum","values":["x-y","x_y"]}"#;
    assert!(err(&with(p, PASS)).contains("repeats"));
}

#[test]
fn more_than_32_slots_is_refused_with_the_count() {
    let p: Vec<String> = (0..9)
        .map(|i| format!(r#"{{"id":"c{i}","type":"color4"}}"#))
        .collect();
    let e = err(&with(&p.join(","), PASS));
    assert!(e.contains("36 param slots") && e.contains("32"), "{e}");
    let ok: Vec<String> = (0..8)
        .map(|i| format!(r#"{{"id":"c{i}","type":"color4"}}"#))
        .collect();
    assert_eq!(
        parse_effect(&with(&ok.join(","), PASS), Origin::Library)
            .unwrap()
            .slots,
        32
    );
}

#[test]
fn textures_and_passes_are_cross_checked() {
    let t = r#""textures":[{"name":"half","scale":0.5}],"#;
    let bad_in = format!(r#"{t}"passes":[{{"name":"a","shader":"s","inputs":["nope"]}}]"#);
    assert!(err(&with("", &bad_in)).contains("not a declared texture"));
    let bad_out = format!(r#"{t}"passes":[{{"name":"a","shader":"s","output":"nope"}}]"#);
    assert!(err(&with("", &bad_out)).contains("output `nope`"));
    let ok = format!(
        r#"{t}"passes":[{{"name":"a","shader":"s","inputs":["original","previous","half"],"output":"half"}}]"#
    );
    assert!(parse_effect(&with("", &ok), Origin::Library).is_ok());
    let dup = r#""textures":[{"name":"h"},{"name":"h"}],"passes":[{"name":"a","shader":"s"}]"#;
    assert!(err(&with("", dup)).contains("declared twice"));
    let reserved = r#""textures":[{"name":"original"}],"passes":[{"name":"a","shader":"s"}]"#;
    assert!(err(&with("", reserved)).contains("reserved"));
    assert!(err(&with("", r#""passes":[]"#)).contains("no passes"));
    let twice = r#""passes":[{"name":"a","shader":"s"},{"name":"a","shader":"s"}]"#;
    assert!(err(&with("", twice)).contains("declared twice"));
    let file_out =
        r#""textures":[{"name":"d","file":"d.png"}],"passes":[{"name":"a","shader":"s","output":"d"}]"#;
    assert!(err(&with("", file_out)).contains("file texture"));
}

#[test]
fn mips_are_checked_and_clamped() {
    let passes = r#""passes":[{"name":"a","shader":"s"}]"#;
    let tex = |t: &str| with("", &format!(r#""textures":[{t}],{passes}"#));
    let ok = parse_effect(
        &tex(r#"{"name":"m","width":64,"height":32,"mips":7}"#),
        Origin::Library,
    )
    .unwrap();
    assert_eq!(ok.textures[0].mips, Some(7));
    assert!(ok.warnings.is_empty());
    let big = parse_effect(
        &tex(r#"{"name":"m","width":64,"height":32,"mips":9}"#),
        Origin::Library,
    )
    .unwrap();
    assert_eq!(big.textures[0].mips, Some(7));
    assert_eq!(big.warnings.len(), 1);
    let huge = parse_effect(&tex(r#"{"name":"m","scale":0.5,"mips":20}"#), Origin::Library).unwrap();
    assert_eq!(huge.textures[0].mips, Some(12));
    assert!(err(&tex(r#"{"name":"m","mips":0}"#)).contains("at least 1"));
    assert!(err(&tex(r#"{"name":"m","file":"a.png","mips":3}"#)).contains("no mips"));
}

#[test]
fn mip_levels_follow_the_size() {
    assert_eq!(mip_levels(4, 4, 3), 3);
    assert_eq!(mip_levels(4, 4, 9), 3);
    assert_eq!(mip_levels(1920, 1080, 12), 11);
    assert_eq!(mip_levels(1, 1, 5), 1);
    assert_eq!(mip_levels(64, 32, 0), 1);
    assert_eq!(mip_levels(1 << 20, 1, 99), 12);
}

#[test]
fn broken_json_names_the_problem() {
    assert!(err("{").starts_with("effect:"));
}

#[test]
fn name_falls_back_to_the_id() {
    let e = parse_effect(
        &with("", PASS),
        Origin::Addon {
            id: "a.b".into(),
            name: "AB".into(),
        },
    )
    .unwrap();
    assert_eq!(e.name, "e");
    assert_eq!(
        e.origin,
        Origin::Addon {
            id: "a.b".into(),
            name: "AB".into()
        }
    );
}
