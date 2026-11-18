//! Config checks (DESIGN.md §2).

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;

use crate::config::{self, Elem, Lit, RawEntry, RawValue};
use crate::text::{Lines, did_you_mean, number};
use crate::{Catalog, Config, Diagnostic, Effect, Fix, Instance, Lint, Param, ParamKind, Severity, Value};

const TOP_KEYS: [&str; 4] = ["name", "author", "description", "swfx"];

pub(crate) fn bounds(kind: &ParamKind) -> (Option<f64>, Option<f64>) {
    match kind {
        ParamKind::Float { min, max, .. }
        | ParamKind::Float2 { min, max }
        | ParamKind::Color3 { min, max }
        | ParamKind::Color4 { min, max } => (*min, *max),
        ParamKind::Int { min, max } => (min.map(|v| v as f64), max.map(|v| v as f64)),
        ParamKind::Bool | ParamKind::Enum { .. } => (None, None),
    }
}

pub(crate) struct Sink<'a> {
    lines: Lines<'a>,
    out: Vec<Diagnostic>,
}

impl Sink<'_> {
    pub(crate) fn push(
        &mut self,
        severity: Severity,
        range: Range<usize>,
        message: String,
        fix: Option<Fix>,
    ) {
        let (start, end) = self.lines.range(&range);
        self.out.push(Diagnostic {
            severity,
            range,
            start,
            end,
            message,
            fix,
        });
    }

    pub(crate) fn error(&mut self, range: Range<usize>, message: impl Into<String>) {
        self.push(Severity::Error, range, message.into(), None);
    }
}

pub(crate) fn describe(lit: &Lit) -> &'static str {
    match lit {
        Lit::Int(_) => "a whole number",
        Lit::Float(_) => "a decimal number",
        Lit::Bool(_) => "true or false",
        Lit::Str(_) => "a string",
        Lit::Array(_) => "an array",
        Lit::Strs(_) => "an array of strings",
        Lit::Tables(_) => "an array of tables",
    }
}

fn clamp_one(
    sink: &mut Sink,
    key: &str,
    v: f64,
    range: &Range<usize>,
    (min, max): (Option<f64>, Option<f64>),
    float: bool,
) -> f64 {
    let bound = match (min, max) {
        (Some(m), _) if v < m => Some(("below its minimum", m)),
        (_, Some(m)) if v > m => Some(("above its maximum", m)),
        _ => None,
    };
    let Some((what, to)) = bound else { return v };
    let shown = number(to, float);
    sink.push(
        Severity::Warning,
        range.clone(),
        format!("`{key}` is {what} of {shown}; it is clamped to {shown}"),
        Some(Fix {
            title: format!("Clamp to {shown}"),
            range: range.clone(),
            replacement: shown.clone(),
        }),
    );
    to
}

pub(crate) fn check_param(sink: &mut Sink, p: &Param, v: &RawValue) -> Option<Value> {
    let key = &p.id;
    let range = &v.range;
    let wrong = |sink: &mut Sink, want: &str| {
        sink.error(
            range.clone(),
            format!("`{key}` expects {want}, not {}", describe(&v.lit)),
        );
        None
    };
    match (&p.kind, &v.lit) {
        (ParamKind::Float { .. }, Lit::Int(_) | Lit::Float(_)) => {
            let n = match v.lit {
                Lit::Int(i) => i as f64,
                Lit::Float(f) => f,
                _ => unreachable!(),
            };
            Some(Value::Number(clamp_one(
                sink,
                key,
                n,
                range,
                bounds(&p.kind),
                true,
            )))
        }
        (ParamKind::Float { .. }, _) => wrong(sink, "a number"),
        (ParamKind::Int { .. }, Lit::Int(i)) => Some(Value::Number(clamp_one(
            sink,
            key,
            *i as f64,
            range,
            bounds(&p.kind),
            false,
        ))),
        (ParamKind::Int { .. }, Lit::Float(_)) => wrong(sink, "a whole number"),
        (ParamKind::Int { .. }, _) => wrong(sink, "a whole number"),
        (ParamKind::Bool, Lit::Bool(b)) => Some(Value::Bool(*b)),
        (ParamKind::Bool, _) => wrong(sink, "true or false"),
        (ParamKind::Enum { values }, Lit::Str(s)) => {
            if values.contains(s) {
                return Some(Value::Text(s.clone()));
            }
            let list = values
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let fix = did_you_mean(s, values.iter().map(String::as_str)).map(|c| Fix {
                title: format!("Change to \"{c}\""),
                range: range.clone(),
                replacement: format!("\"{c}\""),
            });
            sink.push(
                Severity::Error,
                range.clone(),
                format!("\"{s}\" is not a value of `{key}`; use one of {list}"),
                fix,
            );
            None
        }
        (ParamKind::Enum { values }, _) => {
            let list = values
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ");
            sink.error(
                range.clone(),
                format!("`{key}` expects one of {list}, not {}", describe(&v.lit)),
            );
            None
        }
        (
            k @ (ParamKind::Float2 { .. } | ParamKind::Color3 { .. } | ParamKind::Color4 { .. }),
            Lit::Array(items),
        ) => {
            let n = k.width();
            if items.len() != n {
                sink.error(
                    range.clone(),
                    format!("`{key}` expects {n} numbers, not {}", items.len()),
                );
                return None;
            }
            let out: Vec<f64> = items
                .iter()
                .map(|Elem { num, range, .. }| clamp_one(sink, key, *num, range, bounds(k), true))
                .collect();
            Some(Value::Vector(out))
        }
        (k, _) => wrong(sink, &format!("an array of {} numbers", k.width())),
    }
}

fn first_line(text: &str) -> Range<usize> {
    let start = text.len() - text.trim_start().len();
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    start..text[..end].trim_end().len().max(start)
}

fn top_level(
    sink: &mut Sink,
    text: &str,
    top: &[RawEntry],
) -> (Option<String>, Option<String>, Option<String>) {
    let (mut name, mut author, mut description) = (None, None, None);
    for e in top {
        let slot = match e.key.as_str() {
            "name" => &mut name,
            "author" => &mut author,
            "description" => &mut description,
            "swfx" => {
                if e.value.lit != Lit::Int(1) {
                    sink.error(
                        e.value.range.clone(),
                        "`swfx` must be 1, the only config format version",
                    );
                }
                continue;
            }
            other => {
                let fix = did_you_mean(other, TOP_KEYS.into_iter()).map(|c| Fix {
                    title: format!("Change to `{c}`"),
                    range: e.key_range.clone(),
                    replacement: c.to_owned(),
                });
                sink.push(
                    Severity::Warning,
                    e.key_range.clone(),
                    format!("Unknown top-level key `{other}`; it is ignored"),
                    fix,
                );
                continue;
            }
        };
        match &e.value.lit {
            Lit::Str(s) if e.key != "name" || !s.trim().is_empty() => *slot = Some(s.clone()),
            Lit::Str(_) => sink.error(e.value.range.clone(), "`name` must not be empty"),
            other => sink.error(
                e.value.range.clone(),
                format!("`{}` expects a string, not {}", e.key, describe(other)),
            ),
        }
    }
    if name.is_none() && !top.iter().any(|e| e.key == "name") {
        let at = first_line(text);
        let fix = Fix {
            title: "Add a name".into(),
            range: 0..0,
            replacement: "name = \"Untitled\"\n".into(),
        };
        sink.push(
            Severity::Error,
            at,
            "A config needs a `name = \"...\"`".into(),
            Some(fix),
        );
    }
    (name, author, description)
}

fn instance(sink: &mut Sink, effect: &Effect, table: &config::RawTable) -> Instance {
    let mut enabled = true;
    let mut values = BTreeMap::new();
    for e in &table.entries {
        if e.key == "enabled" {
            match e.value.lit {
                Lit::Bool(b) => enabled = b,
                _ => sink.error(
                    e.value.range.clone(),
                    format!("`enabled` expects true or false, not {}", describe(&e.value.lit)),
                ),
            }
            continue;
        }
        let Some(p) = effect.params.iter().find(|p| p.id == e.key) else {
            let names = effect.params.iter().map(|p| p.id.as_str()).chain(["enabled"]);
            let fix = did_you_mean(&e.key, names).map(|c| Fix {
                title: format!("Change to `{c}`"),
                range: e.key_range.clone(),
                replacement: c.to_owned(),
            });
            sink.push(
                Severity::Error,
                e.key_range.clone(),
                format!("`{}` has no param `{}`", effect.id, e.key),
                fix,
            );
            continue;
        };
        if let Some(v) = check_param(sink, p, &e.value) {
            values.insert(p.id.clone(), v);
        }
    }
    Instance {
        effect: effect.id.clone(),
        label: table.label.clone(),
        enabled,
        values,
        span: table.span.clone(),
    }
}

/// Parses and checks a config against the catalog (DESIGN.md §2).
pub fn lint(text: &str, catalog: &Catalog) -> Lint {
    let doc = config::parse(text);
    let mut sink = Sink {
        lines: Lines::new(text),
        out: Vec::new(),
    };
    for e in &doc.errors {
        sink.error(e.range.clone(), e.message.clone());
    }
    let (name, author, description) = top_level(&mut sink, text, &doc.top);

    let mut instances = Vec::new();
    let mut seen: HashSet<(String, Option<String>)> = HashSet::new();
    for table in doc.tables.iter().filter(|t| !t.bad) {
        if table.effect == "when" && table.label.is_none() {
            let fix = Fix {
                title: "Change to `[[when]]`".into(),
                range: table.header.clone(),
                replacement: "[[when]]".into(),
            };
            let msg = "`[when]` needs double brackets: `[[when]]`".to_owned();
            sink.push(Severity::Error, table.header.clone(), msg, Some(fix));
            continue;
        }
        let Some(effect) = catalog.get(&table.effect) else {
            let fix = catalog.did_you_mean(&table.effect).map(|c| Fix {
                title: format!("Change to `{c}`"),
                range: table.effect_range.clone(),
                replacement: c.to_owned(),
            });
            let msg = format!(
                "Unknown effect `{}`; if it comes from an addon, that addon is not installed",
                table.effect
            );
            sink.push(Severity::Error, table.effect_range.clone(), msg, fix);
            continue;
        };
        if !seen.insert((table.effect.clone(), table.label.clone())) {
            sink.error(
                table.header.clone(),
                format!(
                    "`[{}]` appears twice; give the second a label, like `[{}.second]`",
                    table.effect, table.effect
                ),
            );
            continue;
        }
        if let Some(label) = &table.label
            && effect.params.iter().any(|p| &p.id == label)
        {
            sink.error(
                table.header.clone(),
                format!("The label `{label}` is also a param of `{}`", effect.id),
            );
        }
        instances.push(instance(&mut sink, effect, table));
    }

    let rules = crate::when::check(&mut sink, &doc, &instances, catalog);

    let has_errors = sink.out.iter().any(|d| d.severity == Severity::Error);
    if !has_errors && !instances.iter().any(|i| i.enabled) {
        let at = doc
            .top
            .iter()
            .find(|e| e.key == "name")
            .map_or_else(|| first_line(text), |e| e.range.clone());
        sink.push(Severity::Info, at, "This config changes nothing".into(), None);
    }
    let mut diagnostics = sink.out;
    diagnostics.sort_by_key(|d| (d.range.start, d.range.end));
    let config = (!has_errors).then(|| Config {
        name: name.unwrap_or_default(),
        author,
        description,
        instances,
        rules,
    });
    Lint { config, diagnostics }
}

#[cfg(test)]
pub(crate) mod tests;
