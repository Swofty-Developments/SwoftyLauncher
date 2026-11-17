use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::gvdb::{Record, instance_count_path};
use swofty_formats::hash::fnv1a64;

use crate::ops::apply::{AddedRow, Cx};
use crate::ops::db::{CopyMode, IdFrom, StatIdMode};
use crate::ops::field_names::{ANY_SHAPE, FieldNames, row_shape};
use crate::ops::game_db::Db;
use crate::ops::resolve::{Base, Row, resolve_path, resolve_row};
use crate::ops::stat_ids::{
    MovedStatId, free_stat_id, stat_id_ceiling, stat_id_ceiling_reason, stat_id_field, stat_id_unusable,
    virtual_lists,
};
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

#[derive(Default)]
pub(crate) struct Walk {
    pub(crate) keys: BTreeSet<String>,
    pub(crate) uncatalogued: BTreeSet<String>,
}

pub(crate) fn walk(db: &Db, names: &dyn FieldNames, path: &str, rel: &str, out: &mut Walk) {
    let shape = row_shape(path);
    let entries = match names.row_keys(&shape) {
        Some(e) => e,
        None => {
            out.uncatalogued.insert(shape);
            match names.row_keys(ANY_SHAPE) {
                Some(e) => e,
                None => return,
            }
        }
    };
    for entry in entries {
        let Some(sub) = entry.strip_prefix('+') else {
            if db.get(fnv1a64(format!("{path}.{entry}"))).is_some() {
                out.keys.insert(format!("{rel}{entry}"));
            }
            continue;
        };
        let (head, word) = match sub.rsplit_once('.') {
            Some((h, w)) => (format!("{h}."), w),
            None => (String::new(), sub),
        };
        let container = format!("{path}.{}", head.trim_end_matches('.'));
        let container = container.trim_end_matches('.');
        let count_key = format!("{rel}{head}_{word}_instancecount_");
        let rows = match db.instance_count(container, word) {
            Some(n) => {
                out.keys.insert(count_key);
                n
            }
            None => (0..4096)
                .take_while(|i| {
                    db.get(fnv1a64(format!("{container}.{word}_{i}._metadata_")))
                        .is_some()
                })
                .count(),
        };
        for i in 0..rows {
            walk(
                db,
                names,
                &format!("{container}.{word}_{i}"),
                &format!("{rel}{head}{word}_{i}."),
                out,
            );
        }
    }
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

// The row added earlier that holds `id`, unless a stock row holds it too.
fn added_holder(
    db: &Db,
    names: &dyn FieldNames,
    table: &str,
    row: &str,
    rows: usize,
    id: i64,
) -> Option<String> {
    let field = stat_id_field(table, row);
    let mut holder = None;
    for i in 0..rows {
        let key = fnv1a64(format!("{table}.{row}_{i}.{field}"));
        if db.text(key).and_then(|v| v.trim().parse::<i64>().ok()) != Some(id) {
            continue;
        }
        if names.stock(key) || names.track(key) {
            return None;
        }
        holder.get_or_insert_with(|| format!("{table}.{row}_{i}"));
    }
    holder
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
    let fresh = match (stat_id, set.contains_key("stat_id")) {
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
    let later = |id: i64| names.reserved(&path, row, id) || stat_id_unusable(&path, row, id);
    let held = |id: i64| {
        (0..n).map(|i| format!("{path}.{row}_{i}")).find(|r| {
            db.text(fnv1a64(format!("{r}.{field}")))
                .and_then(|v| v.trim().parse::<i64>().ok())
                == Some(id)
        })
    };
    // The next id above the table's highest, or the lowest free one when that passes the ceiling.
    // A moved id, and any id under the ceiling, passes over the ids later addons set on their rows.
    let next = || -> Result<(i64, Option<String>)> {
        let mut top = db.stat_ids(names, &path, row, n).max.map_or(0, |m| m + 1);
        while later(top) || stat_id_unusable(&path, row, top) {
            top += 1;
        }
        match ceiling {
            Some(c) if top > c.max || c.from.is_some() => free_stat_id(db, names, &path, row, n, c, &later)
                .map(|id| {
                    (
                        id,
                        (top > c.max)
                            .then(|| format!("{top} is above {}, the highest the game reads", c.max)),
                    )
                })
                .ok_or_else(|| {
                    Error::Resolve(format!(
                        "{what}: {path} has no free {} up to {}, the highest the game reads{}",
                        stat_id_field(&path, row),
                        c.max,
                        stat_id_ceiling_reason(&path, row).map_or(String::new(), |r| format!(" ({r})"))
                    ))
                }),
            _ => Ok((top, None)),
        }
    };
    // The id kept for the row when it's still free, else the id the profile's last build gave a row with
    // its label, else the next free id above the table's rows in load order as builds before 1.6.10 gave
    // it (so a fresh install gives a save from then its ids back); a full table takes back an id kept for
    // an addon the build leaves out.
    let give = || -> Result<(i64, Option<String>)> {
        let Some((k, label)) = kept.zip(label.as_deref()) else {
            return next();
        };
        let claimed = k.claim(&path, row, label);
        let key = claimed.key.as_str();
        let unfit = |id: i64| -> Option<String> {
            if let Some(c) = ceiling.filter(|c| id > c.max || id < 0) {
                return Some(format!("is above {}, the highest the game reads", c.max));
            }
            if stat_id_unusable(&path, row, id) {
                return Some("is an id the game reads as another driver or none".to_owned());
            }
            let r = held(id)?;
            let label = db.text(fnv1a64(format!("{r}._metadata_")));
            Some(format!(
                "is held by {r} ({})",
                label.as_deref().unwrap_or("no label")
            ))
        };
        let blocked = |id: i64| -> Option<String> {
            unfit(id)
                .or_else(|| later(id).then(|| "is set by a later addon".to_owned()))
                .or_else(|| {
                    k.taken(&path, row, key, id).then(|| {
                        "is kept for another row or held by a record of the core's registry".to_owned()
                    })
                })
        };
        let mut notes = Vec::new();
        let mut from = IdFrom::Next;
        match claimed.kept {
            Some(id) => match blocked(id) {
                None => return Ok((id, Some("kept from row-ids.json".to_owned()))),
                Some(why) => {
                    notes.push(format!("{id}, kept for it, {why}"));
                    from = IdFrom::Moved;
                }
            },
            None => {
                if let Some(id) = claimed.seed.filter(|id| blocked(*id).is_none()) {
                    k.keep(&path, row, key, id, IdFrom::Seeded);
                    return Ok((id, Some("as the last build gave it".to_owned())));
                }
            }
        }
        let top = db.stat_ids(names, &path, row, n).max.map_or(0, |m| m + 1);
        let picked = (top..=ceiling.map_or(i64::MAX, |c| c.max))
            .find(|id| blocked(*id).is_none())
            .or_else(|| {
                ceiling.and_then(|c| {
                    free_stat_id(db, names, &path, row, n, c, &|id| {
                        later(id) || blocked(id).is_some()
                    })
                })
            })
            .map(|id| (id, IdFrom::Next));
        let id = match picked {
            Some((id, how)) => {
                if from != IdFrom::Moved {
                    from = how;
                }
                id
            }
            None => {
                let free = |id: i64| unfit(id).is_none() && !later(id);
                let Some((id, held_by)) = k.reclaim(&path, row, key, &free) else {
                    return Err(Error::Resolve(format!(
                        "{what}: {path} has no free {field} up to {}, the highest the game reads{}",
                        ceiling.map_or(i64::MAX, |c| c.max),
                        stat_id_ceiling_reason(&path, row).map_or(String::new(), |r| format!(" ({r})"))
                    )));
                };
                let max = ceiling.map_or(String::new(), |c| format!(" up to {}", c.max));
                notes.push(format!(
                    "no other id{max} is free; reused from {held_by}, which this build leaves out"
                ));
                from = IdFrom::Reused;
                id
            }
        };
        k.keep(&path, row, key, id, from);
        Ok((id, (!notes.is_empty()).then(|| notes.join("; "))))
    };
    if fresh && let Some(i) = rows.iter().position(|r| r.key == stat_key) {
        let old = String::from_utf8_lossy(&rows[i].value).into_owned();
        let (next, passed) = give()?;
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
        (Some(old), _) if top_level && (field != "stat_id" || indexed()) => {
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
        let (new, note) = give()?;
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
            Some(old) => *old = r,
            None => db.push(r),
        }
    }
    cx.report.added_rows.push(AddedRow {
        from: src_path.clone(),
        row: dst_path.clone(),
    });
    let count = db
        .get_mut(fnv1a64(&count_path))
        .expect("instance_count read it above");
    count.value = (n + 1).to_string().into_bytes();
    db.row_added(names, &path, row, n, &dst_path, before);
    db.added_rows.insert(dst_path);
    Ok(())
}
