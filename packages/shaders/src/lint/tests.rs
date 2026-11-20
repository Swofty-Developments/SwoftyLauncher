use super::*;
use crate::{Origin, parse_effect};

pub(crate) fn catalog() -> Catalog {
    let ambient = parse_effect(crate::effect::tests::AMBIENT, Origin::Library).unwrap();
    let grain = parse_effect(
        r#"{"id":"grain","name":"Grain","description":"Film grain.","params":[
                {"id":"amount","type":"float","default":0.1,"min":0,"max":1,"label":"Amount","doc":"Strength."},
                {"id":"size","type":"int","default":2,"min":1,"max":8}],
              "passes":[{"name":"p","shader":"grain.hlsl"}]}"#,
        Origin::Library,
    )
    .unwrap();
    let bloom = parse_effect(
        r#"{"id":"bloom_wide","name":"Wide bloom","params":[],"passes":[{"name":"p","shader":"b.hlsl"}]}"#,
        Origin::Addon {
            id: "a.b".into(),
            name: "Addon".into(),
        },
    )
    .unwrap();
    Catalog {
        effects: vec![ambient, grain, bloom],
    }
}

pub(crate) const GOOD: &str = "# Realistic look
name = \"Realistic\"
author = 'Swofty'
description = \"Flatter contrast.\"

[ambient_light]
intensity = 1.5
lens = true
adapt_black_level = \"low\"
tint = [1, 1.5, 0.25]   # r g b
taps = 8

[grain]
amount = 0.2

[grain.heavy]
enabled = false
amount = 1
size = 4
";

fn msgs(text: &str) -> Vec<(Severity, String)> {
    lint(text, &catalog())
        .diagnostics
        .into_iter()
        .map(|d| (d.severity, d.message))
        .collect()
}

fn has(text: &str, sev: Severity, needle: &str) -> bool {
    msgs(text).iter().any(|(s, m)| *s == sev && m.contains(needle))
}

#[test]
fn a_good_config_lints_clean_and_matches_a_real_toml_parse() {
    let l = lint(GOOD, &catalog());
    assert!(l.diagnostics.is_empty(), "{:?}", l.diagnostics);
    let c = l.config.unwrap();
    assert_eq!(c.name, "Realistic");
    assert_eq!(c.author.as_deref(), Some("Swofty"));
    assert_eq!(c.instances.len(), 3);
    assert_eq!(c.instances[2].label.as_deref(), Some("heavy"));
    assert!(!c.instances[2].enabled);
    assert_eq!(c.instances[0].values["tint"], Value::Vector(vec![1.0, 1.5, 0.25]));
    let t: toml::Table = GOOD.parse().unwrap();
    assert_eq!(t["name"].as_str(), Some("Realistic"));
    assert_eq!(t["ambient_light"]["taps"].as_integer(), Some(8));
    assert_eq!(t["grain"]["heavy"]["size"].as_integer(), Some(4));
}

#[test]
fn every_lint_clean_text_is_valid_toml() {
    for text in [
        GOOD,
        "name = \"x\"\n",
        "name=\"x\"\n[grain]\namount=1e-1\n",
        "name = \"t\\u00e9\"\r\n[grain]\r\nsize = 1_0\r\n",
    ] {
        let l = lint(text, &catalog());
        if l.config.is_some() {
            text.parse::<toml::Table>()
                .unwrap_or_else(|e| panic!("{text:?}: {e}"));
        }
    }
}

#[test]
fn the_name_is_required_with_a_fix() {
    let l = lint("[grain]\namount = 1\n", &catalog());
    assert!(l.config.is_none());
    let d = &l.diagnostics[0];
    assert_eq!(d.severity, Severity::Error);
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(
        (fix.range.clone(), fix.replacement.as_str()),
        (0..0, "name = \"Untitled\"\n")
    );
    let mut fixed = String::from("[grain]\namount = 1\n");
    fixed.insert_str(0, &fix.replacement);
    assert!(lint(&fixed, &catalog()).config.is_some());
}

#[test]
fn name_type_and_emptiness_are_checked() {
    assert!(has("name = 3", Severity::Error, "expects a string"));
    assert!(has("name = \"  \"", Severity::Error, "must not be empty"));
    assert!(has(
        "name = \"a\"\nauthor = 1",
        Severity::Error,
        "`author` expects a string"
    ));
    assert!(has("name = \"a\"\nswfx = 2", Severity::Error, "must be 1"));
    assert!(!has("name = \"a\"\nswfx = 1\n[grain]", Severity::Error, "swfx"));
}

#[test]
fn unknown_top_level_keys_warn_with_a_suggestion() {
    let l = lint("name = \"a\"\nauthr = \"x\"\n[grain]\n", &catalog());
    let d = &l.diagnostics[0];
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(d.fix.as_ref().unwrap().replacement, "author");
    assert!(l.config.is_some());
}

#[test]
fn unknown_effects_get_did_you_mean() {
    let text = "name = \"a\"\n[grane]\namount = 1\n";
    let l = lint(text, &catalog());
    assert!(l.config.is_none());
    let d = &l.diagnostics[0];
    assert!(d.message.contains("Unknown effect `grane`") && d.message.contains("addon"));
    assert_eq!(&text[d.range.clone()], "grane");
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(
        (&text[fix.range.clone()], fix.replacement.as_str()),
        ("grane", "grain")
    );
    assert!(
        lint("name = \"a\"\n[qqqqqqqq]\n", &catalog()).diagnostics[0]
            .fix
            .is_none()
    );
}

#[test]
fn unknown_params_get_did_you_mean() {
    let text = "name = \"a\"\n[grain]\namout = 0.5\n";
    let d = &lint(text, &catalog()).diagnostics[0];
    assert_eq!(&text[d.range.clone()], "amout");
    assert_eq!(d.fix.as_ref().unwrap().replacement, "amount");
    assert!(has(
        "name = \"a\"\n[grain]\nenabld = true\n",
        Severity::Error,
        "no param"
    ));
}

#[test]
fn the_same_table_twice_is_an_error() {
    assert!(has(
        "name = \"a\"\n[grain]\n[grain]\n",
        Severity::Error,
        "appears twice"
    ));
    assert!(!has(
        "name = \"a\"\n[grain]\n[grain.b]\n",
        Severity::Error,
        "twice"
    ));
    assert!(has(
        "name = \"a\"\n[ambient_light]\n[ambient_light.lens]\n",
        Severity::Error,
        "also a param"
    ));
}

#[test]
fn types_are_checked() {
    let g = |line: &str| format!("name = \"a\"\n[ambient_light]\n{line}\n");
    assert!(has(&g("intensity = \"x\""), Severity::Error, "expects a number"));
    assert!(has(&g("intensity = true"), Severity::Error, "expects a number"));
    assert!(!has(&g("intensity = 2"), Severity::Error, ""));
    assert!(has(&g("taps = 2.5"), Severity::Error, "whole number"));
    assert!(has(&g("lens = 1"), Severity::Error, "true or false"));
    assert!(has(&g("tint = [1, 2]"), Severity::Error, "expects 3 numbers"));
    assert!(has(&g("tint = 1"), Severity::Error, "array of 3"));
    assert!(has(
        &g("adapt_black_level = 2"),
        Severity::Error,
        "expects one of"
    ));
    assert!(has(&g("enabled = 1"), Severity::Error, "`enabled`"));
}

#[test]
fn enum_values_get_did_you_mean() {
    let text = "name = \"a\"\n[ambient_light]\nadapt_black_level = \"hig\"\n";
    let d = &lint(text, &catalog()).diagnostics[0];
    assert!(d.message.contains("\"off\", \"low\", \"high\""));
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(fix.replacement, "\"high\"");
    assert_eq!(&text[fix.range.clone()], "\"hig\"");
}

#[test]
fn out_of_range_values_warn_clamp_and_offer_a_fix() {
    let text = "name = \"a\"\n[grain]\namount = 5\nsize = 0\n";
    let l = lint(text, &catalog());
    assert!(l.config.is_some());
    let warns: Vec<_> = l
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .collect();
    assert_eq!(warns.len(), 2);
    assert_eq!(warns[0].fix.as_ref().unwrap().title, "Clamp to 1.0");
    assert_eq!(warns[1].fix.as_ref().unwrap().replacement, "1");
    assert_eq!(&text[warns[0].fix.as_ref().unwrap().range.clone()], "5");
    let c = l.config.unwrap();
    assert_eq!(c.instances[0].values["amount"], Value::Number(1.0));
    assert_eq!(c.instances[0].values["size"], Value::Number(1.0));
    let v = lint("name = \"a\"\n[ambient_light]\ntint = [3, 1, -1]\n", &catalog());
    assert_eq!(v.diagnostics.len(), 2);
    assert_eq!(
        v.config.unwrap().instances[0].values["tint"],
        Value::Vector(vec![2.0, 1.0, 0.0])
    );
}

#[test]
fn a_config_with_nothing_enabled_says_so() {
    assert!(has("name = \"a\"\n", Severity::Info, "changes nothing"));
    assert!(has(
        "name = \"a\"\n[grain]\nenabled = false\n",
        Severity::Info,
        "changes nothing"
    ));
    assert!(!has(GOOD, Severity::Info, "nothing"));
    assert!(lint("name = \"a\"\n", &catalog()).config.is_some());
}

#[test]
fn syntax_errors_and_semantic_errors_come_together_sorted() {
    let text = "name = \"a\"\n[grane]\nx = \n[grain]\namout = 1\n";
    let l = lint(text, &catalog());
    assert!(l.errors() >= 3, "{:?}", l.diagnostics);
    let starts: Vec<_> = l.diagnostics.iter().map(|d| d.range.start).collect();
    assert!(starts.windows(2).all(|w| w[0] <= w[1]));
    assert!(l.config.is_none());
}

#[test]
fn positions_are_utf16_columns() {
    let text = "name = \"é😀\"\n[grane]\n";
    let l = lint(text, &catalog());
    let d = l
        .diagnostics
        .iter()
        .find(|d| d.message.contains("Unknown effect"))
        .unwrap();
    assert_eq!((d.start.line, d.start.character), (1, 1));
    assert_eq!((d.end.line, d.end.character), (1, 6));
    let text = "name = \"é😀\"\n[grain]\nsize = 0.5\n";
    let d = &lint(text, &catalog()).diagnostics[0];
    assert_eq!((d.start.line, d.start.character), (2, 7));
}

#[test]
fn instance_spans_cover_header_to_last_line() {
    let text = "name = \"a\"\n[grain]\namount = 1\n\n[ambient_light]\nlens = true\n";
    let c = lint(text, &catalog()).config.unwrap();
    assert_eq!(&text[c.instances[0].span.clone()], "[grain]\namount = 1");
    assert_eq!(&text[c.instances[1].span.clone()], "[ambient_light]\nlens = true");
}

#[test]
fn addon_effects_resolve_like_library_ones() {
    let l = lint("name = \"a\"\n[bloom_wide]\n", &catalog());
    assert!(l.config.is_some(), "{:?}", l.diagnostics);
}
