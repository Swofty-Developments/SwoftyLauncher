use super::values::parse_number;
use super::*;

fn errs(text: &str) -> Vec<String> {
    parse(text).errors.into_iter().map(|e| e.message).collect()
}

#[test]
fn numbers_follow_the_toml_grammar() {
    for ok in [
        "0", "1", "-3", "+4", "1_000", "0.5", "-0.25", "1e3", "1.5E-2", "6.02e+23", "1_0.2_5", "-0",
    ] {
        assert!(parse_number(ok).is_some(), "{ok}");
        ok.parse::<toml::Value>().ok();
    }
    for bad in [
        "", "01", "1__0", "_1", "1_", ".5", "5.", "1e", "1.e3", "0x10", "inf", "nan", "--1", "1.2.3", "1e+",
    ] {
        assert!(parse_number(bad).is_none(), "{bad}");
    }
    assert_eq!(parse_number("12"), Some((12.0, true)));
    assert_eq!(parse_number("1e3"), Some((1000.0, false)));
    assert_eq!(parse_number("1_000.5"), Some((1000.5, false)));
}

#[test]
fn a_good_file_parses_to_tables_and_entries() {
    let text = "# hi\nname = \"A\"\n\n[bloom]\nintensity = 1.5 # c\ntint = [1, 2.0, 3,]\non = true\n[bloom.wide]\nx = 'lit'\n";
    let d = parse(text);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert_eq!(d.top.len(), 1);
    assert_eq!(d.tables.len(), 2);
    assert_eq!(d.tables[0].entries.len(), 3);
    assert_eq!(d.tables[1].label.as_deref(), Some("wide"));
    let e = &d.tables[0].entries[0];
    assert_eq!(&text[e.key_range.clone()], "intensity");
    assert_eq!(&text[e.value.range.clone()], "1.5");
    assert_eq!(
        &text[d.tables[0].span.clone()],
        "[bloom]\nintensity = 1.5 # c\ntint = [1, 2.0, 3,]\non = true"
    );
    match &d.tables[0].entries[1].value.lit {
        Lit::Array(a) => assert_eq!(a.len(), 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn strings_decode_escapes() {
    let d = parse("name = \"a\\tb\\u00e9\\\"q\\\\\"\n");
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert_eq!(d.top[0].value.lit, Lit::Str("a\tbé\"q\\".into()));
    assert!(errs("n = \"a\\qb\"").iter().any(|m| m.contains("Unknown escape")));
    assert!(errs("n = \"abc").iter().any(|m| m.contains("not closed")));
    assert!(errs("n = \"a\\u12\"").iter().any(|m| m.contains("unicode")));
}

#[test]
fn unsupported_toml_is_named() {
    assert!(errs("n = \"\"\"x\"\"\"")[0].contains("multi-line string"));
    assert!(errs("n = '''x'''")[0].contains("multi-line string"));
    assert!(errs("n = {a = 1}")[0].contains("inline table"));
    assert!(errs("n = 1979-05-27")[0].contains("Dates"));
    assert!(errs("n = [1,\n 2")[0].contains("not closed"));
    assert!(errs("[[x]]")[0].contains("array of tables"));
    assert!(errs("a.b = 1")[0].contains("dotted key"));
    assert!(errs("\"a\" = 1")[0].contains("quoted key"));
    assert!(errs("[a.b.c]")[0].contains("[effect.label]"));
    assert!(errs("[\"a\"]")[0].contains("quoted table"));
    assert!(errs("n = [1, \"a\"]")[0].contains("one kind"));
    assert!(errs("n = [true]")[0].contains("numbers, strings"));
    assert!(errs("n = yes")[0].contains("not a valid value"));
    assert!(errs("n = 0x10")[0].contains("decimal"));
}

#[test]
fn it_keeps_going_after_a_bad_line() {
    let d = parse("a = \nb = 2\nc 3\nd = 4 5\n[x\ne = 1\n");
    let msgs: Vec<_> = d.errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(msgs.len(), 4, "{msgs:?}");
    assert!(d.top.iter().any(|e| e.key == "b"));
    assert!(d.top.iter().any(|e| e.key == "d"));
    assert!(d.tables[0].bad);
}

#[test]
fn duplicate_keys_are_errors_in_their_table() {
    assert!(errs("a = 1\na = 2")[0].contains("set twice"));
    assert!(errs("[x]\na = 1\n[y]\na = 2").is_empty());
}

#[test]
fn crlf_and_indent_are_accepted() {
    let d = parse("name = \"a\"\r\n  [x]\r\n\ty = 1\r\n");
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert_eq!(d.tables[0].entries[0].key, "y");
}

#[test]
fn trailing_garbage_is_flagged() {
    assert!(errs("a = 1 2")[0].contains("after the value"));
    assert!(errs("[x] y")[0].contains("after the value"));
    assert!(errs("a = [1] 2")[0].contains("after the value"));
}

#[test]
fn rules_collect_their_entries_and_set_tables() {
    let text = "name = \"a\"\n[grain]\nsize = 1\n[[when]]\nname = \"k\"\non = \"car.speed\" # c\n[when.set.grain]\namount = 0.5\n[when.set.grain.heavy]\nsize = 2\n[[when]]\non = \"view.replay\"\n";
    let d = parse(text);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert_eq!(d.tables.len(), 1);
    assert_eq!(d.rules.len(), 2);
    let r = &d.rules[0];
    assert_eq!(r.entries.len(), 2);
    assert_eq!(r.sets.len(), 2);
    assert_eq!(r.sets[0].effect, "grain");
    assert_eq!(r.sets[1].label.as_deref(), Some("heavy"));
    assert_eq!(r.sets[1].entries[0].key, "size");
    assert_eq!(&text[r.header.clone()], "[[when]]");
    assert!(text[r.span.clone()].ends_with("size = 2"));
    assert_eq!(d.rules[1].entries.len(), 1);
}

#[test]
fn arrays_hold_strings_tables_and_span_lines() {
    let text = "[[when]]\nis = [\"grass\", 'gravel']\nall = [\n  { on = \"car.surface\", is = \"kerb\" }, # a\n  { on = \"car.speed\", above = 150 },\n]\npriority = 1\n";
    let d = parse(text);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    let r = &d.rules[0];
    assert_eq!(r.entries.len(), 3);
    match &r.entries[0].value.lit {
        Lit::Strs(s) => assert_eq!((s[0].text.as_str(), s[1].text.as_str()), ("grass", "gravel")),
        other => panic!("{other:?}"),
    }
    match &r.entries[1].value.lit {
        Lit::Tables(t) => {
            assert_eq!(t.len(), 2);
            assert_eq!(t[1].entries[1].key, "above");
            assert_eq!(t[1].entries[1].value.lit, Lit::Int(150));
            assert_eq!(&text[t[0].entries[0].key_range.clone()], "on");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(r.entries[2].key, "priority");
}

#[test]
fn rule_header_problems_are_errors_and_parsing_goes_on() {
    let before = errs("[when.set.grain]\nx = 1\n");
    assert!(before[0].contains("must come after a `[[when]]`"));
    assert!(errs("[[when]]\n[when.foo.grain]\n")[0].contains("[when.set.<effect>]"));
    assert!(errs("[[when\n")[0].contains("missing its `]]`"));
    assert!(errs("[[nope]]\n")[0].contains("only `[[when]]`"));
    let d = parse("[[when]]\non = \n[[when]]\nname = \"x\"\n");
    assert_eq!(d.rules.len(), 2);
    assert_eq!(d.errors.len(), 1);
    assert_eq!(d.rules[1].entries.len(), 1);
}

#[test]
fn an_unclosed_array_does_not_swallow_the_next_table() {
    let d = parse("[[when]]\nall = [\n  { on = \"a\" },\n[grain]\nsize = 1\n");
    assert!(
        d.errors.iter().any(|e| e.message.contains("not closed")),
        "{:?}",
        d.errors
    );
    assert_eq!(d.tables.len(), 1);
    assert_eq!(d.tables[0].entries.len(), 1);
}
