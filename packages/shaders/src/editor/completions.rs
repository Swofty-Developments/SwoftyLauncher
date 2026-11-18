use std::collections::HashSet;
use std::ops::Range;

use crate::config;
use crate::editor::context::{Section, in_comment, table_context, word_end};
use crate::editor::default_text;
use crate::signals::{SIGNALS, Signal, SignalKind, signal};
use crate::text::line_at;
use crate::when::{INLINE_KEYS, RULE_KEYS, rule_key_doc};
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

fn find_on(segment: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = segment[from..].find("on") {
        let at = from + i;
        from = at + 2;
        let before_ok = segment[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        let rest = segment[at + 2..].trim_start();
        if before_ok && let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(q) = r.strip_prefix('"') {
                return q.find('"').map(|k| q[..k].to_owned());
            }
        }
    }
    None
}

fn rule_value(text: &str, line: &Range<usize>, offset: usize) -> Option<Vec<Completion>> {
    let prefix = &text[line.start..offset];
    let (mut quote, mut escaped) = (None, false);
    let (mut last_eq, mut sep, mut open_brace) = (None, 0, None);
    for (i, c) in prefix.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '=' => {
                    last_eq = Some(i);
                    sep = i;
                }
                '[' | ',' => sep = i,
                '{' => open_brace = Some(i),
                _ => {}
            },
        }
    }
    let eq = last_eq?;
    if open_brace.is_some_and(|b| b > eq) {
        return None;
    }
    let (mut in_array, mut quote2) = (false, None);
    for c in prefix[eq + 1..].chars() {
        match quote2 {
            Some(q) if c == q => quote2 = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote2 = Some(c),
                '}' | ']' => return None,
                '[' => in_array = true,
                ',' if !in_array => return None,
                _ => {}
            },
        }
    }
    let key = prefix[..eq]
        .trim_end()
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    let lead = prefix[sep + 1..].len() - prefix[sep + 1..].trim_start().len();
    let vstart = line.start + sep + 1 + lead;
    let quoted = text[vstart..line.end].starts_with('"');
    let end = if quoted {
        let tail = &text[vstart + 1..line.end];
        tail.find('"')
            .map_or(line.end, |k| vstart + 1 + k + 1)
            .max(offset)
    } else {
        let tail = &text[offset..line.end];
        offset + tail.find([' ', '\t', ',', ']', '}', '#']).unwrap_or(tail.len())
    };
    let range = vstart..end;
    let typed = text[vstart..offset].trim_matches('"').to_ascii_lowercase();
    let item = |label: String, insert: String, detail: String, doc: String| Completion {
        label,
        kind: CompletionKind::Value,
        detail,
        doc,
        range: range.clone(),
        insert,
    };
    Some(match key {
        "on" => SIGNALS
            .iter()
            .filter(|s| s.id.contains(&typed))
            .map(|s| signal_completion(s, &range))
            .collect(),
        "is" => {
            let segment_start = open_brace.unwrap_or(0);
            let id = find_on(&prefix[segment_start..]).or_else(|| {
                let doc = config::parse(text);
                let rule = doc
                    .rules
                    .iter()
                    .find(|r| r.span.start <= line.start && line.start <= r.span.end)?;
                match &rule.entries.iter().find(|e| e.key == "on")?.value.lit {
                    config::Lit::Str(s) => Some(s.clone()),
                    _ => None,
                }
            });
            let Some((_, sig)) = id.as_deref().and_then(signal) else {
                return Some(Vec::new());
            };
            sig.values
                .iter()
                .filter(|v| v.starts_with(&typed))
                .map(|v| {
                    item(
                        (*v).to_owned(),
                        format!("\"{v}\""),
                        sig.id.to_owned(),
                        sig.doc.to_owned(),
                    )
                })
                .collect()
        }
        "wheels" => ["1", "2", "3", "4"]
            .into_iter()
            .filter(|v| v.starts_with(&typed))
            .map(|v| {
                item(
                    v.to_owned(),
                    v.to_owned(),
                    "wheels".into(),
                    "How many wheels must match.".into(),
                )
            })
            .collect(),
        _ => Vec::new(),
    })
}

fn rule_key_completions(
    keys: &[&str],
    present: impl Fn(&str) -> bool,
    typed: &str,
    range: Range<usize>,
    full_line: bool,
) -> Vec<Completion> {
    let typed = typed.to_ascii_lowercase();
    keys.iter()
        .filter(|k| !present(k) && k.starts_with(&typed))
        .filter_map(|k| {
            let (ty, doc, default) = rule_key_doc(k)?;
            Some(Completion {
                label: (*k).to_owned(),
                kind: CompletionKind::Keyword,
                detail: ty.to_owned(),
                doc: doc.to_owned(),
                range: range.clone(),
                insert: if full_line {
                    format!("{k} = {default}")
                } else {
                    (*k).to_owned()
                },
            })
        })
        .collect()
}

fn header_completions(
    text: &str,
    body: &str,
    offset: usize,
    line: &Range<usize>,
    catalog: &Catalog,
) -> Vec<Completion> {
    let after = &body[1..];
    let end = word_end(text, offset);
    if let Some(inner) = after.strip_prefix('[') {
        if inner.contains(']') {
            return Vec::new();
        }
        let typed = inner.trim_start();
        let closed = text[end..line.end].trim_start().starts_with("]]");
        return if "when".starts_with(&typed.to_ascii_lowercase()) {
            vec![Completion {
                label: "when".into(),
                kind: CompletionKind::Keyword,
                detail: "Shader event rule".into(),
                doc: "A rule that changes the config's options while a signal is on.".into(),
                range: offset - typed.len()..end,
                insert: if closed { "when".into() } else { "when]]".into() },
            }]
        } else {
            Vec::new()
        };
    }
    if after.contains(']') {
        return Vec::new();
    }
    let closed = text[end..line.end].trim_start().starts_with(']');
    let segments: Vec<&str> = after.split('.').map(str::trim_start).collect();
    if segments.first().map(|s| s.trim_end()) != Some("when") {
        if after.contains('.') {
            return Vec::new();
        }
        let typed = after.trim_start();
        let from = offset - typed.len();
        let t = typed.to_ascii_lowercase();
        return catalog
            .effects
            .iter()
            .filter(|e| e.id.starts_with(&t))
            .map(|e| Completion {
                label: e.id.clone(),
                kind: CompletionKind::Effect,
                detail: e.name.clone(),
                doc: e.description.clone(),
                range: from..end,
                insert: if closed {
                    e.id.clone()
                } else {
                    format!("{}]", e.id)
                },
            })
            .collect();
    }
    let typed = segments.last().copied().unwrap_or("");
    let from = offset - typed.len();
    let t = typed.to_ascii_lowercase();
    let keyword = |label: &str, doc: &str, insert: String| Completion {
        label: label.to_owned(),
        kind: CompletionKind::Keyword,
        detail: "rule override".into(),
        doc: doc.to_owned(),
        range: from..end,
        insert,
    };
    match segments.len() {
        2 if "set".starts_with(&t) => vec![keyword(
            "set",
            "Options this rule overrides: `[when.set.<effect>]`.",
            "set.".into(),
        )],
        3 if segments[1].trim_end() == "set" => {
            let doc = config::parse(text);
            let mut names: Vec<String> = Vec::new();
            for tbl in doc.tables.iter().filter(|t| !t.bad && !t.effect.is_empty()) {
                let name = match &tbl.label {
                    Some(l) => format!("{}.{l}", tbl.effect),
                    None => tbl.effect.clone(),
                };
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            names
                .into_iter()
                .filter(|n| n.starts_with(&t))
                .map(|n| Completion {
                    label: n.clone(),
                    kind: CompletionKind::Effect,
                    detail: "instance of this config".into(),
                    doc: String::new(),
                    range: from..end,
                    insert: if closed { n } else { format!("{n}]") },
                })
                .collect()
        }
        _ => Vec::new(),
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
