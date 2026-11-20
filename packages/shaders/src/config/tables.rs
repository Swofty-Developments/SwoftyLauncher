use std::ops::Range;

use crate::config::values::{trailing, value};
use crate::config::{LineCtx, RawDoc, RawEntry, RawError, RawRule, RawTable, bare_key};

#[derive(Clone, Copy)]
pub(super) enum Cur {
    Top,
    Table(usize),
    Rule(usize),
    Set(usize, usize),
}

fn split_segments(line: &str, from: usize, to: usize) -> Vec<(&str, Range<usize>)> {
    let inner = &line[from..to];
    let mut segments = Vec::new();
    let mut off = from;
    for part in inner.split('.') {
        let lead = part.len() - part.trim_start_matches([' ', '\t']).len();
        let trimmed = part.trim_matches([' ', '\t']);
        segments.push((trimmed, off + lead..off + lead + trimmed.len()));
        off += part.len() + 1;
    }
    segments
}

fn bad_table(doc: &mut RawDoc, ctx: &LineCtx, at: usize, end: usize) -> Cur {
    doc.tables.push(RawTable {
        effect: String::new(),
        effect_range: ctx.at(at, at),
        label: None,
        header: ctx.at(at, end),
        entries: Vec::new(),
        span: ctx.at(at, end),
        bad: true,
    });
    Cur::Table(doc.tables.len() - 1)
}

pub(super) fn rule_header(ctx: &LineCtx, at: usize, doc: &mut RawDoc) -> Cur {
    let line = ctx.line;
    let body_end = line.trim_end().len();
    let inner_start = at + 2;
    let close = line[inner_start..].find("]]").map(|i| inner_start + i);
    let inner_end = close.unwrap_or_else(|| {
        line[inner_start..]
            .find('#')
            .map_or(body_end, |i| inner_start + i)
    });
    let name = line[inner_start..inner_end].trim();
    if name != "when" {
        doc.errors.push(RawError {
            range: ctx.at(at, body_end),
            message: "An array of tables (`[[x]]`) is not supported in a shader config; only `[[when]]` is"
                .into(),
        });
        return bad_table(doc, ctx, at, line.len());
    }
    let end = match close {
        Some(c) => {
            let mut errors = Vec::new();
            trailing(ctx, c + 2, &mut errors);
            doc.errors.append(&mut errors);
            c + 2
        }
        None => {
            doc.errors.push(RawError {
                range: ctx.at(at, inner_end),
                message: "This rule header is missing its `]]`".into(),
            });
            inner_end
        }
    };
    doc.rules.push(RawRule {
        header: ctx.at(at, end),
        entries: Vec::new(),
        sets: Vec::new(),
        span: ctx.at(at, end),
    });
    Cur::Rule(doc.rules.len() - 1)
}

pub(super) fn header(ctx: &LineCtx, at: usize, doc: &mut RawDoc) -> Cur {
    let line = ctx.line;
    let mk = |bad: bool, effect: String, effect_range: Range<usize>, label: Option<String>, close: usize| {
        RawTable {
            effect,
            effect_range,
            label,
            header: ctx.at(at, close),
            entries: Vec::new(),
            span: ctx.at(at, close),
            bad,
        }
    };
    let Some(close) = line[at..].find(']').map(|i| at + i) else {
        doc.errors.push(RawError {
            range: ctx.at(at, line.len()),
            message: "This table header is missing its `]`".into(),
        });
        doc.tables
            .push(mk(true, String::new(), ctx.at(at, at), None, line.len()));
        return Cur::Table(doc.tables.len() - 1);
    };
    let mut errors = Vec::new();
    trailing(ctx, close + 1, &mut errors);
    doc.errors.append(&mut errors);
    let inner = &line[at + 1..close];
    let segments = split_segments(line, at + 1, close);
    let mut bad = false;
    let whole = ctx.at(at, close + 1);
    if inner.contains(['"', '\'']) {
        doc.errors.push(RawError {
            range: whole.clone(),
            message: "A quoted table name is not supported in a shader config".into(),
        });
        bad = true;
    } else if segments.iter().any(|(s, _)| !bare_key(s)) {
        doc.errors.push(RawError {
            range: whole.clone(),
            message: "A table name is letters, digits, `_` and `-`".into(),
        });
        bad = true;
    } else if segments[0].0 == "when" && segments.len() > 1 {
        let ok_shape = segments[1].0 == "set" && (3..=4).contains(&segments.len());
        if !ok_shape {
            doc.errors.push(RawError {
                range: whole.clone(),
                message: "A rule's table is `[when.set.<effect>]` or `[when.set.<effect>.<label>]`".into(),
            });
            bad = true;
        } else if doc.rules.is_empty() {
            doc.errors.push(RawError {
                range: whole.clone(),
                message: format!("`[{}]` must come after a `[[when]]` rule", inner.trim()),
            });
            bad = true;
        } else {
            let r = doc.rules.len() - 1;
            let mut t = mk(
                false,
                segments[2].0.to_owned(),
                segments[2].1.clone(),
                segments.get(3).map(|(s, _)| (*s).to_owned()),
                close + 1,
            );
            t.span = whole;
            doc.rules[r].span.end = t.span.end;
            doc.rules[r].sets.push(t);
            return Cur::Set(r, doc.rules[r].sets.len() - 1);
        }
    } else if segments.len() > 2 {
        doc.errors.push(RawError {
            range: whole.clone(),
            message: "A shader table is `[effect]` or `[effect.label]`".into(),
        });
        bad = true;
    }
    let (effect, er) = segments[0].clone();
    let label = segments.get(1).map(|(s, _)| (*s).to_owned()).filter(|_| !bad);
    let mut t = mk(bad, effect.to_owned(), ctx.at(er.start, er.end), label, close + 1);
    t.span = whole;
    doc.tables.push(t);
    Cur::Table(doc.tables.len() - 1)
}

pub(super) fn entry(ctx: &LineCtx, at: usize, cur: Cur, doc: &mut RawDoc) {
    let line = ctx.line;
    let Some(eq) = line[at..].find('=').map(|i| at + i) else {
        doc.errors.push(RawError {
            range: ctx.at(at, line.trim_end().len()),
            message: "Expected `key = value`".into(),
        });
        return;
    };
    let raw_key = &line[at..eq];
    let key = raw_key.trim_end_matches([' ', '\t']);
    let key_range = ctx.at(at, at + key.len());
    if key.is_empty() {
        doc.errors.push(RawError {
            range: ctx.at(eq, eq + 1),
            message: "Missing the key before `=`".into(),
        });
        return;
    }
    if key.contains('"') || key.contains('\'') {
        doc.errors.push(RawError {
            range: key_range,
            message: "A quoted key is not supported in a shader config".into(),
        });
        return;
    }
    if key.contains('.') {
        doc.errors.push(RawError {
            range: key_range,
            message: "A dotted key is not supported in a shader config".into(),
        });
        return;
    }
    if !bare_key(key) {
        doc.errors.push(RawError {
            range: key_range,
            message: format!("`{key}` is not a valid key"),
        });
        return;
    }
    match value(ctx, eq + 1) {
        Ok((v, end)) => {
            let mut errors = Vec::new();
            trailing(ctx, end, &mut errors);
            doc.errors.append(&mut errors);
            let range = ctx.at(at, end);
            let e = RawEntry {
                key: key.to_owned(),
                key_range: key_range.clone(),
                value: v,
                range: range.clone(),
            };
            let target = match cur {
                Cur::Top => &mut doc.top,
                Cur::Table(i) => {
                    let t = &mut doc.tables[i];
                    t.span.end = range.end;
                    &mut t.entries
                }
                Cur::Rule(r) => {
                    let t = &mut doc.rules[r];
                    t.span.end = range.end;
                    &mut t.entries
                }
                Cur::Set(r, s) => {
                    let rule = &mut doc.rules[r];
                    rule.span.end = range.end;
                    let t = &mut rule.sets[s];
                    t.span.end = range.end;
                    &mut t.entries
                }
            };
            if target.iter().any(|x| x.key == e.key) {
                doc.errors.push(RawError {
                    range: key_range,
                    message: format!("`{key}` is set twice"),
                });
            } else {
                target.push(e);
            }
        }
        Err(e) => doc.errors.push(e),
    }
}
