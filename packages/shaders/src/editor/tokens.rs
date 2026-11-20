use std::ops::Range;

use crate::text::skip_ws;
use crate::{Token, TokenKind};

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
