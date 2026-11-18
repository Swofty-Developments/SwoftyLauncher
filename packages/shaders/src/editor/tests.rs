use super::*;
use crate::lint::tests::{GOOD, catalog};
use crate::{Completion, TokenKind};

fn at(text: &str) -> (String, usize) {
    let i = text.find('|').expect("cursor");
    (text.replacen('|', "", 1), i)
}

fn comp(text: &str) -> (String, Vec<Completion>) {
    let (t, i) = at(text);
    let c = completions(&t, i, &catalog());
    (t, c)
}

fn labels(c: &[Completion]) -> Vec<&str> {
    c.iter().map(|c| c.label.as_str()).collect()
}

fn kinds(text: &str) -> Vec<(TokenKind, String)> {
    tokens(text)
        .into_iter()
        .map(|t| (t.kind, text[t.range].to_owned()))
        .collect()
}

#[test]
fn tokens_cover_every_construct() {
    let k = kinds("# c\nname = \"A b\" # t\n[grain.x]\namount = [1, -2.5e3]\non = true\n");
    use TokenKind::*;
    let want = [
        (Comment, "# c"),
        (Key, "name"),
        (Punct, "="),
        (String, "\"A b\""),
        (Comment, "# t"),
        (Punct, "["),
        (Header, "grain.x"),
        (Punct, "]"),
        (Key, "amount"),
        (Punct, "="),
        (Punct, "["),
        (Number, "1"),
        (Punct, ","),
        (Number, "-2.5e3"),
        (Punct, "]"),
        (Key, "on"),
        (Punct, "="),
        (Bool, "true"),
    ];
    assert_eq!(k.len(), want.len(), "{k:?}");
    for (g, w) in k.iter().zip(want) {
        assert_eq!((g.0, g.1.as_str()), w);
    }
}

#[test]
fn tokens_handle_broken_text() {
    let k = kinds("[grain\nx = \"open\ny 'z\n");
    assert!(k.contains(&(TokenKind::Header, "grain".into())));
    assert!(k.contains(&(TokenKind::String, "\"open".into())));
    let k = kinds("a = \"q\\\"r\"");
    assert!(k.contains(&(TokenKind::String, "\"q\\\"r\"".into())));
}

#[test]
fn nothing_panics_on_any_prefix_of_any_config() {
    let cat = catalog();
    let texts = [
        GOOD,
        "é😀 = [1,\n[[x]]\n[a.b.c]\n\"\"\"x\n'''y\n= = =\n\u{feff}[x]\r\n#é",
        crate::effect::tests::AMBIENT,
    ];
    for text in texts {
        for (i, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let p = &text[..i];
            let _ = tokens(p);
            let _ = crate::lint(p, &cat);
            let _ = completions(p, p.len(), &cat);
            let _ = hover(p, p.len().saturating_sub(1), &cat);
            for j in 0..=p.len() {
                let _ = completions(p, j, &cat);
                let _ = hover(p, j, &cat);
            }
        }
    }
}

#[test]
fn effect_ids_complete_after_a_bracket() {
    let (t, c) = comp("name = \"a\"\n[|");
    assert_eq!(labels(&c), ["ambient_light", "grain", "bloom_wide"]);
    assert_eq!(c[0].detail, "Ambient Light");
    assert_eq!(c[0].insert, "ambient_light]");
    assert_eq!(c[0].range, t.len()..t.len());
}

#[test]
fn a_partial_effect_word_is_replaced_whole() {
    let (t, c) = comp("[gr|ain]\n");
    assert_eq!(labels(&c), ["grain"]);
    assert_eq!(&t[c[0].range.clone()], "grain");
    assert_eq!(c[0].insert, "grain");
    let (t, c) = comp("[am|");
    assert_eq!(&t[c[0].range.clone()], "am");
    assert_eq!(c[0].insert, "ambient_light]");
    assert!(comp("[grain.|").1.is_empty());
}

#[test]
fn unset_params_and_enabled_complete_inside_a_table() {
    let (t, c) = comp("[grain]\namount = 1\n|");
    assert_eq!(labels(&c), ["size", "enabled"]);
    assert_eq!(c[0].insert, "size = 2");
    assert_eq!(c[0].range, t.len()..t.len());
    assert_eq!(c[1].insert, "enabled = false");
    let (_, c) = comp("[ambient_light]\n|");
    assert_eq!(c[0].detail, "Bloom intensity · default 1.0");
    assert_eq!(c[0].doc, "How bright the bloom is.");
    assert_eq!(c[0].insert, "intensity = 1.0");
    assert_eq!(
        c.iter().find(|c| c.label == "tint").unwrap().insert,
        "tint = [1.0, 1.0, 1.0]"
    );
}

#[test]
fn a_typed_key_prefix_narrows_and_keeps_the_rest_of_the_line() {
    let (t, c) = comp("[ambient_light]\nin|\n");
    assert_eq!(labels(&c), ["intensity"]);
    assert_eq!(&t[c[0].range.clone()], "in");
    let (t, c) = comp("[grain]\nam|ount = 5\n");
    assert_eq!(labels(&c), ["amount"]);
    assert_eq!(&t[c[0].range.clone()], "amount");
    assert_eq!(c[0].insert, "amount");
}

#[test]
fn the_top_level_offers_metadata_keys() {
    let (_, c) = comp("name = \"a\"\n|");
    assert_eq!(labels(&c), ["author", "description"]);
    assert!(comp("[grain]\n#|").1.is_empty());
}

#[test]
fn values_complete_after_equals() {
    let (t, c) = comp("[ambient_light]\nadapt_black_level = |\n");
    assert_eq!(labels(&c), ["off", "low", "high"]);
    assert_eq!(c[1].insert, "\"low\"");
    assert_eq!(c[0].range, t.len() - 1..t.len() - 1);
    let (t, c) = comp("[ambient_light]\nadapt_black_level = \"lo|\"\n");
    assert_eq!(labels(&c), ["low"]);
    assert_eq!(&t[c[0].range.clone()], "\"lo\"");
    let (_, c) = comp("[ambient_light]\nlens = |\n");
    assert_eq!(labels(&c), ["true", "false"]);
    let (_, c) = comp("[ambient_light]\nlens = t|\n");
    assert_eq!(labels(&c), ["true"]);
    let (_, c) = comp("[grain]\namount = |\n");
    assert_eq!(c[0].insert, "0.1");
    let (_, c) = comp("[grain]\nsize = |\n");
    assert_eq!(c[0].insert, "2");
    let (_, c) = comp("[ambient_light]\ntint = |\n");
    assert_eq!(c[0].insert, "[1.0, 1.0, 1.0]");
    assert!(comp("[ambient_light]\nnope = |\n").1.is_empty());
    assert!(comp("name = |\n").1.is_empty());
}

#[test]
fn hover_describes_effects_and_params() {
    let cat = catalog();
    let t = "name = \"a\"\n[ambient_light]\nintensity = 1.5\nenabled = true\n";
    let h = hover(t, t.find("ambient").unwrap() + 2, &cat).unwrap();
    assert!(h.text.starts_with("**Ambient Light** (`ambient_light`)"));
    assert!(
        h.text.contains("Bloom from bright areas.")
            && h.text.contains("- `intensity` Bloom intensity (float)")
    );
    assert_eq!(&t[h.range], "ambient_light");
    let h = hover(t, t.find("intensity").unwrap() + 1, &cat).unwrap();
    assert!(
        h.text.contains("**Bloom intensity**")
            && h.text.contains("Range: 0.0 to 4.0")
            && h.text.contains("Default: `1.0`")
    );
    assert_eq!(&t[h.range], "intensity");
    let h = hover(t, t.find("1.5").unwrap(), &cat).unwrap();
    assert_eq!(&t[h.range.clone()], "1.5");
    assert!(
        hover(t, t.find("enabled").unwrap(), &cat)
            .unwrap()
            .text
            .contains("Turn this instance off")
    );
    assert!(
        hover(t, t.find("name").unwrap(), &cat)
            .unwrap()
            .text
            .contains("shown in the list")
    );
    assert!(hover(t, 0, &cat).is_some());
    let e = "[ambient_light]\nadapt_black_level = \"low\"\n";
    assert!(
        hover(e, e.find("low").unwrap(), &cat)
            .unwrap()
            .text
            .contains("One of: `off`, `low`, `high`")
    );
    assert!(hover("[nope]\nx = 1\n", 2, &cat).is_none());
    assert!(hover("# c\n", 1, &cat).is_none());
}

const RULED: &str = "name = \"a\"\n[grain]\namount = 0.1\n[grain.heavy]\n[[when]]\nname = \"k\"\non = \"car.surface\"\nis = \"kerb\"\n[when.set.grain]\namount = 1\n";

fn with_cursor(extra: &str) -> String {
    format!("{RULED}{extra}")
}

#[test]
fn rule_headers_and_multi_line_arrays_tokenise() {
    let text = "[[when]]\nall = [\n  { on = \"car.speed\", above = 150 }, # c\n]\n[when.set.grain.heavy]\n";
    let k = kinds(text);
    use TokenKind::*;
    assert!(k.contains(&(Header, "when".into())));
    assert!(k.contains(&(Header, "when.set.grain.heavy".into())));
    assert!(k.contains(&(Key, "all".into())));
    assert!(k.contains(&(Key, "on".into())));
    assert!(k.contains(&(Key, "above".into())));
    assert!(k.contains(&(String, "\"car.speed\"".into())));
    assert!(k.contains(&(Number, "150".into())));
    assert!(k.contains(&(Comment, "# c".into())));
    assert!(!k.iter().any(|(kind, t)| *kind == Key && t == "{"));
}

#[test]
fn rule_keys_complete_inside_a_rule_and_skip_the_ones_set() {
    let (_, c) = comp(&with_cursor("[[when]]\non = \"view.replay\"\nfade|"));
    assert_eq!(labels(&c), ["fade_in_ms", "fade_out_ms"]);
    assert_eq!(c[0].insert, "fade_in_ms = 150");
    let (_, c) = comp(&with_cursor("[[when]]\non = \"view.replay\"\n|"));
    let l = labels(&c);
    assert!(l.contains(&"priority") && l.contains(&"all") && !l.contains(&"on"));
}

#[test]
fn signal_ids_complete_for_on_with_the_quoted_token_replaced() {
    let (t, c) = comp(&with_cursor("[[when]]\non = \"car.sp|\"\n"));
    assert_eq!(labels(&c), ["car.speed"]);
    assert_eq!(c[0].insert, "\"car.speed\"");
    assert_eq!(&t[c[0].range.clone()], "\"car.sp\"");
    assert!(!c[0].detail.contains("Preview only"));
    let (_, c) = comp(&with_cursor("[[when]]\non = \"session.we|\"\n"));
    assert_eq!(labels(&c), ["session.weather", "session.wetness"]);
    assert!(c[0].detail.contains("Preview only") && !c[1].detail.contains("Preview only"));
    let (_, c) = comp(&with_cursor("[[when]]\non = |\n"));
    assert_eq!(c.len(), crate::signals::SIGNALS.len());
    let (_, c) = comp(&with_cursor("[[when]]\nall = [{ on = \"view.c|\" }]\n"));
    assert_eq!(labels(&c), ["view.camera", "view.cutscene"]);
}

#[test]
fn is_completes_the_values_of_the_rules_signal() {
    let (_, c) = comp(&with_cursor("[[when]]\non = \"car.surface\"\nis = \"|\"\n"));
    assert_eq!(
        labels(&c),
        ["tarmac", "kerb", "rumble", "grass", "gravel", "sand"]
    );
    let (_, c) = comp(&with_cursor(
        "[[when]]\non = \"car.surface\"\nis = [\"kerb\", \"gr|\"]\n",
    ));
    assert_eq!(labels(&c), ["grass", "gravel"]);
    let (_, c) = comp(&with_cursor(
        "[[when]]\nall = [{ on = \"view.camera\", is = \"ch|\" }]\n",
    ));
    assert_eq!(labels(&c), ["chase_near", "chase_far"]);
    let (_, c) = comp(&with_cursor("[[when]]\non = \"car.speed\"\nis = |\n"));
    assert!(c.is_empty());
}

#[test]
fn inline_table_keys_complete_inside_braces() {
    let (_, c) = comp(&with_cursor("[[when]]\nall = [{ on = \"car.speed\", ab|"));
    assert_eq!(labels(&c), ["above"]);
    let (_, c) = comp(&with_cursor("[[when]]\nall = [{ |"));
    assert_eq!(labels(&c), ["on", "is", "above", "below", "wheels"]);
    let (_, c) = comp(&with_cursor("[[when]]\nall = [\n  { on = \"car.surface\", w|"));
    assert_eq!(labels(&c), ["wheels"]);
}

#[test]
fn set_tables_complete_instances_options_and_values() {
    let (_, c) = comp(&with_cursor("[[when]]\non = \"view.replay\"\n[when.|"));
    assert_eq!(labels(&c), ["set"]);
    let (t, c) = comp(&with_cursor("[[when]]\non = \"view.replay\"\n[when.set.|"));
    assert_eq!(labels(&c), ["grain", "grain.heavy"]);
    assert_eq!(c[0].insert, "grain]");
    assert_eq!(&t[c[0].range.clone()], "");
    let (_, c) = comp(&with_cursor("[[when]]\non = \"view.replay\"\n[when.set.grain.|"));
    assert!(c.is_empty() || c.iter().all(|x| x.label.starts_with("grain")));
    let (_, c) = comp(&with_cursor(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\nam|",
    ));
    assert_eq!(labels(&c), ["amount"]);
    let (_, c) = comp(&with_cursor(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\nenabled = |",
    ));
    assert_eq!(labels(&c), ["false"]);
    let (_, c) = comp(&with_cursor("[[when]]\n|"));
    assert!(labels(&c).contains(&"name"));
    let (_, c) = comp("[[wh|");
    assert_eq!(labels(&c), ["when"]);
    assert_eq!(c[0].insert, "when]]");
}

#[test]
fn hover_describes_signals_and_rule_keys() {
    let t = RULED;
    let h = |needle: &str, nth: usize| {
        let i = t.match_indices(needle).nth(nth).unwrap().0 + 1;
        hover(t, i, &catalog())
    };
    let on = h("\"car.surface\"", 0).unwrap();
    assert!(on.text.contains("**car.surface**") && on.text.contains("One of: `tarmac`"));
    assert!(!on.text.contains("Not read in game yet"));
    assert_eq!(&t[on.range], "\"car.surface\"");
    let key = h("is =", 0).unwrap();
    assert!(key.text.starts_with("**is**"));
    assert!(h("[[when]]", 0).unwrap().text.contains("Rule"));
    let set = h("[when.set.grain]", 0).unwrap();
    assert!(set.text.contains("grain") || set.text.contains("Grain"));
    let opt = h("amount = 1", 0).unwrap();
    assert!(opt.text.contains("amount"));
    let all = "[[when]]\nall = [{ on = \"car.speed\", above = 1 }]\n";
    let i = all.find("car.speed").unwrap() + 1;
    assert!(hover(all, i, &catalog()).unwrap().text.contains("**car.speed**"));
}

#[test]
fn nothing_panics_on_any_prefix_of_a_ruled_config() {
    let text = format!(
        "{RULED}[[when]]\nall = [\n  {{ on = \"car.speed\", above = 150 }},\n]\n[when.set.grain]\nsize = 3\n"
    );
    for i in 0..=text.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        let _ = completions(&text, i, &catalog());
        let _ = hover(&text, i, &catalog());
        let _ = tokens(&text[..i]);
    }
}
