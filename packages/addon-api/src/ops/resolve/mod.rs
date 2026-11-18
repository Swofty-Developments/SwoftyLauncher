mod errors;
pub(crate) mod selectors;

use swofty_formats::hash::{fnv1a64, fnv1a64_from};

use self::errors::NO_ROW;
pub use self::errors::{Selector, ids_exhausted, no_row, no_row_selectors};
use crate::ops::game_db::Db;
use crate::ops::stat_ids::stat_id_field;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Base {
    Path(String),
    Hash(u64),
}

impl Base {
    pub(crate) fn key(&self, suffix: &str) -> u64 {
        match self {
            Base::Path(p) => fnv1a64(format!("{p}{suffix}")),
            Base::Hash(h) => fnv1a64_from(*h, suffix),
        }
    }

    pub(crate) fn child(&self, name: &str) -> Base {
        match self {
            Base::Path(p) => Base::Path(format!("{p}.{name}")),
            Base::Hash(h) => Base::Hash(fnv1a64_from(*h, format!(".{name}"))),
        }
    }
}

impl std::fmt::Display for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Base::Path(p) => f.write_str(p),
            Base::Hash(h) => write!(f, "{{{h:016x}}}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) base: Base,
}

impl Row {
    pub(crate) fn field(&self, field: &str) -> u64 {
        self.base.key(&format!(".{field}"))
    }
}

fn parse_base(s: &str) -> Result<Base> {
    if let Some(hex) = s.strip_prefix('{') {
        let (hex, rest) = hex
            .split_once('}')
            .ok_or_else(|| Error::Resolve(format!("bad placeholder {s}")))?;
        let h = u64::from_str_radix(hex, 16).map_err(|_| Error::Resolve(format!("bad placeholder {s}")))?;
        return Ok(if rest.is_empty() {
            Base::Hash(h)
        } else {
            Base::Hash(fnv1a64_from(h, rest))
        });
    }
    Ok(Base::Path(s.to_owned()))
}

fn row_name(base: &Base, idx: usize, row: &str) -> Base {
    match base {
        Base::Path(p) if row.is_empty() => Base::Path(format!("{p}{idx}")),
        Base::Path(p) => Base::Path(format!("{p}.{row}_{idx}")),
        Base::Hash(h) if row.is_empty() => Base::Hash(fnv1a64_from(*h, idx.to_string())),
        Base::Hash(h) => Base::Hash(fnv1a64_from(*h, format!(".{row}_{idx}"))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment<'a> {
    pub name: &'a str,
    pub select: Option<(&'a str, &'a str)>,
}

pub fn first_segment(path: &str) -> Result<(Segment<'_>, Option<&str>)> {
    let end = path.find(['.', '[']).unwrap_or(path.len());
    let (name, rest) = path.split_at(end);
    let Some(inner) = rest.strip_prefix('[') else {
        return Ok((Segment { name, select: None }, rest.strip_prefix('.')));
    };
    let close = inner
        .match_indices(']')
        .map(|(i, _)| i)
        .find(|&i| matches!(inner.as_bytes().get(i + 1), None | Some(b'.')))
        .ok_or_else(|| Error::Resolve(format!("{path}: [ needs a ] at the end of its segment")))?;
    let sel = &inner[..close];
    let (field, want) = sel
        .split_once('=')
        .ok_or_else(|| Error::Resolve(format!("{path}: bad selector [{sel}]")))?;
    let segment = Segment {
        name,
        select: Some((field.trim(), want)),
    };
    Ok((segment, inner[close + 1..].strip_prefix('.')))
}

fn select_row(
    db: &Db,
    table: &Base,
    row: &str,
    field: &str,
    want: &str,
    first_wins: bool,
) -> std::result::Result<(Base, usize), usize> {
    let count = match table {
        Base::Path(t) if !row.is_empty() => db.instance_count(t, row),
        _ => None,
    };
    let want = match table {
        Base::Path(t) if field == stat_id_field(t, row) => db
            .moved
            .get(&(format!("{t}.{row}"), want.to_owned()))
            .map_or(want, String::as_str),
        _ => want,
    };
    #[cfg(test)]
    if db.scan_selectors {
        return scan_row(db, table, row, field, want, first_wins, count);
    }
    match selectors::select(db, table, row, field, count, want) {
        None => Err(0),
        Some((base, 1)) => Ok((base, 1)),
        Some((base, n)) if first_wins => Ok((base, n)),
        Some((_, n)) => Err(n),
    }
}

/// [`select_row`] by reading every row for the one selector.
#[cfg(test)]
fn scan_row(
    db: &Db,
    table: &Base,
    row: &str,
    field: &str,
    want: &str,
    first_wins: bool,
    count: Option<usize>,
) -> std::result::Result<(Base, usize), usize> {
    let mut hits = Vec::new();
    for idx in 0..count.unwrap_or(4096) {
        let candidate = Row {
            base: row_name(table, idx, row),
        };
        match db.text(candidate.field("_metadata_")) {
            Some(_) if db.text(candidate.field(field)).as_deref() == Some(want) => hits.push(candidate.base),
            Some(_) => {}
            None if count.is_none() => break,
            None => {}
        }
    }
    match hits.len() {
        0 => Err(0),
        1 => Ok((hits.remove(0), 1)),
        n if first_wins => Ok((hits.remove(0), n)),
        n => Err(n),
    }
}

pub(crate) fn resolve_path(db: &Db, path: &str) -> Result<Base> {
    resolve_path_with(db, path, true)
}

fn resolve_path_with(db: &Db, path: &str, first_wins: bool) -> Result<Base> {
    if !path.contains('[') {
        return parse_base(path);
    }
    let mut base: Option<Base> = None;
    let mut nested = false;
    let mut rest = Some(path);
    while let Some(p) = rest {
        let (seg, tail) = first_segment(p)?;
        if seg.name.is_empty() {
            return Err(Error::Resolve(format!("{path}: empty segment")));
        }
        base = Some(match (base, seg.select) {
            (None, None) => parse_base(seg.name)?,
            (Some(b), None) => b.child(seg.name),
            (prefix, Some((field, want))) => {
                let (table, row) = match prefix {
                    Some(b) => (b, seg.name),
                    None => (parse_base(seg.name)?, ""),
                };
                match select_row(db, &table, row, field, want, first_wins) {
                    Ok((b, matched)) => {
                        nested = true;
                        if matched > 1 {
                            let upto = &path[..path.len() - tail.map_or(0, |t| t.len() + 1)];
                            db.ties.borrow_mut().push(format!(
                                "{upto}: {matched} rows match; using the first, {}",
                                b.key("")
                            ));
                        }
                        b
                    }
                    Err(n) => {
                        let upto = &path[..path.len() - tail.map_or(0, |t| t.len() + 1)];
                        let what = match n {
                            0 => NO_ROW.to_owned(),
                            n => format!("{n} rows match"),
                        };
                        let scope = match (nested, row) {
                            (false, _) => String::new(),
                            (true, "") => format!(" in {table}"),
                            (true, row) => format!(" in {table}.{row}"),
                        };
                        return Err(Error::Resolve(format!("{upto}: {what}{scope}")));
                    }
                }
            }
        });
        rest = tail;
    }
    base.ok_or_else(|| Error::Resolve(format!("bad path {path}")))
}

pub(crate) fn resolve_row(db: &Db, select: &str) -> Result<Row> {
    Ok(Row {
        base: resolve_path(db, select)?,
    })
}

pub(crate) fn resolve_value(db: &Db, path: &str) -> Result<String> {
    if !path.contains(['.', ']']) {
        return Err(Error::Resolve(format!("bad path {path}")));
    }
    let key = resolve_path(db, path)?.key("");
    db.text(key)
        .ok_or_else(|| Error::Resolve(format!("{path}: no value")))
}

pub(crate) fn resolve_key(db: &Db, key: &str) -> Result<u64> {
    match key.starts_with("0x") || !key.contains('[') {
        true => record_key(key),
        false => Ok(resolve_path_with(db, key, false)?.key("")),
    }
}

pub fn record_key(key: &str) -> Result<u64> {
    match key.strip_prefix("0x") {
        Some(hex) if hex.len() == 16 => {
            u64::from_str_radix(hex, 16).map_err(|_| Error::Resolve(format!("bad record key {key}")))
        }
        Some(_) => Err(Error::Resolve(format!("bad record key {key}"))),
        None => Ok(fnv1a64(key)),
    }
}
