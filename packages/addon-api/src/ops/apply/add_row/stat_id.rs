use swofty_formats::hash::fnv1a64;

use crate::ops::db_ops::IdFrom;
use crate::ops::field_names::{FieldNames, RowIds};
use crate::ops::game_db::Db;
use crate::ops::stat_ids::{
    StatIdCeiling, free_stat_id, stat_id_ceiling_reason, stat_id_field, stat_id_unusable,
};
use crate::{Error, Result};

// The row added earlier that holds `id`, unless a stock row holds it too.
pub(super) fn added_holder(
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

// What picks the stat_id of the row add-row adds as `{path}.{row}_{n}`.
pub(super) struct StatIdPicker<'a> {
    pub(super) db: &'a Db<'a>,
    pub(super) names: &'a dyn FieldNames,
    pub(super) path: &'a str,
    pub(super) row: &'a str,
    pub(super) n: usize,
    pub(super) what: &'a str,
    pub(super) field: &'static str,
    pub(super) ceiling: Option<StatIdCeiling>,
    pub(super) kept: Option<&'a dyn RowIds>,
    pub(super) label: Option<&'a str>,
}

impl StatIdPicker<'_> {
    fn later(&self, id: i64) -> bool {
        self.names.reserved(self.path, self.row, id) || stat_id_unusable(self.path, self.row, id)
    }

    fn held(&self, id: i64) -> Option<String> {
        let (path, row, field) = (self.path, self.row, self.field);
        (0..self.n).map(|i| format!("{path}.{row}_{i}")).find(|r| {
            self.db
                .text(fnv1a64(format!("{r}.{field}")))
                .and_then(|v| v.trim().parse::<i64>().ok())
                == Some(id)
        })
    }

    // The next id above the table's highest, or the lowest free one when that passes the ceiling.
    // A moved id, and any id under the ceiling, passes over the ids later addons set on their rows.
    pub(super) fn next(&self) -> Result<(i64, Option<String>)> {
        let StatIdPicker {
            db,
            names,
            path,
            row,
            n,
            what,
            ceiling,
            ..
        } = *self;
        let later = |id: i64| self.later(id);
        let mut top = db.stat_ids(names, path, row, n).max.map_or(0, |m| m + 1);
        while later(top) || stat_id_unusable(path, row, top) {
            top += 1;
        }
        match ceiling {
            Some(c) if top > c.max || c.from.is_some() => free_stat_id(db, names, path, row, n, c, &later)
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
                        stat_id_field(path, row),
                        c.max,
                        stat_id_ceiling_reason(path, row).map_or(String::new(), |r| format!(" ({r})"))
                    ))
                }),
            _ => Ok((top, None)),
        }
    }

    // The id kept for the row when it's still free, else the id the profile's last build gave a row with
    // its label, else the next free id above the table's rows in load order as builds before 1.6.10 gave
    // it (so a fresh install gives a save from then its ids back); a full table takes back an id kept for
    // an addon the build leaves out.
    pub(super) fn give(&self) -> Result<(i64, Option<String>)> {
        let StatIdPicker {
            db,
            names,
            path,
            row,
            n,
            what,
            field,
            ceiling,
            kept,
            label,
        } = *self;
        let later = |id: i64| self.later(id);
        let held = |id: i64| self.held(id);
        let next = || self.next();
        let Some((k, label)) = kept.zip(label) else {
            return next();
        };
        let claimed = k.claim(path, row, label);
        let key = claimed.key.as_str();
        let unfit = |id: i64| -> Option<String> {
            if let Some(c) = ceiling.filter(|c| id > c.max || id < 0) {
                return Some(format!("is above {}, the highest the game reads", c.max));
            }
            if stat_id_unusable(path, row, id) {
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
                    k.taken(path, row, key, id).then(|| {
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
                    k.keep(path, row, key, id, IdFrom::Seeded);
                    return Ok((id, Some("as the last build gave it".to_owned())));
                }
            }
        }
        let top = db.stat_ids(names, path, row, n).max.map_or(0, |m| m + 1);
        let picked = (top..=ceiling.map_or(i64::MAX, |c| c.max))
            .find(|id| blocked(*id).is_none())
            .or_else(|| {
                ceiling.and_then(|c| {
                    free_stat_id(db, names, path, row, n, c, &|id| {
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
                let Some((id, held_by)) = k.reclaim(path, row, key, &free) else {
                    return Err(Error::Resolve(format!(
                        "{what}: {path} has no free {field} up to {}, the highest the game reads{}",
                        ceiling.map_or(i64::MAX, |c| c.max),
                        stat_id_ceiling_reason(path, row).map_or(String::new(), |r| format!(" ({r})"))
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
        k.keep(path, row, key, id, from);
        Ok((id, (!notes.is_empty()).then(|| notes.join("; "))))
    }
}
