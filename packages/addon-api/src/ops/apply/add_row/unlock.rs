use std::collections::{BTreeMap, HashMap};

use swofty_formats::gvdb::Record;
use swofty_formats::hash::fnv1a64;

use crate::ops::apply::Cx;
use crate::ops::db_ops::{IdFrom, StatIdMode};
use crate::ops::field_names::FieldNames;
use crate::ops::game_db::Db;
use crate::ops::resolve::Row;
use crate::ops::stat_ids::{MovedStatId, UNLOCK_FRESH_FROM, UNLOCK_KINDS, UNLOCK_ROW, UNLOCK_STAT_MAX};
use crate::{Error, Result};

// A row `add-row` adds to a folder of an unlock list.
pub(super) struct UnlockRow<'a> {
    pub(super) list: &'a str,
    pub(super) folder: &'a str,
    pub(super) kind: &'a str,
    pub(super) dst: &'a Row,
    pub(super) dst_path: &'a str,
    pub(super) what: &'a str,
    pub(super) set: &'a BTreeMap<String, String>,
    pub(super) stat_id: Option<StatIdMode>,
}

// The statIds the rows of an unlock list hold, across all its folders and kinds.
struct UnlockIds {
    // Each id and the row holding it, a stock row before an added one, and whether that row is stock.
    held: HashMap<i64, (String, bool)>,
    // The highest id a stock or track row holds, -1 for none.
    stock_top: i64,
}

impl UnlockIds {
    fn read(db: &Db, names: &dyn FieldNames, list: &str, kind: &str) -> Self {
        let mut ids = Self {
            held: HashMap::new(),
            stock_top: -1,
        };
        let extra = (!UNLOCK_KINDS.contains(&kind)).then_some(kind);
        for f in 0..db.instance_count(list, "folder").unwrap_or(0) {
            let folder = format!("{list}.folder_{f}");
            for kind in UNLOCK_KINDS.iter().copied().chain(extra) {
                let Some(rows) = db.instance_count(&folder, kind) else {
                    continue;
                };
                for i in 0..rows {
                    let row = format!("{folder}.{kind}_{i}");
                    let key = fnv1a64(format!("{row}.statId"));
                    let Some(id) = db.text(key).and_then(|v| v.trim().parse::<i64>().ok()) else {
                        continue;
                    };
                    let stock = names.stock(key) || names.track(key);
                    if stock {
                        ids.stock_top = ids.stock_top.max(id);
                    }
                    match ids.held.entry(id) {
                        std::collections::hash_map::Entry::Vacant(v) => {
                            v.insert((row, stock));
                        }
                        std::collections::hash_map::Entry::Occupied(mut o) => {
                            if stock && !o.get().1 {
                                o.insert((row, stock));
                            }
                        }
                    }
                }
            }
        }
        ids
    }

    // Why the row `key` can't take `id`, if it can't.
    fn blocked(
        &self,
        db: &Db,
        names: &dyn FieldNames,
        list: &str,
        key: Option<&str>,
        id: i64,
    ) -> Option<String> {
        if !(0..=UNLOCK_STAT_MAX).contains(&id) {
            return Some(format!("is outside 0 to {UNLOCK_STAT_MAX}"));
        }
        if let Some((row, stock)) = self.held.get(&id) {
            let label = db.text(fnv1a64(format!("{row}._metadata_")));
            let whose = if *stock { "the game's row" } else { "row" };
            return Some(format!(
                "is held by {whose} {row} ({})",
                label.as_deref().unwrap_or("no label")
            ));
        }
        if names.reserved(list, UNLOCK_ROW, id) {
            return Some("is set by a later addon".to_owned());
        }
        let kept = names.row_ids().zip(key);
        if kept.is_some_and(|(k, key)| k.taken(list, UNLOCK_ROW, key, id)) {
            return Some("is kept for another row or held by a record of the core's registry".to_owned());
        }
        None
    }
}

// Gives the unlock row `u` a `statId` of its own when its `set` leaves the id out (unless
// `"stat_id": "keep"`) or sets one another row of the list holds: the id the install keeps for it, else
// the lowest id from UNLOCK_FRESH_FROM (or above the list's stock rows) to UNLOCK_STAT_MAX that no row of
// the list holds, no later addon sets and row-ids.json keeps for no other row, else one kept for an addon
// the build leaves out.
pub(super) fn unlock_stat_id(
    db: &mut Db,
    names: &dyn FieldNames,
    cx: &mut Cx,
    u: &UnlockRow,
    rows: &mut [Record],
) -> Result<()> {
    let Some(i) = rows.iter().position(|r| r.key == u.dst.field("statId")) else {
        return Ok(());
    };
    let source = String::from_utf8_lossy(&rows[i].value).trim().to_owned();
    let asked = u.set.get("statId").map(|v| v.trim().parse::<i64>());
    let ids = UnlockIds::read(db, names, u.list, u.kind);
    let clash = match asked {
        None if u.stat_id == Some(StatIdMode::Keep) => return Ok(()),
        None => None,
        // Not a number, or below 0 (some stock rows hold -1): left as the addon set it.
        Some(Err(_)) => return Ok(()),
        Some(Ok(old)) if old < 0 => return Ok(()),
        Some(Ok(old)) => match ids.held.get(&old) {
            Some(held) => Some((old, held.clone())),
            None => return Ok(()),
        },
    };
    let label = rows
        .iter()
        .find(|r| r.key == u.dst.field("_metadata_"))
        .map(|r| r.text().into_owned());
    let kept = names.row_ids().zip(label.as_deref());
    let claimed = kept.map(|(k, label)| k.claim(u.list, UNLOCK_ROW, &format!("{}/{label}", u.kind)));
    let key = claimed.as_ref().map(|c| c.key.as_str());
    let blocked = |id: i64| ids.blocked(db, names, u.list, key, id);
    let keep = |id: i64, from: IdFrom| {
        if let (Some((k, _)), Some(key)) = (kept, key) {
            k.keep(u.list, UNLOCK_ROW, key, id, from);
        }
    };
    let mut notes = Vec::new();
    let mut from = IdFrom::Next;
    let mut given = None;
    match claimed.as_ref().and_then(|c| c.kept) {
        Some(id) => match blocked(id) {
            None => {
                given = Some(id);
                notes.push("kept from row-ids.json".to_owned());
            }
            Some(why) => {
                notes.push(format!("{id}, kept for it, {why}"));
                from = IdFrom::Moved;
            }
        },
        None => {
            if let Some(id) = claimed
                .as_ref()
                .and_then(|c| c.seed)
                .filter(|id| blocked(*id).is_none())
            {
                keep(id, IdFrom::Seeded);
                given = Some(id);
                notes.push("as the last build gave it".to_owned());
            }
        }
    }
    let new = match given {
        Some(id) => id,
        None => {
            let low = (ids.stock_top + 1).max(0);
            let start = low.max(UNLOCK_FRESH_FROM);
            let picked = (start..=UNLOCK_STAT_MAX)
                .chain(low..start)
                .find(|id| blocked(*id).is_none());
            let id = match picked {
                Some(id) => id,
                None => {
                    let free = |id: i64| {
                        (0..=UNLOCK_STAT_MAX).contains(&id)
                            && !ids.held.contains_key(&id)
                            && !names.reserved(u.list, UNLOCK_ROW, id)
                    };
                    let reclaimed = match (kept, key) {
                        (Some((k, _)), Some(key)) => k.reclaim(u.list, UNLOCK_ROW, key, &free),
                        _ => None,
                    };
                    let Some((id, held_by)) = reclaimed else {
                        return Err(Error::Resolve(format!(
                            "{}: {} has no free statId up to {UNLOCK_STAT_MAX} in any of its folders",
                            u.what, u.list
                        )));
                    };
                    notes.push(format!(
                        "no other id up to {UNLOCK_STAT_MAX} is free; reused from {held_by}, which this build leaves out"
                    ));
                    from = IdFrom::Reused;
                    id
                }
            };
            if from != IdFrom::Reused {
                keep(id, from);
            }
            id
        }
    };
    rows[i].value = new.to_string().into_bytes();
    let note = (!notes.is_empty()).then(|| notes.join("; "));
    let what = u.what;
    let Some((old, (holder, _))) = clash else {
        cx.report.lines.push(match note {
            Some(note) => format!("{what}: statId {new} (the source's is {source}; {note})"),
            None => format!("{what}: statId {new} (the source's is {source})"),
        });
        return Ok(());
    };
    let holder_label = db.text(fnv1a64(format!("{holder}._metadata_")));
    let why = format!(
        "is taken by {holder} ({})",
        holder_label.as_deref().unwrap_or("no label")
    );
    cx.report.lines.push(match note {
        Some(note) => format!("{what}: statId {old} {why}; statId {new} ({note})"),
        None => format!("{what}: statId {old} {why}; statId {new}"),
    });
    db.moved.insert(
        (format!("{}.{}", u.folder, u.kind), old.to_string()),
        new.to_string(),
    );
    cx.report.moved_stat_ids.push(MovedStatId {
        row: u.dst_path.to_owned(),
        field: "statId",
        label: u.set.get("_metadata_").cloned(),
        old,
        new,
        holder: Some(holder),
        holder_label,
        record: None,
    });
    Ok(())
}
