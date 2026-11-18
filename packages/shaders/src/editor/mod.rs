//! Editor support: highlighting tokens, completions and hover docs.

use std::collections::HashSet;
use std::ops::Range;

use crate::config::{self, RawEntry};
use crate::lint::bounds;
use crate::signals::{SIGNALS, Signal, SignalKind, signal};
use crate::text::{line_at, number};
use crate::when::{INLINE_KEYS, RULE_KEYS, rule_key_doc};
use crate::{Catalog, Completion, CompletionKind, Effect, Hover, Param, ParamKind, Token, TokenKind, Value};

fn skip_ws(s: &str, mut i: usize) -> usize {
    while s[i..].starts_with([' ', '\t']) {
        i += 1;
    }
    i
}

fn next_char(s: &str, i: usize) -> char {
    s[i..].chars().next().unwrap_or('\0')
}

fn push(out: &mut Vec<Token>, kind: TokenKind, start: usize, range: Range<usize>) {
    if !range.is_empty() {
        out.push(Token {
            kind,
            range: start + range.start..start + range.end,
        });
    }
}

fn scan_values(line: &str, mut i: usize, start: usize, out: &mut Vec<Token>) {
    while i < line.len() {
        i = skip_ws(line, i);
        if i >= line.len() {
            break;
        }
        let c = next_char(line, i);
        match c {
            '#' => {
                push(out, TokenKind::Comment, start, i..line.len());
                break;
            }
            '"' | '\'' => {
                let mut end = line.len();
                let mut escaped = false;
                for (k, d) in line[i + 1..].char_indices() {
                    if c == '"' && escaped {
                        escaped = false;
                    } else if c == '"' && d == '\\' {
                        escaped = true;
                    } else if d == c {
                        end = i + 1 + k + 1;
                        break;
                    }
                }
                push(out, TokenKind::String, start, i..end);
                i = end;
            }
            '[' | ']' | ',' | '{' | '}' | '=' => {
                push(out, TokenKind::Punct, start, i..i + 1);
                i += 1;
            }
            _ => {
                let end = line[i..]
                    .find([' ', '\t', ',', ']', '[', '#', '"', '\'', '='])
                    .map_or(line.len(), |k| i + k)
                    .max(i + c.len_utf8());
                let word = &line[i..end];
                if line[skip_ws(line, end)..].starts_with('=')
                    && word.chars().all(|d| d.is_ascii_alphanumeric() || d == '_')
                {
                    push(out, TokenKind::Key, start, i..end);
                } else if word == "true" || word == "false" {
                    push(out, TokenKind::Bool, start, i..end);
                } else if word.starts_with(|d: char| d.is_ascii_digit() || matches!(d, '+' | '-' | '.')) {
                    push(out, TokenKind::Number, start, i..end);
                }
                i = end;
            }
        }
    }
}

/// Highlighting tokens; tolerant of broken text.
pub fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        let i = skip_ws(line, 0);
        if i >= line.len() {
            continue;
        }
        if line[i..].starts_with('#') {
            push(&mut out, TokenKind::Comment, start, i..line.len());
        } else if line[i..].starts_with('[') {
            let open = if line[i..].starts_with("[[") { 2 } else { 1 };
            push(&mut out, TokenKind::Punct, start, i..i + open);
            let close = line[i + open..].find(']').map(|k| i + open + k);
            let inner_end = close.unwrap_or_else(|| line[i..].find('#').map_or(line.len(), |k| i + k));
            let inner = &line[i + open..inner_end];
            let lead = inner.len() - inner.trim_start().len();
            let trimmed = inner.trim();
            push(
                &mut out,
                TokenKind::Header,
                start,
                i + open + lead..i + open + lead + trimmed.len(),
            );
            if let Some(c) = close {
                let end = c + if line[c..].starts_with("]]") { 2 } else { 1 };
                push(&mut out, TokenKind::Punct, start, c..end);
                scan_values(line, end, start, &mut out);
            }
        } else if line[i..].starts_with(['{', '}', ']', ',', '"', '\'']) {
            scan_values(line, i, start, &mut out);
        } else {
            let key_end = line[i..]
                .find([' ', '\t', '=', '#'])
                .map_or(line.len(), |k| i + k)
                .max(i + next_char(line, i).len_utf8());
            push(&mut out, TokenKind::Key, start, i..key_end);
            scan_values(line, key_end, start, &mut out);
        }
    }
    out
}

fn in_comment(prefix: &str) -> bool {
    let (mut dq, mut sq, mut escaped) = (false, false, false);
    for c in prefix.chars() {
        if dq && escaped {
            escaped = false;
        } else if dq && c == '\\' {
            escaped = true;
        } else if c == '"' && !sq {
            dq = !dq;
        } else if c == '\'' && !dq {
            sq = !sq;
        } else if c == '#' && !dq && !sq {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq)]
enum Section {
    Top,
    Effect(String),
    Rule,
    Set(String),
}

fn header_section(line: &str) -> Option<Section> {
    let t = line.trim_start_matches([' ', '\t']);
    let rest = t.strip_prefix('[')?;
    if let Some(inner) = rest.strip_prefix('[') {
        let word: String = inner
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        return Some(if word == "when" {
            Section::Rule
        } else {
            Section::Effect(String::new())
        });
    }
    let inner = rest.split(']').next().unwrap_or("");
    let mut parts = inner.split('.').map(str::trim);
    let id: String = parts
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if id == "when" {
        if parts.next() == Some("set") {
            return Some(Section::Set(parts.next().unwrap_or("").to_owned()));
        }
        return Some(Section::Effect(id));
    }
    Some(Section::Effect(id))
}

fn key_of(line: &str) -> Option<String> {
    let t = line.trim_start_matches([' ', '\t']);
    if t.starts_with(['#', '[']) {
        return None;
    }
    let key: String = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let rest = t[key.len()..].trim_start_matches([' ', '\t']);
    (!key.is_empty() && rest.starts_with('=')).then_some(key)
}

/// The table the line at `line_start` belongs to and the keys that table already sets on other lines.
fn table_context(text: &str, line_start: usize) -> (Section, HashSet<String>) {
    let mut tables: Vec<(Section, HashSet<String>)> = vec![(Section::Top, HashSet::new())];
    let mut found = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some(section) = header_section(line) {
            tables.push((section, HashSet::new()));
            if start == line_start {
                found = Some(tables.len() - 1);
            }
        } else if start == line_start {
            found = Some(tables.len() - 1);
        } else if let Some(k) = key_of(line) {
            tables.last_mut().expect("never empty").1.insert(k);
        }
    }
    tables.swap_remove(found.unwrap_or(tables.len() - 1))
}

fn word_end(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    from + rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len())
}

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

fn default_text(p: &Param) -> String {
    let float = !matches!(p.kind, ParamKind::Int { .. });
    match &p.default {
        Value::Number(n) => number(*n, float),
        Value::Bool(b) => b.to_string(),
        Value::Text(t) => format!("\"{t}\""),
        Value::Vector(v) => format!(
            "[{}]",
            v.iter().map(|n| number(*n, true)).collect::<Vec<_>>().join(", ")
        ),
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

#[cfg(test)]
mod tests;
