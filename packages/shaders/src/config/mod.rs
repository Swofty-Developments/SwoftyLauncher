//! The tolerant `.swfx` parser: the TOML subset a shader config uses, one line at a time.
//!
//! Every problem becomes an error in [`RawDoc::errors`] and parsing goes on with the next line, so
//! one text can report several.

mod tables;
mod values;

use std::ops::Range;

use self::tables::{Cur, entry, header, rule_header};
use crate::text::skip_ws;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Elem>),
    Strs(Vec<StrElem>),
    Tables(Vec<RawInline>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StrElem {
    pub(crate) text: String,
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawInline {
    pub(crate) entries: Vec<RawEntry>,
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Elem {
    pub(crate) num: f64,
    pub(crate) int: bool,
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawValue {
    pub(crate) lit: Lit,
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawEntry {
    pub(crate) key: String,
    pub(crate) key_range: Range<usize>,
    pub(crate) value: RawValue,
    /// The whole line without indentation or comment.
    pub(crate) range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawTable {
    pub(crate) effect: String,
    pub(crate) effect_range: Range<usize>,
    pub(crate) label: Option<String>,
    pub(crate) header: Range<usize>,
    pub(crate) entries: Vec<RawEntry>,
    pub(crate) span: Range<usize>,
    pub(crate) bad: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawRule {
    pub(crate) header: Range<usize>,
    pub(crate) entries: Vec<RawEntry>,
    pub(crate) sets: Vec<RawTable>,
    pub(crate) span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawError {
    pub(crate) range: Range<usize>,
    pub(crate) message: String,
}

#[derive(Debug, Default)]
pub(crate) struct RawDoc {
    pub(crate) top: Vec<RawEntry>,
    pub(crate) tables: Vec<RawTable>,
    pub(crate) rules: Vec<RawRule>,
    pub(crate) errors: Vec<RawError>,
}

fn bare_key(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

struct LineCtx<'a> {
    line: &'a str,
    start: usize,
}

impl LineCtx<'_> {
    fn at(&self, from: usize, to: usize) -> Range<usize> {
        self.start + from..self.start + to
    }
}

/// How many more `[`/`{` than `]`/`}` a line opens outside strings and comments.
fn depth_delta(s: &str) -> i32 {
    let (mut depth, mut quote, mut escaped) = (0, None, false);
    for c in s.chars() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => break,
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

fn looks_like_entry(body: &str) -> bool {
    let key_len = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(body.len());
    key_len > 0 && body[key_len..].trim_start_matches([' ', '\t']).starts_with('=')
}

pub(crate) fn parse(text: &str) -> RawDoc {
    let mut doc = RawDoc::default();
    let mut cur = Cur::Top;
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .scan(0, |off, raw| {
            let at = *off;
            *off += raw.len();
            Some((at, raw))
        })
        .collect();
    let mut n = 0;
    while n < lines.len() {
        let (start, raw) = lines[n];
        n += 1;
        let mut line_end = start + raw.len();
        let first = raw.trim_end_matches(['\n', '\r']);
        let at = skip_ws(first, 0);
        let body = &first[at..];
        if body.is_empty() || body.starts_with('#') {
            continue;
        }
        if !body.starts_with('[') {
            let mut depth = body.find('=').map_or(0, |eq| depth_delta(&body[eq + 1..]));
            while depth > 0 && n < lines.len() {
                let next = lines[n].1.trim_end_matches(['\n', '\r']).trim_start();
                if next.starts_with('[') || looks_like_entry(next) {
                    break;
                }
                depth += depth_delta(next);
                line_end = lines[n].0 + lines[n].1.len();
                n += 1;
            }
        }
        let line = text[start..line_end].trim_end_matches(['\n', '\r']);
        let ctx = LineCtx { line, start };
        if body.starts_with("[[") {
            cur = rule_header(&ctx, at, &mut doc);
        } else if body.starts_with('[') {
            cur = header(&ctx, at, &mut doc);
        } else {
            entry(&ctx, at, cur, &mut doc);
        }
    }
    doc
}

#[cfg(test)]
mod tests;
