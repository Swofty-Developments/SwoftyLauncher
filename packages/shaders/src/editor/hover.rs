use std::ops::Range;

use crate::config::{self, RawEntry};
use crate::editor::default_text;
use crate::lint::bounds;
use crate::signals::{Signal, SignalKind, signal};
use crate::text::number;
use crate::when::rule_key_doc;
use crate::{Catalog, Effect, Hover, Param, ParamKind};

fn type_name(kind: &ParamKind) -> &'static str {
    match kind {
        ParamKind::Float { .. } => "float",
        ParamKind::Int { .. } => "int",
        ParamKind::Bool => "bool",
        ParamKind::Enum { .. } => "enum",
        ParamKind::Float2 { .. } => "float2",
        ParamKind::Color3 { .. } => "color3",
        ParamKind::Color4 { .. } => "color4",
    }
}

fn range_text(p: &Param) -> Option<String> {
    let float = !matches!(p.kind, ParamKind::Int { .. });
    match bounds(&p.kind) {
        (Some(a), Some(b)) => Some(format!("{} to {}", number(a, float), number(b, float))),
        (Some(a), None) => Some(format!("at least {}", number(a, float))),
        (None, Some(b)) => Some(format!("at most {}", number(b, float))),
        (None, None) => None,
    }
}

fn effect_hover(e: &Effect, range: Range<usize>) -> Hover {
    let mut text = format!("**{}** (`{}`)", e.name, e.id);
    if !e.description.is_empty() {
        text.push_str(&format!("\n\n{}", e.description));
    }
    if !e.params.is_empty() {
        text.push_str("\n\n");
        let lines: Vec<String> = e
            .params
            .iter()
            .map(|p| format!("- `{}` {} ({})", p.id, p.label, type_name(&p.kind)))
            .collect();
        text.push_str(&lines.join("\n"));
    }
    Hover { text, range }
}

fn param_hover(p: &Param, range: Range<usize>) -> Hover {
    let mut text = format!("**{}** (`{}`, {})", p.label, p.id, type_name(&p.kind));
    if !p.doc.is_empty() {
        text.push_str(&format!("\n\n{}", p.doc));
    }
    if let ParamKind::Enum { values } = &p.kind {
        text.push_str(&format!(
            "\n\nOne of: {}",
            values
                .iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else if let Some(r) = range_text(p) {
        text.push_str(&format!("\n\nRange: {r}"));
    }
    text.push_str(&format!("\n\nDefault: `{}`", default_text(p)));
    Hover { text, range }
}

fn entry_hover(e: &RawEntry, offset: usize, effect: Option<&Effect>) -> Option<Hover> {
    let range = if e.key_range.contains(&offset) || e.key_range.end == offset {
        e.key_range.clone()
    } else {
        e.value.range.clone()
    };
    match effect {
        Some(_) if e.key == "enabled" => Some(Hover {
            text: "**Enabled** (`enabled`, bool)\n\nTurn this instance off without deleting it.\n\nDefault: `true`".into(),
            range,
        }),
        Some(fx) => fx.params.iter().find(|p| p.id == e.key).map(|p| param_hover(p, range)),
        None => {
            let doc = match e.key.as_str() {
                "name" => "The config's name, shown in the list.",
                "author" => "Who made the config.",
                "description" => "A short description of the look.",
                "swfx" => "The config format version; only 1 exists.",
                _ => return None,
            };
            Some(Hover { text: format!("**{}** (`{}`)\n\n{}", e.key, e.key, doc), range })
        }
    }
}

fn signal_hover(sig: &Signal, range: Range<usize>) -> Hover {
    let mut text = format!(
        "**{}** ({})\n\n{}",
        sig.id,
        sig.kind_word().trim_start_matches("a "),
        sig.doc
    );
    match sig.kind {
        SignalKind::Name => text.push_str(&format!(
            "\n\nOne of: {}",
            sig.values
                .iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        SignalKind::Number => {
            let unit = if sig.unit.is_empty() {
                String::new()
            } else {
                format!(" {}", sig.unit)
            };
            text.push_str(&format!(
                "\n\nRange: {} to {}{unit}",
                number(sig.min, false),
                number(sig.max, false)
            ));
        }
        SignalKind::Event => {
            if let Some((lo, hi)) = sig.magnitude {
                let unit = if sig.unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", sig.unit)
                };
                text.push_str(&format!(
                    "\n\nMagnitude: {} to {}{unit}; `above` and `below` compare against it.",
                    number(lo, false),
                    number(hi, false)
                ));
            }
        }
        SignalKind::Bool => {}
    }
    if !sig.in_game {
        text.push_str("\n\nNot read in game yet; a rule on it only fires in the Preview.");
    }
    Hover { text, range }
}

fn rule_key_hover(key: &str, range: Range<usize>) -> Option<Hover> {
    let (ty, doc, _) = rule_key_doc(key)?;
    Some(Hover {
        text: format!("**{key}** ({ty})\n\n{doc}"),
        range,
    })
}

fn rule_entry_hover(e: &RawEntry, offset: usize) -> Option<Hover> {
    if e.key_range.start <= offset && offset <= e.key_range.end {
        return rule_key_hover(&e.key, e.key_range.clone());
    }
    match (&e.key[..], &e.value.lit) {
        ("on", config::Lit::Str(id)) => signal(id).map(|(_, s)| signal_hover(s, e.value.range.clone())),
        ("all", config::Lit::Tables(list)) => list
            .iter()
            .flat_map(|t| &t.entries)
            .find(|x| {
                (x.key_range.start <= offset && offset <= x.key_range.end)
                    || (x.key == "on" && x.value.range.start <= offset && offset <= x.value.range.end)
            })
            .and_then(|x| rule_entry_hover(x, offset)),
        _ => None,
    }
}

pub fn hover(text: &str, offset: usize, catalog: &Catalog) -> Option<Hover> {
    let doc = config::parse(text);
    for r in &doc.rules {
        if r.header.start <= offset && offset <= r.header.end {
            return Some(Hover {
                text: "**Rule** (`[[when]]`)\n\nChanges the config's options while a signal is on. Each `[when.set.<effect>]` table below it lists the overrides.".into(),
                range: r.header.clone(),
            });
        }
        for t in r.sets.iter().filter(|t| !t.bad) {
            if t.header.start <= offset && offset <= t.header.end {
                let e = catalog.get(&t.effect)?;
                return Some(effect_hover(e, t.effect_range.clone()));
            }
            for e in &t.entries {
                if e.range.start <= offset && offset <= e.range.end {
                    return entry_hover(e, offset, catalog.get(&t.effect));
                }
            }
        }
        for e in &r.entries {
            if e.range.start <= offset && offset <= e.range.end {
                return rule_entry_hover(e, offset);
            }
        }
    }
    if let Some(t) = doc
        .tables
        .iter()
        .find(|t| t.header.start <= offset && offset <= t.header.end)
    {
        let e = catalog.get(&t.effect)?;
        return Some(effect_hover(e, t.effect_range.clone()));
    }
    for e in &doc.top {
        if e.range.start <= offset && offset <= e.range.end {
            return entry_hover(e, offset, None);
        }
    }
    for t in doc.tables.iter().filter(|t| !t.bad) {
        for e in &t.entries {
            if e.range.start <= offset && offset <= e.range.end {
                return entry_hover(e, offset, catalog.get(&t.effect));
            }
        }
    }
    None
}
