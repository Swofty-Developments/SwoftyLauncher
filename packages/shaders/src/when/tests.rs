use crate::lint::tests::catalog;
use crate::{Severity, lint};

const HEAD: &str = "name = \"a\"\n[grain]\namount = 0.1\n[ambient_light]\nintensity = 1\n";

fn run(rule: &str) -> crate::Lint {
    lint(&format!("{HEAD}{rule}"), &catalog())
}

fn msgs(rule: &str) -> Vec<(Severity, String)> {
    run(rule)
        .diagnostics
        .into_iter()
        .map(|d| (d.severity, d.message))
        .collect()
}

fn only(rule: &str, sev: Severity, message: &str) {
    let m = msgs(rule);
    assert!(
        m.iter().any(|(s, t)| *s == sev && t == message),
        "{rule:?} -> {m:?}"
    );
}

fn clean(rule: &str) {
    let l = run(rule);
    assert!(l.diagnostics.is_empty(), "{rule:?} -> {:?}", l.diagnostics);
    assert!(l.config.is_some());
}

#[test]
fn a_full_rule_lints_clean_and_fills_defaults() {
    let text = format!(
        "{HEAD}[[when]]\nname = \"k\"\non = \"car.surface\"\nis = [\"kerb\", \"grass\"]\nmin_ms = 40\npriority = 5\n[when.set.grain]\namount = 0.5\nsize = 3\n[when.set.ambient_light]\nenabled = false\nlens = true\n"
    );
    let l = lint(&text, &catalog());
    assert!(l.diagnostics.is_empty(), "{:?}", l.diagnostics);
    let r = &l.config.unwrap().rules[0];
    assert_eq!(r.name.as_deref(), Some("k"));
    assert_eq!(
        (r.min_ms, r.hold_ms, r.fade_in_ms, r.fade_out_ms, r.priority),
        (40.0, 0.0, 150.0, 300.0, 5)
    );
    assert!(!r.event);
    assert_eq!(r.sets.len(), 2);
    assert_eq!(r.sets[0].instance, 0);
    assert!(r.sets[1].off);
    assert_eq!(&text[r.span.clone()][..8], "[[when]]");
}

#[test]
fn every_condition_kind_lints_clean() {
    let set = "[when.set.grain]\namount = 0.5\n";
    clean(&format!("[[when]]\non = \"view.replay\"\n{set}"));
    clean(&format!("[[when]]\non = \"session.start\"\n{set}"));
    for body in ["above = 100\nbelow = 200", "from = 250\nto = 320"] {
        clean(&format!("[[when]]\non = \"car.speed\"\n{body}\n{set}"));
    }
    let m = msgs(&format!(
        "[[when]]\non = \"car.impact\"\nabove = 3.0\nfor_ms = 500\n{set}"
    ));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].0, Severity::Warning);
    let m = msgs(&format!(
        "[[when]]\nall = [\n  {{ on = \"car.surface\", is = \"kerb\" }},\n  {{ on = \"view.paused\" }},\n]\n{set}"
    ));
    assert!(m.is_empty(), "{m:?}");
    clean(&format!("[[when]]\non = \"session.lap\"\n{set}"));
}

#[test]
fn signal_typos_get_a_suggestion_and_a_fix() {
    let text = format!("{HEAD}[[when]]\non = \"car.surfce\"\nis = \"kerb\"\n[when.set.grain]\namount = 1\n");
    let l = lint(&text, &catalog());
    let d = &l.diagnostics[0];
    assert_eq!(
        d.message,
        "unknown signal `car.surfce`; did you mean `car.surface`?"
    );
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(fix.replacement, "\"car.surface\"");
    assert_eq!(&text[fix.range.clone()], "\"car.surfce\"");
    only(
        "[[when]]\non = \"zzzzzzzz\"\n",
        Severity::Error,
        "unknown signal `zzzzzzzz`",
    );
}

#[test]
fn is_checks_the_signals_names() {
    let s = "[when.set.grain]\namount = 1\n";
    only(
        &format!("[[when]]\non = \"car.surface\"\nis = \"curb\"\n{s}"),
        Severity::Error,
        "`is` needs a value of car.surface: tarmac, kerb, rumble, grass, gravel, sand",
    );
    let l = run(&format!("[[when]]\non = \"car.surface\"\nis = \"curb\"\n{s}"));
    assert_eq!(
        l.diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .unwrap()
            .fix
            .as_ref()
            .unwrap()
            .replacement,
        "\"kerb\""
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\nis = \"x\"\n{s}"),
        Severity::Error,
        "`is` needs a name signal; car.speed is a number",
    );
    only(
        &format!("[[when]]\non = \"car.surface\"\n{s}"),
        Severity::Error,
        "car.surface is a name; add `is = \"...\"` with tarmac, kerb, rumble, grass, gravel, sand",
    );
}

#[test]
fn number_and_event_conditions_are_typed() {
    let s = "[when.set.grain]\namount = 1\n";
    only(
        &format!("[[when]]\non = \"car.surface\"\nabove = 3\n{s}"),
        Severity::Error,
        "`above` needs a number signal; car.surface is a name",
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\nfor_ms = 5\nabove = 3\n{s}"),
        Severity::Error,
        "`for_ms` is for events; car.speed is a number",
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\n{s}"),
        Severity::Error,
        "car.speed is a number; add `above`, `below`, or `from` and `to`",
    );
    only(
        &format!("[[when]]\non = \"session.lap\"\nabove = 3\n{s}"),
        Severity::Error,
        "`above` needs a number signal; session.lap is an event with no magnitude",
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\nfrom = 320\nto = 250\n{s}"),
        Severity::Error,
        "`from` must be below `to`",
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\nfrom = 3\n{s}"),
        Severity::Error,
        "`from` needs `to` too",
    );
    only(
        &format!("[[when]]\non = \"car.speed\"\nabove = 9\nbelow = 3\n{s}"),
        Severity::Error,
        "`above` must be below `below`",
    );
    only(
        &format!("[[when]]\non = \"car.rpm\"\nfrom = 0\nto = 1\nis = \"x\"\n{s}"),
        Severity::Error,
        "A rule has one kind of condition; `from` and `is` can't be used together",
    );
}

#[test]
fn all_and_wheels_follow_the_contract() {
    let s = "[when.set.grain]\namount = 1\n";
    only(
        &format!("[[when]]\nall = [{{ on = \"car.impact\" }}]\n{s}"),
        Severity::Error,
        "car.impact is an event; events can't go inside `all`",
    );
    only(
        &format!("[[when]]\nall = [{{ on = \"car.speed\", from = 1, to = 2 }}]\n{s}"),
        Severity::Error,
        "A ramp (`from`/`to`) can't go inside `all`",
    );
    only(
        &format!("[[when]]\non = \"view.replay\"\nall = [{{ on = \"view.menu\" }}]\n{s}"),
        Severity::Error,
        "`on` and `all` can't be used together; name each signal inside `all`",
    );
    only(
        &format!("[[when]]\nall = []\n{s}"),
        Severity::Error,
        "`all` needs at least one condition",
    );
    only(
        &format!("[[when]]\n{s}"),
        Severity::Error,
        "A rule needs `on = \"<signal>\"` or `all = [...]`",
    );
    only(
        &format!("[[when]]\non = \"view.camera\"\nis = \"cockpit\"\nwheels = 2\n{s}"),
        Severity::Error,
        "`wheels` is for car.surface; view.camera has none",
    );
    only(
        &format!("[[when]]\non = \"car.surface\"\nis = \"kerb\"\nwheels = 5\n{s}"),
        Severity::Error,
        "`wheels` must be a whole number from 1 to 4",
    );
    let l = run(&format!(
        "[[when]]\non = \"car.surface\"\nis = [\"kerb\", \"rumble\"]\nwheels = 2\n{s}"
    ));
    let r = &l.config.as_ref().unwrap().rules[0];
    assert!(matches!(&r.when[0].op, crate::CondOp::Is { values, wheels: 2 } if values.len() == 2));
}

#[test]
fn timing_priority_and_unknown_keys_are_checked() {
    let s = "[when.set.grain]\namount = 1\n";
    only(
        &format!("[[when]]\non = \"view.replay\"\nmin_ms = 70000\n{s}"),
        Severity::Error,
        "`min_ms` must be a number from 0 to 60000",
    );
    only(
        &format!("[[when]]\non = \"view.replay\"\nhold_ms = -1\n{s}"),
        Severity::Error,
        "`hold_ms` must be a number from 0 to 60000",
    );
    only(
        &format!("[[when]]\non = \"view.replay\"\npriority = 101\n{s}"),
        Severity::Error,
        "`priority` must be a whole number from -100 to 100",
    );
    only(
        &format!("[[when]]\non = \"view.replay\"\nfade_inn_ms = 1\n{s}"),
        Severity::Error,
        "Unknown key `fade_inn_ms` in a rule; did you mean `fade_in_ms`?",
    );
    only(
        &format!("[[when]]\non = \"view.replay\"\nfor_ms = 1\n{s}"),
        Severity::Error,
        "`for_ms` is for events; view.replay is a yes/no signal",
    );
}

#[test]
fn set_tables_are_checked_against_the_configs_instances() {
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.bloom]\nx = 1\n",
        Severity::Error,
        "[when.set.bloom] names no instance of this config",
    );
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain.heavy]\nsize = 1\n",
        Severity::Error,
        "[when.set.grain.heavy] names no instance of this config",
    );
    let off = "name = \"a\"\n[grain]\nenabled = false\n[ambient_light]\n[[when]]\non = \"view.replay\"\n[when.set.grain]\nsize = 1\n";
    let m: Vec<_> = lint(off, &catalog())
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert!(
        m.contains(&"[when.set.grain] names an instance that's off in this config".to_owned()),
        "{m:?}"
    );
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\namout = 1\n",
        Severity::Error,
        "grain has no option `amout`; did you mean `amount`?",
    );
    let text = format!("{HEAD}[[when]]\non = \"view.replay\"\n[when.set.grain]\namout = 1\n");
    let l = lint(&text, &catalog());
    assert_eq!(l.diagnostics[0].fix.as_ref().unwrap().replacement, "amount");
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\nenabled = true\n",
        Severity::Error,
        "`enabled` can only be false in a rule; a rule turns an instance off, never on",
    );
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\namount = 5\n",
        Severity::Warning,
        "`amount` is above its maximum of 1.0; it is clamped to 1.0",
    );
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\nsize = 0.5\n",
        Severity::Error,
        "`size` expects a whole number, not a decimal number",
    );
    only(
        "[[when]]\non = \"view.replay\"\n[when.set.grain]\namount = 1\n[when.set.grain]\namount = 2\n",
        Severity::Error,
        "[when.set.grain] appears twice in this rule",
    );
    only(
        "[[when]]\non = \"view.replay\"\n",
        Severity::Warning,
        "This rule sets nothing; add a `[when.set.<effect>]` table",
    );
}

#[test]
fn signals_that_the_game_does_not_read_warn() {
    let s = "[when.set.grain]\namount = 1\n";
    only(
        &format!("[[when]]\non = \"session.weather\"\nis = \"storm\"\n{s}"),
        Severity::Warning,
        "session.weather isn't read in game yet; this rule only fires in the Preview",
    );
    let m = msgs(&format!("[[when]]\non = \"view.replay\"\n{s}"));
    assert!(m.is_empty(), "{m:?}");
    let both = msgs(&format!(
        "[[when]]\nall = [{{ on = \"session.weather\", is = \"storm\" }}, {{ on = \"session.weather\", is = \"heavy_rain\" }}]\n{s}"
    ));
    assert_eq!(both.len(), 1, "{both:?}");
}

#[test]
fn limits_are_enforced() {
    let one = "[[when]]\non = \"view.replay\"\n[when.set.grain]\namount = 1\n";
    let seventeen = one.repeat(17);
    let m = msgs(&seventeen);
    assert_eq!(
        m.iter()
            .filter(|(_, t)| t.starts_with("A config has at most 16 rules"))
            .count(),
        1,
        "{m:?}"
    );
    assert!(
        lint(&format!("{HEAD}{}", one.repeat(16)), &catalog())
            .config
            .is_some()
    );
    let mut big = String::from("name = \"a\"\n");
    for i in 0..6 {
        big.push_str(&format!("[ambient_light.a{i}]\n"));
    }
    big.push_str("[[when]]\non = \"view.replay\"\n");
    for i in 0..6 {
        big.push_str(&format!(
            "[when.set.ambient_light.a{i}]\nintensity = 1\nlens = true\nadapt_black_level = \"low\"\ntint = [1, 1, 1]\ntaps = 4\noffset = [0, 0]\n"
        ));
    }
    let m: Vec<(Severity, String)> = lint(&big, &catalog())
        .diagnostics
        .into_iter()
        .map(|d| (d.severity, d.message))
        .collect();
    assert!(
        m.iter()
            .any(|(_, t)| t == "A rule has at most 32 overrides; this one has 36"),
        "{m:?}"
    );
}

#[test]
fn the_bare_when_table_has_a_double_bracket_fix() {
    let text = format!("{HEAD}[when]\non = \"view.replay\"\n");
    let l = lint(&text, &catalog());
    let d = l
        .diagnostics
        .iter()
        .find(|d| d.message.contains("[[when]]"))
        .unwrap();
    assert_eq!(d.message, "`[when]` needs double brackets: `[[when]]`");
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(
        (&text[fix.range.clone()], fix.replacement.as_str()),
        ("[when]", "[[when]]")
    );
    assert!(l.config.is_none());
}

#[test]
fn a_config_with_rules_is_still_valid_toml_for_the_real_parser() {
    let text = format!(
        "{HEAD}[[when]]\nname = \"k\"\nall = [\n  {{ on = \"view.camera\", is = [\"cockpit\"] }},\n  {{ on = \"car.speed\", above = 150 }}, # c\n]\n[when.set.grain]\namount = 0.5\n"
    );
    let l = lint(&text, &catalog());
    assert!(l.config.is_some(), "{:?}", l.diagnostics);
    text.parse::<toml::Table>().unwrap();
}
