pub(crate) mod add_row;
mod walk;

use std::collections::BTreeSet;

use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use crate::ops::apply::add_row::{AddRow, add_row};
use crate::ops::apply::walk::{Walk, walk};
use crate::ops::db_ops::{DbOp, DbOps};
use crate::ops::field_names::{FieldNames, Schema};
use crate::ops::game_db::Db;
use crate::ops::resolve::{resolve_key, resolve_path, resolve_row, resolve_value};
use crate::ops::stat_ids::{MovedStatId, VIRTUAL_LISTS, VIRTUAL_LISTS_TAG, virtual_lists};
use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbReport {
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    /// `remove-record` ops left out because their key is a stock record.
    pub stock_removals: usize,
    /// `remove-record` ops left out because their key is a record a track added.
    pub track_removals: usize,
    /// Each `add-row` applied, in order.
    pub added_rows: Vec<AddedRow>,
    /// Each `add-row` whose `stat_id` a row added earlier held, or the game can't read, in order.
    pub moved_stat_ids: Vec<MovedStatId>,
    /// Each record a `set`, `put-record` or `remove-record` changed, in order.
    pub rewrites: Vec<Rewrite>,
}

/// A record an op changed: its value before and after, as text; `None` when it was missing or was removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub key: u64,
    pub was: Option<String>,
    pub now: Option<String>,
}

fn rewrote(cx: &mut Cx, key: u64, was: Option<&[u8]>, now: Option<&[u8]>) {
    if was != now {
        let text = |v: &[u8]| String::from_utf8_lossy(v).into_owned();
        cx.report.rewrites.push(Rewrite {
            key,
            was: was.map(text),
            now: now.map(text),
        });
    }
}

/// An `add-row` applied: the source row's path and the new row's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedRow {
    pub from: String,
    pub row: String,
}

pub fn apply_db_ops(db: &mut Gvdb, ops: &DbOps, names: &dyn FieldNames) -> Result<Vec<u64>> {
    apply_db_ops_report(db, ops, names, &mut DbReport::default())
}

pub fn apply_db_ops_report(
    db: &mut Gvdb,
    ops: &DbOps,
    names: &dyn FieldNames,
    report: &mut DbReport,
) -> Result<Vec<u64>> {
    let schema = Schema {
        file: &ops.fields,
        names,
    };
    let mut db = Db::new(db);
    let mut cx = Cx {
        strict: ops.strict,
        report,
    };
    let mut replaced = Vec::new();
    for op in &ops.ops {
        let applied = apply(&mut db, op, &schema, &mut cx);
        db.drain_ties(&mut cx)?;
        replaced.extend(applied?);
    }
    db.finish();
    Ok(replaced)
}

/// Like [`apply_db_ops`], but an op that fails is skipped and returned with
/// its error instead of stopping the rest. A failing op changes nothing.
pub fn apply_db_ops_lenient(
    db: &mut Gvdb,
    ops: &DbOps,
    names: &dyn FieldNames,
    report: &mut DbReport,
) -> (Vec<u64>, Vec<(usize, Error)>) {
    let schema = Schema {
        file: &ops.fields,
        names,
    };
    let mut db = Db::new(db);
    let mut cx = Cx {
        strict: ops.strict,
        report,
    };
    let mut replaced = Vec::new();
    let mut failed = Vec::new();
    for (i, op) in ops.ops.iter().enumerate() {
        let applied = apply(&mut db, op, &schema, &mut cx);
        let ties = db.drain_ties(&mut cx);
        match applied.and_then(|key| ties.map(|()| key)) {
            Ok(key) => replaced.extend(key),
            Err(e) => failed.push((i, e)),
        }
    }
    db.finish();
    (replaced, failed)
}

pub fn apply_db_op(db: &mut Gvdb, op: &DbOp, names: &dyn FieldNames) -> Result<Option<u64>> {
    let mut db = Db::new(db);
    let mut report = DbReport::default();
    let mut cx = Cx {
        strict: false,
        report: &mut report,
    };
    let replaced = apply(&mut db, op, names, &mut cx)?;
    db.drain_ties(&mut cx)?;
    db.finish();
    Ok(replaced)
}

/// The keys of each row in `rows` that `db` holds, relative to the row, as `add-row` copies them.
pub fn row_keys(db: &mut Gvdb, names: &dyn FieldNames, rows: &[String]) -> Vec<BTreeSet<String>> {
    let db = Db::new(db);
    rows.iter()
        .map(|row| {
            let mut out = Walk::default();
            walk(&db, names, row, "", &mut out);
            out.keys
        })
        .collect()
}

pub(crate) struct Cx<'r> {
    pub(crate) strict: bool,
    pub(crate) report: &'r mut DbReport,
}

impl Cx<'_> {
    pub(crate) fn warn(&mut self, what: String) -> Result<()> {
        if self.strict {
            return Err(Error::Resolve(format!("{what} (strict)")));
        }
        self.report.warnings.push(what);
        Ok(())
    }
}

fn hex_bytes(key: &str, value: &str) -> Result<Vec<u8>> {
    let bad = || Error::Resolve(format!("{key}: value must be hex bytes"));
    if !value.is_ascii() || !value.len().is_multiple_of(2) {
        return Err(bad());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| bad()))
        .collect()
}

fn list_items(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|v| !v.is_empty())
}

fn edit_list(db: &mut Db, select: &str, field: &str, edit: impl FnOnce(&mut Vec<String>)) -> Result<()> {
    let key = match field.contains('[') {
        true => resolve_path(db, &format!("{select}.{field}"))?.key(""),
        false => resolve_row(db, select)?.field(field),
    };
    let mut r = db
        .get_mut(key)
        .ok_or_else(|| Error::Resolve(format!("{select}.{field}: no such key")))?;
    let mut items: Vec<String> = list_items(&r.text()).map(String::from).collect();
    edit(&mut items);
    r.set_value(items.join(",").into_bytes());
    Ok(())
}

pub(crate) fn apply(db: &mut Db, op: &DbOp, names: &dyn FieldNames, cx: &mut Cx) -> Result<Option<u64>> {
    match op {
        DbOp::Set {
            select,
            field,
            value,
            value_from,
            datasets,
            create,
            tag,
        } => {
            if !datasets.is_empty() {
                return Err(Error::Resolve("dataset-scoped set is not implemented yet".into()));
            }
            let value = match (value, value_from) {
                (Some(v), None) => v.clone(),
                (None, Some(from)) => resolve_value(db, from)?,
                _ => {
                    return Err(Error::Resolve(format!(
                        "{select}.{field}: set needs value or value_from"
                    )));
                }
            };
            let key = match field.contains('[') {
                true => resolve_path(db, &format!("{select}.{field}"))?.key(""),
                false => resolve_row(db, select)?.field(field),
            };
            let value = value.into_bytes();
            let was = match db.get_mut(key) {
                Some(mut r) => Some(r.replace_value(value.clone())),
                None if *create => {
                    let tag =
                        tag.ok_or_else(|| Error::Resolve(format!("{select}.{field}: create needs type")))?;
                    db.push(Record {
                        key,
                        tag,
                        value: value.clone(),
                    });
                    None
                }
                None => return Err(Error::Resolve(format!("{select}.{field}: no such key"))),
            };
            rewrote(cx, key, was.as_deref(), Some(&value));
            Ok(Some(key))
        }
        DbOp::AddRow {
            table,
            from,
            set,
            copy,
            stat_id,
        } => {
            add_row(
                db,
                names,
                cx,
                AddRow {
                    table,
                    from,
                    set,
                    copy: *copy,
                    stat_id: *stat_id,
                },
            )?;
            Ok(None)
        }
        DbOp::RemoveRow { .. } => Err(Error::Resolve("remove-row is not implemented yet".into())),
        DbOp::ListAppend { select, field, value } => {
            edit_list(db, select, field, |items| {
                for v in list_items(value) {
                    if !items.iter().any(|i| i == v) {
                        items.push(v.to_string());
                    }
                }
            })?;
            Ok(None)
        }
        DbOp::ListRemove { select, field, value } => {
            let drop: Vec<&str> = list_items(value).collect();
            edit_list(db, select, field, |items| {
                items.retain(|i| !drop.contains(&i.as_str()))
            })?;
            Ok(None)
        }
        DbOp::PutRecord { key, tag, value } => {
            let value = hex_bytes(key, value)?;
            let k = resolve_key(db, key)?;
            let was = match db.get_mut(k) {
                Some(mut r) => {
                    r.set_tag(*tag);
                    Some(r.replace_value(value.clone()))
                }
                None => {
                    db.push(Record {
                        key: k,
                        tag: *tag,
                        value: value.clone(),
                    });
                    None
                }
            };
            rewrote(cx, k, was.as_deref(), Some(&value));
            Ok(Some(k))
        }
        DbOp::VirtualList { list } => {
            let key = fnv1a64(VIRTUAL_LISTS);
            let mut labels = virtual_lists(db);
            if !labels.iter().any(|l| l == list) {
                labels.push(list.clone());
            }
            let value = labels.join(",").into_bytes();
            match db.get_mut(key) {
                Some(mut r) => r.set_value(value),
                None => db.push(Record {
                    key,
                    tag: VIRTUAL_LISTS_TAG,
                    value,
                }),
            }
            Ok(Some(key))
        }
        DbOp::RemoveRecord {
            key,
            allow_stock_removal,
        } => {
            let k = resolve_key(db, key)?;
            if db.get(k).is_none() {
                cx.report.lines.push(format!("{key}: already removed"));
                return Ok(None);
            }
            // The game reads every stock key and crashes at the missing-key sentinel.
            if !allow_stock_removal && names.stock(k) {
                cx.report.stock_removals += 1;
                return Ok(None);
            }
            // A track's new rows count in their tables, and the game reads them like stock rows.
            if names.track(k) {
                cx.report.track_removals += 1;
                return Ok(None);
            }
            let was = db.get(k).map(|r| r.value.clone());
            db.remove(k);
            rewrote(cx, k, was.as_deref(), None);
            Ok(Some(k))
        }
    }
}
