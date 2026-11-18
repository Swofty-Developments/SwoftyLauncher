//! Checks for `[[when]]` rules (shader events): the conditions, timing and `[when.set.*]` overrides.

mod condition;
mod rule;

use self::rule::check_rule;
use crate::config::{Lit, RawDoc, RawEntry};
use crate::lint::{Sink, describe};
use crate::{Catalog, Instance, WhenRule};

pub(crate) const RULE_KEYS: [&str; 15] = [
    "name",
    "on",
    "is",
    "wheels",
    "above",
    "below",
    "from",
    "to",
    "all",
    "min_ms",
    "hold_ms",
    "for_ms",
    "fade_in_ms",
    "fade_out_ms",
    "priority",
];

pub(crate) const INLINE_KEYS: [&str; 5] = ["on", "is", "above", "below", "wheels"];

pub(crate) const MAX_RULES: usize = 16;

pub(crate) const MAX_OVERRIDES: usize = 32;

const MAX_MS: f64 = 60_000.0;

/// Editor docs of the rule keys: (value type, doc, default text).
pub(crate) fn rule_key_doc(key: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match key {
        "name" => (
            "string",
            "A name for the rule, shown in the editor, the Preview and the log.",
            "\"\"",
        ),
        "on" => ("signal", "The signal the rule watches.", "\"\""),
        "is" => (
            "name or list of names",
            "On while a name signal has this value, or any of a list of values.",
            "\"\"",
        ),
        "wheels" => (
            "1-4",
            "car.surface only: how many wheels must match. Default 1.",
            "1",
        ),
        "above" => (
            "number",
            "On above this value; for an event, its magnitude must be above it.",
            "0",
        ),
        "below" => (
            "number",
            "On below this value; for an event, its magnitude must be below it.",
            "0",
        ),
        "from" => (
            "number",
            "Ramp start: the rule is off at `from` and fully on at `to`.",
            "0",
        ),
        "to" => ("number", "Ramp end: the rule is fully on at `to` and above.", "1"),
        "all" => (
            "list of conditions",
            "Every listed condition must hold: `[{ on = \"car.surface\", is = \"kerb\" }, ...]`.",
            "[]",
        ),
        "min_ms" => (
            "ms",
            "The condition must hold this long before the rule turns on. Default 0.",
            "0",
        ),
        "hold_ms" => (
            "ms",
            "The rule stays on this long after the condition clears. Default 0.",
            "0",
        ),
        "for_ms" => (
            "ms",
            "Events only: how long one event keeps the rule on. Default 400.",
            "400",
        ),
        "fade_in_ms" => (
            "ms",
            "How long the rule takes to fade in. Default 150, or 0 for a `from`/`to` ramp.",
            "150",
        ),
        "fade_out_ms" => (
            "ms",
            "How long the rule takes to fade out. Default 300, or 0 for a `from`/`to` ramp.",
            "300",
        ),
        "priority" => (
            "-100 to 100",
            "Higher wins on a shared option; equal priority: later in the file wins. Default 0.",
            "0",
        ),
        _ => return None,
    })
}

fn get<'a>(entries: &'a [RawEntry], key: &str) -> Option<&'a RawEntry> {
    entries.iter().find(|e| e.key == key)
}

fn number(sink: &mut Sink, e: &RawEntry) -> Option<f64> {
    match e.value.lit {
        Lit::Int(i) => Some(i as f64),
        Lit::Float(f) => Some(f),
        ref other => {
            sink.error(
                e.value.range.clone(),
                format!("`{}` expects a number, not {}", e.key, describe(other)),
            );
            None
        }
    }
}

pub(crate) fn check(
    sink: &mut Sink,
    doc: &RawDoc,
    instances: &[Instance],
    catalog: &Catalog,
) -> Vec<WhenRule> {
    doc.rules
        .iter()
        .enumerate()
        .map(|(i, r)| check_rule(sink, r, i, instances, catalog))
        .collect()
}

#[cfg(test)]
mod tests;
