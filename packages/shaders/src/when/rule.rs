use std::collections::BTreeMap;

use crate::config::{Lit, RawEntry, RawRule};
use crate::lint::{Sink, check_param, describe};
use crate::signals::{Signal, SignalKind};
use crate::text::did_you_mean;
use crate::when::condition::{Parsed, condition, one_kind};
use crate::when::{INLINE_KEYS, MAX_MS, MAX_OVERRIDES, MAX_RULES, RULE_KEYS, get, number};
use crate::{Catalog, Fix, Instance, RuleSet, Severity, WhenRule};

fn timing(sink: &mut Sink, entries: &[RawEntry], key: &str, default: f64) -> f64 {
    let Some(e) = get(entries, key) else {
        return default;
    };
    match number(sink, e) {
        Some(v) if (0.0..=MAX_MS).contains(&v) => v,
        Some(_) => {
            sink.error(
                e.value.range.clone(),
                format!("`{key}` must be a number from 0 to 60000"),
            );
            default
        }
        None => default,
    }
}

pub(super) fn check_rule(
    sink: &mut Sink,
    rule: &RawRule,
    order: usize,
    instances: &[Instance],
    catalog: &Catalog,
) -> WhenRule {
    let entries = &rule.entries;
    if order >= MAX_RULES {
        sink.error(
            rule.header.clone(),
            format!(
                "A config has at most {MAX_RULES} rules; this is rule {}",
                order + 1
            ),
        );
    }
    for e in entries {
        if !RULE_KEYS.contains(&e.key.as_str()) {
            let near = did_you_mean(&e.key, RULE_KEYS.into_iter());
            let message = match near {
                Some(c) => format!("Unknown key `{}` in a rule; did you mean `{c}`?", e.key),
                None => format!("Unknown key `{}` in a rule", e.key),
            };
            let fix = near.map(|c| Fix {
                title: format!("Change to `{c}`"),
                range: e.key_range.clone(),
                replacement: c.to_owned(),
            });
            sink.push(Severity::Error, e.key_range.clone(), message, fix);
        }
    }

    let name = get(entries, "name").and_then(|e| match &e.value.lit {
        Lit::Str(s) => Some(s.clone()),
        other => {
            sink.error(
                e.value.range.clone(),
                format!("`name` expects a string, not {}", describe(other)),
            );
            None
        }
    });
    let priority = get(entries, "priority").map_or(0, |e| match e.value.lit {
        Lit::Int(n) if (-100..=100).contains(&n) => n as i32,
        _ => {
            sink.error(
                e.value.range.clone(),
                "`priority` must be a whole number from -100 to 100",
            );
            0
        }
    });
    let min_ms = timing(sink, entries, "min_ms", 0.0);
    let hold_ms = timing(sink, entries, "hold_ms", 0.0);
    let for_ms = timing(sink, entries, "for_ms", 400.0);
    let ramped = get(entries, "from").is_some() || get(entries, "to").is_some();
    let fade_in_ms = timing(sink, entries, "fade_in_ms", if ramped { 0.0 } else { 150.0 });
    let fade_out_ms = timing(sink, entries, "fade_out_ms", if ramped { 0.0 } else { 300.0 });

    let mut when = Vec::new();
    let mut ramp = None;
    let mut event = false;
    let mut warned = Vec::new();
    let mut top_sig: Option<&'static Signal> = None;
    let on = get(entries, "on");
    let all = get(entries, "all");
    if let (Some(on), Some(_)) = (on, all) {
        sink.error(
            on.key_range.clone(),
            "`on` and `all` can't be used together; name each signal inside `all`",
        );
    } else if on.is_none() && all.is_none() {
        sink.error(
            rule.header.clone(),
            "A rule needs `on = \"<signal>\"` or `all = [...]`",
        );
    } else if one_kind(sink, entries, true) {
        if on.is_some() {
            let parsed = condition(sink, entries, &rule.header, false, &mut warned);
            match parsed {
                Some(Parsed::Cond(c, sig)) => {
                    event = sig.kind == SignalKind::Event;
                    top_sig = Some(sig);
                    when.push(c);
                }
                Some(Parsed::Ramp(r, sig)) => {
                    top_sig = Some(sig);
                    ramp = Some(r);
                }
                None => {}
            }
        } else if let Some(all) = all {
            match &all.value.lit {
                Lit::Tables(list) if !list.is_empty() => {
                    for t in list {
                        if let Some(bad) = t.entries.iter().find(|e| !INLINE_KEYS.contains(&e.key.as_str())) {
                            let message = if matches!(bad.key.as_str(), "from" | "to") {
                                "A ramp (`from`/`to`) can't go inside `all`".to_owned()
                            } else {
                                format!(
                                    "`{}` isn't allowed inside `all`; use `on`, `is`, `above`, `below` or `wheels`",
                                    bad.key
                                )
                            };
                            sink.error(bad.key_range.clone(), message);
                            continue;
                        }
                        if !one_kind(sink, &t.entries, false) {
                            continue;
                        }
                        if let Some(Parsed::Cond(c, _)) =
                            condition(sink, &t.entries, &t.range, true, &mut warned)
                        {
                            when.push(c);
                        }
                    }
                }
                Lit::Tables(_) | Lit::Array(_) => {
                    sink.error(all.value.range.clone(), "`all` needs at least one condition");
                }
                other => sink.error(
                    all.value.range.clone(),
                    format!(
                        "`all` expects a list like `[{{ on = \"car.speed\", above = 150 }}]`, not {}",
                        describe(other)
                    ),
                ),
            }
        }
    }

    if let Some(e) = get(entries, "for_ms")
        && !event
    {
        let message = match top_sig {
            Some(sig) => format!("`for_ms` is for events; {} is {}", sig.id, sig.kind_word()),
            None => "`for_ms` is for events; this rule isn't one".to_owned(),
        };
        sink.error(e.key_range.clone(), message);
    }
    if let Some(e) = get(entries, "wheels")
        && on.is_none()
    {
        sink.error(e.key_range.clone(), "`wheels` goes with `on = \"car.surface\"`");
    }

    let mut sets = Vec::new();
    let mut overrides = 0;
    let mut seen: Vec<(&str, Option<&str>)> = Vec::new();
    for t in rule.sets.iter().filter(|t| !t.bad) {
        let shown = match &t.label {
            Some(l) => format!("[when.set.{}.{l}]", t.effect),
            None => format!("[when.set.{}]", t.effect),
        };
        let key = (t.effect.as_str(), t.label.as_deref());
        if seen.contains(&key) {
            sink.error(t.header.clone(), format!("{shown} appears twice in this rule"));
            continue;
        }
        seen.push(key);
        let Some(idx) = instances
            .iter()
            .position(|i| i.effect == t.effect && i.label == t.label)
        else {
            sink.error(
                t.header.clone(),
                format!("{shown} names no instance of this config"),
            );
            continue;
        };
        if !instances[idx].enabled {
            sink.error(
                t.header.clone(),
                format!("{shown} names an instance that's off in this config"),
            );
            continue;
        }
        let Some(effect) = catalog.get(&t.effect) else {
            continue;
        };
        let mut values = BTreeMap::new();
        let mut off = false;
        overrides += t.entries.len();
        for e in &t.entries {
            if e.key == "enabled" {
                match e.value.lit {
                    Lit::Bool(false) => off = true,
                    Lit::Bool(true) => sink.error(
                        e.value.range.clone(),
                        "`enabled` can only be false in a rule; a rule turns an instance off, never on",
                    ),
                    ref other => sink.error(
                        e.value.range.clone(),
                        format!("`enabled` expects true or false, not {}", describe(other)),
                    ),
                }
                continue;
            }
            let Some(p) = effect.params.iter().find(|p| p.id == e.key) else {
                let near = did_you_mean(&e.key, effect.params.iter().map(|p| p.id.as_str()));
                let message = match near {
                    Some(c) => format!("{} has no option `{}`; did you mean `{c}`?", effect.id, e.key),
                    None => format!("{} has no option `{}`", effect.id, e.key),
                };
                let fix = near.map(|c| Fix {
                    title: format!("Change to `{c}`"),
                    range: e.key_range.clone(),
                    replacement: c.to_owned(),
                });
                sink.push(Severity::Error, e.key_range.clone(), message, fix);
                continue;
            };
            if let Some(v) = check_param(sink, p, &e.value) {
                values.insert(p.id.clone(), v);
            }
        }
        sets.push(RuleSet {
            instance: idx,
            values,
            off,
            span: t.span.clone(),
        });
    }
    if overrides > MAX_OVERRIDES {
        sink.error(
            rule.header.clone(),
            format!("A rule has at most {MAX_OVERRIDES} overrides; this one has {overrides}"),
        );
    }
    if rule.sets.is_empty() {
        sink.push(
            Severity::Warning,
            rule.header.clone(),
            "This rule sets nothing; add a `[when.set.<effect>]` table".into(),
            None,
        );
    }

    WhenRule {
        name,
        priority,
        order,
        event,
        when,
        ramp,
        min_ms,
        hold_ms,
        for_ms: if event { for_ms } else { 0.0 },
        fade_in_ms,
        fade_out_ms,
        sets,
        span: rule.span.clone(),
    }
}
