mod stat_id;
mod unlock;

use std::collections::BTreeMap;

use swofty_formats::gvdb::{Record, instance_count_path};
use swofty_formats::hash::fnv1a64;

use self::stat_id::{StatIdPicker, added_holder};
use self::unlock::{UnlockRow, unlock_stat_id};
use crate::ops::apply::walk::{Walk, walk};
use crate::ops::apply::{AddedRow, Cx};
use crate::ops::db_ops::{CopyMode, StatIdMode};
use crate::ops::field_names::{FieldNames, row_shape};
use crate::ops::game_db::Db;
use crate::ops::resolve::{Base, Row, resolve_path, resolve_row};
use crate::ops::stat_ids::{MovedStatId, stat_id_ceiling, stat_id_field, unlock_list, virtual_lists};
use crate::{Error, Result};

pub fn add_row_name<'a>(table: &str, from: &'a str) -> Option<&'a str> {
    from.strip_prefix(table)
        .and_then(|r| r.strip_prefix('.'))
        .map(|r| r.split(['[', '.']).next().unwrap_or(r))
        .map(|r| {
            r.rsplit_once('_')
                .filter(|(_, n)| n.parse::<u32>().is_ok())
                .map_or(r, |(n, _)| n)
        })
}

pub(crate) struct AddRow<'a> {
    pub(crate) table: &'a str,
    pub(crate) from: &'a str,
    pub(crate) set: &'a BTreeMap<String, String>,
    pub(crate) copy: Option<CopyMode>,
    pub(crate) stat_id: Option<StatIdMode>,
}

fn cut_by_count(key: &str, set: &BTreeMap<String, String>) -> bool {
    set.iter().any(|(k, v)| {
        let Some(stem) = k.strip_suffix("_instancecount_") else {
            return false;
        };
        let Ok(count) = v.trim().parse::<usize>() else {
            return false;
        };
        let (head, word) = match stem.rsplit_once('.') {
            Some((h, w)) => (format!("{h}."), w),
            None => (String::new(), stem),
        };
        let Some(word) = word.strip_prefix('_') else {
            return false;
        };
        key.strip_prefix(&format!("{head}{word}_"))
            .and_then(|r| r.split_once('.'))
            .and_then(|(i, _)| i.parse::<usize>().ok())
            .is_some_and(|i| i >= count)
    })
}

fn preview(keys: &[&String]) -> String {
    let shown: Vec<&str> = keys.iter().take(12).map(|k| k.as_str()).collect();
    match keys.len() > shown.len() {
        true => format!("{} and {} more", shown.join(", "), keys.len() - shown.len()),
        false => shown.join(", "),
    }
}

pub(crate) fn add_row(db: &mut Db, names: &dyn FieldNames, cx: &mut Cx, op: AddRow) -> Result<()> {
    let AddRow {
        table,
        from,
        set,
        copy,
        stat_id,
    } = op;
    let src = resolve_row(db, from)?;
    let Base::Path(path) = resolve_path(db, table)? else {
        return Err(Error::Resolve(format!(
            "{table}: add-row needs the table's real path, not a {{hash}}"
        )));
    };
    let Base::Path(src_path) = &src.base else {
        return Err(Error::Resolve(format!("{from} is not a row of {table}")));
    };
    let row = add_row_name(&path, src_path)
        .ok_or_else(|| Error::Resolve(format!("{from} is not a row of {table}")))?;
    let count_path = instance_count_path(&path, row);
    let counted = db.instance_count(&path, row).ok_or_else(|| {
        Error::Resolve(format!(
            "{count_path}: missing; add-row needs the table's real path"
        ))
    })?;
    let mut n = counted;
    while db
        .get(fnv1a64(format!("{path}.{row}_{n}._metadata_")))
        .is_some_and(|r| !names.stock(r.key))
    {
        n += 1;
    }
    let listed = names.fields(table, row).or_else(|| names.fields(&path, row));
    let mode = copy.unwrap_or(CopyMode::All);
    let mut found = Walk::default();
    walk(db, names, src_path, "", &mut found);
    let dst_path = format!("{path}.{row}_{n}");
    let fields: Vec<String> = match (mode, &listed) {
        (CopyMode::Listed, None) => {
            return Err(Error::Resolve(format!(
                "{table}.{row}: copy listed needs a fields list"
            )));
        }
        (CopyMode::Listed, Some(l)) => l.clone(),
        (CopyMode::All, l) => {
            let mut all = found.keys.clone();
            all.extend(l.iter().flatten().cloned());
            all.into_iter().collect()
        }
    };
    if fields.is_empty() {
        return Err(Error::Resolve(format!("{table}.{row}: field names unknown")));
    }
    if let Some(f) = set.keys().find(|f| !fields.contains(f)) {
        return Err(Error::Resolve(format!("{table}.{row}: unknown field {f}")));
    }
    let what = format!("add-row {dst_path} (from {from})");
    if n > counted {
        cx.report.lines.push(format!(
            "{what}: {path}'s count is {counted} but rows {counted} to {} already hold another addon's records; adds after them and raises the count",
            n - 1
        ));
    }
    if !found.uncatalogued.is_empty() {
        let shapes: Vec<&String> = found.uncatalogued.iter().collect();
        cx.warn(format!(
            "{what}: the key catalogue has no entry for {}; keys only the exe builds at run time may be missing",
            preview(&shapes)
        ))?;
    }
    if let Some(l) = &listed {
        let gaps: Vec<&String> = found
            .keys
            .iter()
            .filter(|k| !l.contains(k) && !cut_by_count(k, set))
            .collect();
        match (mode, gaps.is_empty()) {
            (_, true) => {}
            (CopyMode::Listed, false) => cx.warn(format!(
                "{what}: the fields list for {table}.{row} misses {} key(s) of the source row, not copied: {}",
                gaps.len(),
                preview(&gaps)
            ))?,
            (CopyMode::All, false) => cx.report.lines.push(format!(
                "{what}: copied {} key(s) the fields list for {table}.{row} doesn't name: {}",
                gaps.len(),
                preview(&gaps)
            )),
        }
    }
    let dst = Row {
        base: Base::Path(dst_path.clone()),
    };
    let mut rows = Vec::with_capacity(fields.len());
    let mut reused = 0;
    for field in &fields {
        if let Some(r) = db.get(dst.field(field)) {
            // A stock row a mod left above the count it lowered is never read.
            if !names.stock(r.key) {
                return Err(Error::Resolve(format!("{dst_path}.{field} already exists")));
            }
            reused += 1;
        }
        let s = db
            .get(src.field(field))
            .ok_or_else(|| Error::Resolve(format!("{from}: has no {field}")))?;
        let value = set
            .get(field)
            .map(|v| v.clone().into_bytes())
            .unwrap_or_else(|| s.value.clone());
        rows.push(Record {
            key: dst.field(field),
            tag: s.tag,
            value,
        });
    }
    let stat_key = dst.field("stat_id");
    let indexed = || db.stat_ids(names, &path, row, n).indexed();
    let top_level = row_shape(&dst_path).matches("_N").count() == 1;
    let unlock = unlock_list(&path);
    let fresh = match (stat_id, set.contains_key("stat_id")) {
        _ if unlock.is_some() => false,
        (_, true) | (Some(StatIdMode::Keep), _) => false,
        (Some(StatIdMode::Fresh), false) => true,
        (None, false) => {
            let indexed = indexed();
            if indexed
                && !top_level
                && let Some(r) = rows.iter().find(|r| r.key == stat_key)
            {
                cx.report.lines.push(format!(
                    "{what}: keeps the source's stat_id {} in a nested table; set stat_id or \"stat_id\": \"fresh\" for its own",
                    String::from_utf8_lossy(&r.value)
                ));
            }
            indexed && top_level
        }
    };
    let virtual_row = path == "entrants.entrant_lists"
        && set
            .get("_metadata_")
            .is_some_and(|l| virtual_lists(db).contains(l));
    let ceiling = stat_id_ceiling(&path, row).filter(|_| top_level && !virtual_row);
    let field = stat_id_field(&path, row);
    let kept = names.row_ids().filter(|_| top_level);
    let label = rows
        .iter()
        .find(|r| r.key == dst.field("_metadata_"))
        .map(|r| r.text().into_owned());
    let ids = StatIdPicker {
        db,
        names,
        path: &path,
        row,
        n,
        what: &what,
        field,
        ceiling,
        kept,
        label: label.as_deref(),
    };
    if fresh && let Some(i) = rows.iter().position(|r| r.key == stat_key) {
        let old = String::from_utf8_lossy(&rows[i].value).into_owned();
        let (next, passed) = ids.give()?;
        rows[i].value = next.to_string().into_bytes();
        cx.report.lines.push(match passed {
            Some(why) => format!("{what}: stat_id {next} (the source's is {old}; {why})"),
            None => format!("{what}: stat_id {next} (the source's is {old})"),
        });
    }
    let id_key = dst.field(field);
    let asked = set.get(field).and_then(|v| v.trim().parse::<i64>().ok());
    let moved = match (asked, ceiling) {
        (Some(old), Some(c)) if old > c.max => Some((
            old,
            None,
            None,
            None,
            format!("is above {}, the highest the game reads", c.max),
        )),
        // Stock scenarios share statId 14, but the core keeps an id pool for them.
        (Some(old), _) if top_level && unlock.is_none() && (field != "stat_id" || indexed()) => {
            added_holder(db, names, &path, row, n, old)
                .map(|holder| {
                    let label = db.text(fnv1a64(format!("{holder}._metadata_")));
                    let why = format!(
                        "is taken by {holder} ({})",
                        label.as_deref().unwrap_or("no label")
                    );
                    (old, Some(holder), label, None, why)
                })
                // A career holds the id the core gave a registry record, so the copy takes another.
                .or_else(|| {
                    let record = kept?.registry_record(&path, row, label.as_deref(), old)?;
                    let why = format!("is kept for {record}, a record of the core's registry");
                    Some((old, None, None, Some(record), why))
                })
        }
        _ => None,
    };
    if let Some((old, holder, holder_label, record, why)) = moved
        && let Some(r) = rows.iter_mut().find(|r| r.key == id_key)
    {
        let (new, note) = ids.give()?;
        r.value = new.to_string().into_bytes();
        let label = set.get("_metadata_").cloned();
        cx.report.lines.push(match note {
            Some(note) => format!("{what}: {field} {old} {why}; {field} {new} ({note})"),
            None => format!("{what}: {field} {old} {why}; {field} {new}"),
        });
        db.moved
            .insert((format!("{path}.{row}"), old.to_string()), new.to_string());
        cx.report.moved_stat_ids.push(MovedStatId {
            row: dst_path.clone(),
            field,
            label,
            old,
            new,
            holder,
            holder_label,
            record,
        });
    }
    if let Some(list) = unlock {
        let unlock_row = UnlockRow {
            list,
            folder: &path,
            kind: row,
            dst: &dst,
            dst_path: &dst_path,
            what: &what,
            set,
            stat_id,
        };
        unlock_stat_id(db, names, cx, &unlock_row, &mut rows)?;
    }
    cx.report
        .lines
        .push(format!("{what}: copied {} key(s)", rows.len()));
    if reused > 0 {
        cx.report.lines.push(format!(
            "{what}: reuses {reused} stock key(s) of a row above the table's count"
        ));
    }
    let before = db.generation;
    for r in rows {
        match db.get_mut(r.key) {
            Some(mut old) => old.set(r),
            None => db.push(r),
        }
    }
    cx.report.added_rows.push(AddedRow {
        from: src_path.clone(),
        row: dst_path.clone(),
    });
    db.get_mut(fnv1a64(&count_path))
        .expect("instance_count read it above")
        .set_value((n + 1).to_string().into_bytes());
    db.row_added(names, &path, row, n, &dst_path, before);
    db.added_rows.insert(dst_path);
    Ok(())
}
