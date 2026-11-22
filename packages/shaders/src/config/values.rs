use crate::config::{Elem, LineCtx, Lit, RawEntry, RawError, RawInline, RawValue, StrElem, bare_key};
use crate::text::skip_ws;

pub(super) fn digits(s: &str) -> Option<&str> {
    let mut prev_us = true;
    let mut any = false;
    for c in s.chars() {
        match c {
            '0'..='9' => {
                prev_us = false;
                any = true;
            }
            '_' if !prev_us => prev_us = true,
            _ => return None,
        }
    }
    (any && !prev_us).then_some(s)
}

/// TOML's integer and float grammar. Returns the value and whether it is an integer.
pub(crate) fn parse_number(tok: &str) -> Option<(f64, bool)> {
    let body = tok.strip_prefix(['+', '-']).unwrap_or(tok);
    let (int_part, rest) = match body.find(['.', 'e', 'E']) {
        Some(i) => (&body[..i], &body[i..]),
        None => (body, ""),
    };
    digits(int_part)?;
    if int_part.len() > 1 && int_part.starts_with('0') {
        return None;
    }
    let clean: String = tok.chars().filter(|&c| c != '_').collect();
    if rest.is_empty() {
        return clean.parse::<i64>().ok().map(|v| (v as f64, true));
    }
    let (frac, exp) = match rest.strip_prefix('.') {
        Some(r) => match r.find(['e', 'E']) {
            Some(i) => (Some(&r[..i]), &r[i..]),
            None => (Some(r), ""),
        },
        None => (None, rest),
    };
    if let Some(f) = frac {
        digits(f)?;
    }
    if !exp.is_empty() {
        digits(exp[1..].strip_prefix(['+', '-']).unwrap_or(&exp[1..]))?;
    }
    clean
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| (v, false))
}

fn skip_all(s: &str, mut i: usize) -> usize {
    loop {
        i = skip_ws(s, i);
        if s[i..].starts_with(['\n', '\r']) {
            i += 1;
        } else if s[i..].starts_with('#') {
            i = s[i..].find('\n').map_or(s.len(), |k| i + k);
        } else {
            return i;
        }
    }
}

fn string_basic(ctx: &LineCtx, open: usize) -> Result<(String, usize), RawError> {
    let line = ctx.line;
    let mut out = String::new();
    let mut chars = line[open + 1..].char_indices();
    while let Some((k, c)) = chars.next() {
        let at = open + 1 + k;
        match c {
            '"' => return Ok((out, at + 1)),
            '\n' | '\r' => break,
            '\\' => {
                let Some((_, e)) = chars.next() else { break };
                match e {
                    'b' => out.push('\u{8}'),
                    't' => out.push('\t'),
                    'n' => out.push('\n'),
                    'f' => out.push('\u{c}'),
                    'r' => out.push('\r'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'u' | 'U' => {
                        let n = if e == 'u' { 4 } else { 8 };
                        let hex: String = chars.by_ref().take(n).map(|(_, c)| c).collect();
                        match u32::from_str_radix(&hex, 16)
                            .ok()
                            .filter(|_| hex.len() == n)
                            .and_then(char::from_u32)
                        {
                            Some(c) => out.push(c),
                            None => {
                                return Err(RawError {
                                    range: ctx.at(at, (at + 2 + hex.len()).min(line.len())),
                                    message: "Invalid unicode escape in string".into(),
                                });
                            }
                        }
                    }
                    other => {
                        return Err(RawError {
                            range: ctx.at(at, at + 1 + other.len_utf8()),
                            message: format!("Unknown escape `\\{other}` in string"),
                        });
                    }
                }
            }
            _ => out.push(c),
        }
    }
    Err(RawError {
        range: ctx.at(open, line.len()),
        message: "This string is not closed on its line".into(),
    })
}

fn number_token(s: &str, from: usize) -> usize {
    s[from..]
        .find([' ', '\t', '\n', '\r', ',', ']', '}', '#'])
        .map_or(s.len(), |i| from + i)
}

fn bad_token(ctx: &LineCtx, from: usize, to: usize) -> RawError {
    let tok = &ctx.line[from..to];
    let message = if tok.starts_with(|c: char| c.is_ascii_digit()) && tok.contains(['-', ':']) {
        format!("Dates and times (`{tok}`) are not supported in a shader config")
    } else if matches!(tok.trim_start_matches(['+', '-']), "inf" | "nan")
        || tok.starts_with("0x")
        || tok.starts_with("0o")
        || tok.starts_with("0b")
    {
        format!("`{tok}` is not supported in a shader config; write a decimal number")
    } else if tok.is_empty() {
        "Expected a value".to_owned()
    } else {
        format!("`{tok}` is not a valid value; use a number, true, false or a quoted string")
    };
    RawError {
        range: ctx.at(from, to.max(from)),
        message,
    }
}

pub(super) fn value(ctx: &LineCtx, from: usize) -> Result<(RawValue, usize), RawError> {
    let line = ctx.line;
    let i = skip_ws(line, from);
    let rest = &line[i..];
    let unsupported = |end: usize, what: &str| RawError {
        range: ctx.at(i, end),
        message: format!("{what} is not supported in a shader config"),
    };
    if rest.starts_with("\"\"\"") || rest.starts_with("'''") {
        return Err(unsupported(line.len(), "A multi-line string"));
    }
    if rest.starts_with('{') {
        return Err(unsupported(line.len(), "An inline table"));
    }
    if rest.starts_with('"') {
        let (s, end) = string_basic(ctx, i)?;
        return Ok((
            RawValue {
                lit: Lit::Str(s),
                range: ctx.at(i, end),
            },
            end,
        ));
    }
    if let Some(inner) = rest.strip_prefix('\'') {
        return match inner.find('\'').filter(|&k| !inner[..k].contains('\n')) {
            Some(k) => {
                let end = i + 1 + k + 1;
                Ok((
                    RawValue {
                        lit: Lit::Str(line[i + 1..end - 1].to_owned()),
                        range: ctx.at(i, end),
                    },
                    end,
                ))
            }
            None => Err(RawError {
                range: ctx.at(i, line.len()),
                message: "This string is not closed on its line".into(),
            }),
        };
    }
    if rest.starts_with('[') {
        return array(ctx, i);
    }
    let end = number_token(line, i);
    let tok = &line[i..end];
    match tok {
        "true" => {
            return Ok((
                RawValue {
                    lit: Lit::Bool(true),
                    range: ctx.at(i, end),
                },
                end,
            ));
        }
        "false" => {
            return Ok((
                RawValue {
                    lit: Lit::Bool(false),
                    range: ctx.at(i, end),
                },
                end,
            ));
        }
        _ => {}
    }
    match parse_number(tok) {
        Some((v, true)) => Ok((
            RawValue {
                lit: Lit::Int(v as i64),
                range: ctx.at(i, end),
            },
            end,
        )),
        Some((v, false)) => Ok((
            RawValue {
                lit: Lit::Float(v),
                range: ctx.at(i, end),
            },
            end,
        )),
        None => Err(bad_token(ctx, i, end)),
    }
}

pub(super) fn array(ctx: &LineCtx, open: usize) -> Result<(RawValue, usize), RawError> {
    let line = ctx.line;
    let mut i = open + 1;
    let mut nums = Vec::new();
    let mut strs = Vec::new();
    let mut tables = Vec::new();
    loop {
        i = skip_all(line, i);
        if i >= line.len() {
            return Err(RawError {
                range: ctx.at(open, (open + 1).min(line.len())),
                message: "This array is not closed with `]`".into(),
            });
        }
        if line[i..].starts_with(']') {
            let lit = if !strs.is_empty() {
                Lit::Strs(strs)
            } else if !tables.is_empty() {
                Lit::Tables(tables)
            } else {
                Lit::Array(nums)
            };
            return Ok((
                RawValue {
                    lit,
                    range: ctx.at(open, i + 1),
                },
                i + 1,
            ));
        }
        let kind_clash = |want_nums: bool, want_strs: bool, want_tables: bool| {
            (!want_nums && !nums.is_empty())
                || (!want_strs && !strs.is_empty())
                || (!want_tables && !tables.is_empty())
        };
        let end;
        if line[i..].starts_with(['"', '\'']) {
            let (v, e) = value(ctx, i)?;
            end = e;
            if kind_clash(false, true, false) {
                return Err(mixed(ctx, i, e));
            }
            if let Lit::Str(text) = v.lit {
                strs.push(StrElem { text, range: v.range });
            }
        } else if line[i..].starts_with('{') {
            let (t, e) = inline_table(ctx, i)?;
            end = e;
            if kind_clash(false, false, true) {
                return Err(mixed(ctx, i, e));
            }
            tables.push(t);
        } else {
            end = number_token(line, i);
            match parse_number(&line[i..end]) {
                Some((num, int)) => {
                    if kind_clash(true, false, false) {
                        return Err(mixed(ctx, i, end));
                    }
                    nums.push(Elem {
                        num,
                        int,
                        range: ctx.at(i, end),
                    });
                }
                None => {
                    let mut e = bad_token(ctx, i, end);
                    if line[i..].starts_with('[') || matches!(&line[i..end], "true" | "false") {
                        e.range = ctx.at(i, end.max(i + 1));
                        e.message =
                            "An array in a shader config holds numbers, strings or `{ }` tables only".into();
                    }
                    return Err(e);
                }
            }
        }
        i = skip_all(line, end);
        if i >= line.len() {
            continue;
        }
        if line[i..].starts_with(',') {
            i += 1;
        } else if !line[i..].starts_with(']') {
            let stop = number_token(line, i).max(i + 1).min(line.len());
            return Err(RawError {
                range: ctx.at(i.min(line.len()), stop),
                message: "Expected `,` or `]` in the array".into(),
            });
        }
    }
}

fn mixed(ctx: &LineCtx, from: usize, to: usize) -> RawError {
    RawError {
        range: ctx.at(from, to.max(from + 1).min(ctx.line.len())),
        message: "An array holds one kind of value: numbers, strings or `{ }` tables".into(),
    }
}

fn inline_table(ctx: &LineCtx, open: usize) -> Result<(RawInline, usize), RawError> {
    let line = ctx.line;
    let mut i = open + 1;
    let mut entries: Vec<RawEntry> = Vec::new();
    loop {
        i = skip_all(line, i);
        if i >= line.len() {
            return Err(RawError {
                range: ctx.at(open, (open + 1).min(line.len())),
                message: "This `{` is not closed with `}`".into(),
            });
        }
        if line[i..].starts_with('}') {
            return Ok((
                RawInline {
                    entries,
                    range: ctx.at(open, i + 1),
                },
                i + 1,
            ));
        }
        let key_end = line[i..]
            .find([' ', '\t', '\n', '\r', '=', ',', '}'])
            .map_or(line.len(), |k| i + k);
        let key = &line[i..key_end];
        if !bare_key(key) {
            return Err(RawError {
                range: ctx.at(i, key_end.max(i + 1).min(line.len())),
                message: "Expected `key = value` in the `{ }` table".into(),
            });
        }
        let eq = skip_ws(line, key_end);
        if !line[eq..].starts_with('=') {
            return Err(RawError {
                range: ctx.at(i, key_end),
                message: format!("Expected `=` after `{key}`"),
            });
        }
        let (v, end) = value(ctx, eq + 1)?;
        if entries.iter().any(|e| e.key == key) {
            return Err(RawError {
                range: ctx.at(i, key_end),
                message: format!("`{key}` is set twice"),
            });
        }
        entries.push(RawEntry {
            key: key.to_owned(),
            key_range: ctx.at(i, key_end),
            range: ctx.at(i, end),
            value: v,
        });
        i = skip_all(line, end);
        if i >= line.len() {
            continue;
        }
        if line[i..].starts_with(',') {
            i += 1;
        } else if !line[i..].starts_with('}') {
            return Err(RawError {
                range: ctx.at(i.min(line.len()), (i + 1).min(line.len())),
                message: "Expected `,` or `}` in the `{ }` table".into(),
            });
        }
    }
}

pub(super) fn trailing(ctx: &LineCtx, from: usize, errors: &mut Vec<RawError>) {
    let i = skip_ws(ctx.line, from);
    if i < ctx.line.len() && !ctx.line[i..].starts_with('#') {
        errors.push(RawError {
            range: ctx.at(i, ctx.line.len()),
            message: "Unexpected text after the value".into(),
        });
    }
}
