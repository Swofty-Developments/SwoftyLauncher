use std::ops::Range;

use crate::config;
use crate::editor::completions::signal_completion;
use crate::signals::{SIGNALS, signal};
use crate::when::rule_key_doc;
use crate::{Completion, CompletionKind};

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

pub(super) fn rule_value(text: &str, line: &Range<usize>, offset: usize) -> Option<Vec<Completion>> {
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

pub(super) fn rule_key_completions(
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
