use std::ops::Range;

use crate::config;
use crate::editor::context::word_end;
use crate::{Catalog, Completion, CompletionKind};

pub(super) fn header_completions(
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
