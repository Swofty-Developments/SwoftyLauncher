mod headers;
mod rules;

use std::collections::HashSet;
use std::ops::Range;

use self::headers::header_completions;
use self::rules::{rule_key_completions, rule_value};
use crate::editor::context::{Section, in_comment, table_context, word_end};
use crate::editor::default_text;
use crate::signals::{Signal, SignalKind};
use crate::text::line_at;
use crate::when::{INLINE_KEYS, RULE_KEYS};
use crate::{Catalog, Completion, CompletionKind, Param, ParamKind};

fn key_completions(
    catalog: &Catalog,
    effect: Option<&str>,
    set: &HashSet<String>,
    typed: &str,
    range: Range<usize>,
    full_line: bool,
) -> Vec<Completion> {
    let typed = typed.to_ascii_lowercase();
    let wanted = |k: &str| !set.contains(k) && k.to_ascii_lowercase().starts_with(&typed);
    let make = |label: &str, detail: String, doc: String, default: String| Completion {
        label: label.to_owned(),
        kind: if effect.is_some() {
            CompletionKind::Param
        } else {
            CompletionKind::Keyword
        },
        detail,
        doc,
        range: range.clone(),
        insert: if full_line {
            format!("{label} = {default}")
        } else {
            label.to_owned()
        },
    };
    match effect {
        None => [
            ("name", "The config's name", "\"\""),
            ("author", "Who made the config", "\"\""),
            ("description", "A short description", "\"\""),
        ]
        .into_iter()
        .filter(|(k, ..)| wanted(k))
        .map(|(k, doc, d)| make(k, "string".into(), doc.into(), d.into()))
        .collect(),
        Some(id) => {
            let Some(e) = catalog.get(id) else {
                return Vec::new();
            };
            let mut out: Vec<Completion> = e
                .params
                .iter()
                .filter(|p| wanted(&p.id))
                .map(|p| {
                    let detail = format!("{} · default {}", p.label, default_text(p));
                    make(&p.id, detail, p.doc.clone(), default_text(p))
                })
                .collect();
            if wanted("enabled") {
                out.push(make(
                    "enabled",
                    "bool · default true".into(),
                    "Turn this instance off without deleting it.".into(),
                    "false".into(),
                ));
            }
            out
        }
    }
}

fn value_completions(p: &Param, range: Range<usize>, typed: &str) -> Vec<Completion> {
    let item = |label: String, insert: String| Completion {
        label,
        kind: CompletionKind::Value,
        detail: p.label.clone(),
        doc: p.doc.clone(),
        range: range.clone(),
        insert,
    };
    let typed = typed.trim_matches('"').to_ascii_lowercase();
    match &p.kind {
        ParamKind::Enum { values } => values
            .iter()
            .filter(|v| v.to_ascii_lowercase().starts_with(&typed))
            .map(|v| item(v.clone(), format!("\"{v}\"")))
            .collect(),
        ParamKind::Bool => ["true", "false"]
            .into_iter()
            .filter(|v| v.starts_with(&typed))
            .map(|v| item(v.to_owned(), v.to_owned()))
            .collect(),
        _ => {
            let d = default_text(p);
            vec![item(d.clone(), d)]
        }
    }
}

fn signal_completion(sig: &Signal, range: &Range<usize>) -> Completion {
    let mut detail = match sig.kind {
        SignalKind::Bool => "yes/no".to_owned(),
        SignalKind::Name => "name".to_owned(),
        SignalKind::Number if sig.unit.is_empty() => "number".to_owned(),
        SignalKind::Number => format!("number, {}", sig.unit),
        SignalKind::Event => "event".to_owned(),
    };
    if !sig.in_game {
        detail.push_str(" · Preview only");
    }
    Completion {
        label: sig.id.to_owned(),
        kind: CompletionKind::Value,
        detail,
        doc: sig.doc.to_owned(),
        range: range.clone(),
        insert: format!("\"{}\"", sig.id),
    }
}

pub fn completions(text: &str, offset: usize, catalog: &Catalog) -> Vec<Completion> {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let line = line_at(text, offset);
    let prefix = &text[line.start..offset];
    let comment = in_comment(prefix);
    if comment {
        return Vec::new();
    }
    let lead = prefix.len() - prefix.trim_start_matches([' ', '\t']).len();
    let body = &prefix[lead..];

    if body.starts_with('[') {
        return header_completions(text, body, offset, &line, catalog);
    }

    let (section, set) = table_context(text, line.start);
    if section == Section::Rule
        && let Some(done) = rule_value(text, &line, offset)
    {
        return done;
    }

    if section != Section::Rule
        && let Some(eq) = body.find('=')
        && !body[..eq].contains(['"', '\''])
    {
        let key = body[..eq].trim();
        let abs_eq = line.start + lead + eq;
        let after = &text[abs_eq + 1..];
        let vstart = (abs_eq + 1 + after.len() - after.trim_start_matches([' ', '\t']).len()).min(offset);
        let (Section::Effect(id) | Section::Set(id)) = &section else {
            return Vec::new();
        };
        if text[vstart..offset].starts_with('[') {
            return Vec::new();
        }
        let quoted = text[vstart..].starts_with('"');
        let end = if quoted {
            let tail = &text[vstart + 1..line.end];
            tail.find('"')
                .map_or(line.end, |k| vstart + 1 + k + 1)
                .max(offset)
        } else {
            let tail = &text[offset..line.end];
            offset + tail.find([' ', '\t', ',', ']', '#']).unwrap_or(tail.len())
        };
        if matches!(section, Section::Set(_)) && key == "enabled" {
            return ["false"]
                .into_iter()
                .filter(|v| v.starts_with(text[vstart..offset].trim()))
                .map(|v| Completion {
                    label: v.to_owned(),
                    kind: CompletionKind::Value,
                    detail: "bool".into(),
                    doc: "A rule can only turn an instance off.".into(),
                    range: vstart..end,
                    insert: v.to_owned(),
                })
                .collect();
        }
        let Some(p) = catalog
            .get(id)
            .and_then(|e| e.params.iter().find(|p| p.id == key))
        else {
            return Vec::new();
        };
        return value_completions(p, vstart..end, &text[vstart..offset]);
    }

    if section == Section::Rule
        && let Some(brace) = body.rfind('{')
        && !body[brace..].contains('}')
    {
        let seg = &body[brace + 1..];
        let typed_len = seg
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        let before = seg[..seg.len() - typed_len].trim_end();
        if before.is_empty() || before.ends_with(',') {
            let typed = &seg[seg.len() - typed_len..];
            let from = offset - typed.len();
            let end = word_end(text, offset);
            let full_line = !text[end..line.end].trim_start().starts_with('=');
            return rule_key_completions(
                &INLINE_KEYS,
                |k| seg.contains(&format!("{k} =")),
                typed,
                from..end,
                full_line,
            );
        }
        return Vec::new();
    }

    if body
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        let from = offset - body.len();
        let end = word_end(text, offset);
        let full_line = !text[end..line.end].trim_start().starts_with('=');
        return match &section {
            Section::Rule => {
                rule_key_completions(&RULE_KEYS, |k| set.contains(k), body, from..end, full_line)
            }
            Section::Top => key_completions(catalog, None, &set, body, from..end, full_line),
            Section::Effect(id) | Section::Set(id) => {
                key_completions(catalog, Some(id), &set, body, from..end, full_line)
            }
        };
    }
    Vec::new()
}
