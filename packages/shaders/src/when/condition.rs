use std::ops::Range;

use crate::config::{Lit, RawEntry};
use crate::lint::{Sink, describe};
use crate::signals::{SIGNALS, Signal, SignalKind, signal};
use crate::text::did_you_mean;
use crate::when::{get, number};
use crate::{Cond, CondOp, Fix, Ramp, Severity};

fn names_of(sig: &Signal) -> String {
    sig.values.join(", ")
}

fn quoted_fix(title_word: &str, range: &Range<usize>, word: &str) -> Fix {
    Fix {
        title: format!("Change to `{title_word}`"),
        range: range.clone(),
        replacement: format!("\"{word}\""),
    }
}

pub(super) enum Parsed {
    Cond(Cond, &'static Signal),
    Ramp(Ramp, &'static Signal),
}

pub(super) fn one_kind(sink: &mut Sink, entries: &[RawEntry], with_all: bool) -> bool {
    let kind_of = |key: &str| match key {
        "is" => Some("is"),
        "above" | "below" => Some("above/below"),
        "from" | "to" => Some("from/to"),
        "all" if with_all => Some("all"),
        _ => None,
    };
    let mut first: Option<(&str, &str)> = None;
    for e in entries {
        let Some(kind) = kind_of(&e.key) else { continue };
        match first {
            None => first = Some((kind, &e.key)),
            Some((k, _)) if k == kind => {}
            Some((_, a)) => {
                sink.error(
                    e.key_range.clone(),
                    format!(
                        "A rule has one kind of condition; `{a}` and `{}` can't be used together",
                        e.key
                    ),
                );
                return false;
            }
        }
    }
    true
}

fn lookup(sink: &mut Sink, on: &RawEntry) -> Option<&'static Signal> {
    let Lit::Str(id) = &on.value.lit else {
        sink.error(
            on.value.range.clone(),
            format!(
                "`on` expects a signal id in quotes, not {}",
                describe(&on.value.lit)
            ),
        );
        return None;
    };
    match signal(id) {
        Some((_, s)) => Some(s),
        None => {
            let near = did_you_mean(id, SIGNALS.iter().map(|s| s.id));
            let message = match near {
                Some(c) => format!("unknown signal `{id}`; did you mean `{c}`?"),
                None => format!("unknown signal `{id}`"),
            };
            let fix = near.map(|c| quoted_fix(c, &on.value.range, c));
            sink.push(Severity::Error, on.value.range.clone(), message, fix);
            None
        }
    }
}

pub(super) fn condition(
    sink: &mut Sink,
    entries: &[RawEntry],
    at: &Range<usize>,
    inline: bool,
    warned: &mut Vec<&'static str>,
) -> Option<Parsed> {
    let Some(on) = get(entries, "on") else {
        sink.error(at.clone(), "Each condition in `all` needs `on = \"<signal>\"`");
        return None;
    };
    let sig = lookup(sink, on)?;
    if !sig.in_game && !warned.contains(&sig.id) {
        warned.push(sig.id);
        sink.push(
            Severity::Warning,
            on.value.range.clone(),
            format!(
                "{} isn't read in game yet; this rule only fires in the Preview",
                sig.id
            ),
            None,
        );
    }
    let id = sig.id;
    let word = sig.kind_word();
    let is = get(entries, "is");
    let above = get(entries, "above");
    let below = get(entries, "below");
    let from = get(entries, "from");
    let to = get(entries, "to");
    let wheels_e = get(entries, "wheels");
    if inline {
        if let Some(e) = from.or(to) {
            sink.error(e.key_range.clone(), "A ramp (`from`/`to`) can't go inside `all`");
            return None;
        }
        if sig.kind == SignalKind::Event {
            sink.error(
                on.value.range.clone(),
                format!("{id} is an event; events can't go inside `all`"),
            );
            return None;
        }
    }
    let mut wheels = 1u8;
    if let Some(w) = wheels_e {
        if !sig.per_wheel() {
            sink.error(
                w.key_range.clone(),
                format!("`wheels` is for car.surface; {id} has none"),
            );
            return None;
        }
        match w.value.lit {
            Lit::Int(n) if (1..=4).contains(&n) => wheels = n as u8,
            _ => {
                sink.error(
                    w.value.range.clone(),
                    "`wheels` must be a whole number from 1 to 4",
                );
                return None;
            }
        }
    }
    let cond = |op| {
        Some(Parsed::Cond(
            Cond {
                signal: id.to_owned(),
                op,
            },
            sig,
        ))
    };
    if let Some(e) = is {
        if sig.kind != SignalKind::Name {
            sink.error(
                e.key_range.clone(),
                format!("`is` needs a name signal; {id} is {word}"),
            );
            return None;
        }
        let items: Vec<(String, Range<usize>)> = match &e.value.lit {
            Lit::Str(s) => vec![(s.clone(), e.value.range.clone())],
            Lit::Strs(list) if !list.is_empty() => {
                list.iter().map(|s| (s.text.clone(), s.range.clone())).collect()
            }
            _ => {
                sink.error(
                    e.value.range.clone(),
                    format!(
                        "`is` expects a name or a list of names, not {}",
                        describe(&e.value.lit)
                    ),
                );
                return None;
            }
        };
        let mut values = Vec::new();
        let mut ok = true;
        for (text, range) in items {
            if sig.values.contains(&text.as_str()) {
                values.push(text);
            } else {
                ok = false;
                let fix = did_you_mean(&text, sig.values.iter().copied()).map(|c| quoted_fix(c, &range, c));
                sink.push(
                    Severity::Error,
                    range,
                    format!("`is` needs a value of {id}: {}", names_of(sig)),
                    fix,
                );
            }
        }
        return ok.then(|| {
            Parsed::Cond(
                Cond {
                    signal: id.to_owned(),
                    op: CondOp::Is { values, wheels },
                },
                sig,
            )
        });
    }
    if above.is_some() || below.is_some() {
        let numeric = matches!(sig.kind, SignalKind::Number)
            || (sig.kind == SignalKind::Event && sig.magnitude.is_some());
        if !numeric {
            let key = above.or(below).expect("one is set");
            let why = if sig.kind == SignalKind::Event {
                format!("{id} is an event with no magnitude")
            } else {
                format!("{id} is {word}")
            };
            sink.error(
                key.key_range.clone(),
                format!("`{}` needs a number signal; {why}", key.key),
            );
            return None;
        }
        let a = above.and_then(|e| number(sink, e));
        let b = below.and_then(|e| number(sink, e));
        if (above.is_some() && a.is_none()) || (below.is_some() && b.is_none()) {
            return None;
        }
        return match (a, b) {
            (Some(lo), Some(hi)) => {
                if lo >= hi {
                    sink.error(
                        above.expect("set").value.range.clone(),
                        "`above` must be below `below`",
                    );
                    return None;
                }
                cond(CondOp::Between(lo, hi))
            }
            (Some(lo), None) => cond(CondOp::Above(lo)),
            (None, Some(hi)) => cond(CondOp::Below(hi)),
            (None, None) => None,
        };
    }
    if from.is_some() || to.is_some() {
        let key = from.or(to).expect("one is set");
        if sig.kind != SignalKind::Number {
            sink.error(
                key.key_range.clone(),
                format!("`{}` needs a number signal; {id} is {word}", key.key),
            );
            return None;
        }
        let (Some(f), Some(t)) = (from, to) else {
            let (have, missing) = if from.is_some() {
                ("from", "to")
            } else {
                ("to", "from")
            };
            sink.error(key.key_range.clone(), format!("`{have}` needs `{missing}` too"));
            return None;
        };
        let (fv, tv) = (number(sink, f)?, number(sink, t)?);
        if fv >= tv {
            sink.error(f.value.range.clone(), "`from` must be below `to`");
            return None;
        }
        return Some(Parsed::Ramp(
            Ramp {
                signal: id.to_owned(),
                from: fv,
                to: tv,
            },
            sig,
        ));
    }
    match sig.kind {
        SignalKind::Bool => cond(CondOp::True),
        SignalKind::Event => cond(CondOp::Event),
        SignalKind::Number => {
            sink.error(
                on.value.range.clone(),
                format!("{id} is a number; add `above`, `below`, or `from` and `to`"),
            );
            None
        }
        SignalKind::Name => {
            sink.error(
                on.value.range.clone(),
                format!("{id} is a name; add `is = \"...\"` with {}", names_of(sig)),
            );
            None
        }
    }
}
