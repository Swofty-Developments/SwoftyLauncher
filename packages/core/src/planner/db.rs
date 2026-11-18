use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use swofty_formats::gvdb::Gvdb;

use crate::asset::Origin;
use crate::dbedit::deps::Copy;
use crate::game_db::owned::{self, Added};
use crate::game_db::row_keys::{Names, RowKeys};
use crate::ops::{
    DbOps, DbReport, FieldNames, Rewrite, RowIds, apply_db_ops_lenient, explicit_stat_ids, ids_exhausted,
    no_row, no_row_selectors, unlock_list,
};
use crate::{Error, Result};

use super::{Planner, Target, moved_stat_id_line, stat_row};

// Field names, the ids that the addons after this one set on the rows they add, and the install's kept row ids.
struct Reserving<'a> {
    names: Names<'a>,
    later: &'a HashSet<(String, i64)>,
    row_ids: Option<crate::row_ids::Scoped<'a>>,
}

impl FieldNames for Reserving<'_> {
    fn fields(&self, table: &str, row: &str) -> Option<Vec<String>> {
        self.names.fields(table, row)
    }

    fn row_keys(&self, shape: &str) -> Option<&[String]> {
        self.names.row_keys(shape)
    }

    fn stock(&self, key: u64) -> bool {
        self.names.stock(key)
    }

    fn track(&self, key: u64) -> bool {
        self.names.track(key)
    }

    fn reserved(&self, table: &str, row: &str, id: i64) -> bool {
        self.later.contains(&(format!("{table}.{row}"), id))
    }

    fn row_ids(&self) -> Option<&dyn RowIds> {
        self.row_ids.as_ref().map(|s| s as &dyn RowIds)
    }
}

// What every addon's database ops share: field names, stock and track keys, and the kept row ids.
struct Shared<'a> {
    fields: &'a BTreeMap<String, BTreeMap<String, Vec<String>>>,
    stock: &'a HashSet<u64>,
    tracks: &'a HashSet<u64>,
    row_ids: Option<&'a RefCell<crate::row_ids::Store>>,
}

impl<'a> Shared<'a> {
    fn names(&self, addon: &'a str, later: &'a HashSet<(String, i64)>) -> Reserving<'a> {
        static NONE: BTreeMap<String, Vec<String>> = BTreeMap::new();
        Reserving {
            names: Names {
                fields: self.fields.get(addon).unwrap_or(&NONE),
                keys: RowKeys::builtin(),
                stock: Some(self.stock),
                tracks: Some(self.tracks),
            },
            later,
            row_ids: self.row_ids.map(|store| crate::row_ids::Scoped { addon, store }),
        }
    }
}

// An addon's ops waiting for their row, with the error each gave last.
struct Waiting {
    origin: Origin,
    ops: DbOps,
    later: HashSet<(String, i64)>,
    errors: Vec<swofty_addon::Error>,
    since: usize,
}

// How an addon's database file applies: in load order, with the owned row counts the file before it left
// when nothing changed the database since, or again after a later addon added rows.
enum Pass {
    InOrder(Option<owned::Counts>),
    Retry,
}

impl<'a> Planner<'a> {
    pub fn has_db(&self) -> bool {
        !self.db.is_empty()
    }

    /// [`Planner::apply_db_report`] with `db` as the stock database.
    pub fn apply_db(&mut self, db: &mut Gvdb) -> Result<Vec<Added>> {
        let stock = db.records.iter().map(|r| r.key).collect();
        self.apply_db_report(db, &stock, &HashSet::new(), &mut DbReport::default())
    }

    /// Applies every addon's database ops in load order. A `remove-record` of a
    /// key in `stock` is left out unless the op allows it, and one of a key in
    /// `tracks`, the records track activation added, always; see
    /// [`Planner::take_stock_removals`] and [`Planner::take_track_removals`].
    ///
    /// An op whose selector matches no row waits, with the ops after it in its
    /// file that fail, and applies right after the first later addon whose ops
    /// add records, if its row exists then; the addons after that still override
    /// it. One still failing after every addon is skipped or fails the build, as
    /// the addon's other failing ops are.
    pub fn apply_db_report(
        &mut self,
        db: &mut Gvdb,
        stock: &HashSet<u64>,
        tracks: &HashSet<u64>,
        report: &mut DbReport,
    ) -> Result<Vec<Added>> {
        let fields = std::mem::take(&mut self.fields);
        let mut added = Vec::new();
        let pending = std::mem::take(&mut self.db);
        let row_ids = self.row_ids.take();
        if let Some(ids) = &row_ids {
            let mut ids = ids.borrow_mut();
            ids.building(pending.iter().map(|(o, _)| o.addon.clone()));
            ids.plan(
                db,
                stock,
                tracks,
                pending.iter().map(|(o, ops)| (o.addon.as_str(), ops)),
            );
        }
        let mut later = vec![HashSet::new(); pending.len()];
        for k in (1..pending.len()).rev() {
            let mut ids = later[k].clone();
            ids.extend(explicit_stat_ids(&pending[k].1));
            later[k - 1] = ids;
        }
        let shared = Shared {
            fields: &fields,
            stock,
            tracks,
            row_ids: row_ids.as_ref(),
        };
        let mut waiting = Vec::new();
        let mut grew: Vec<String> = Vec::new();
        let mut counted = None;
        for ((origin, ops), later) in pending.into_iter().zip(later) {
            let size = db.records.len();
            let names = shared.names(&origin.addon, &later);
            let (failed, rows, after) =
                self.apply_ops(db, &origin, &ops, &names, report, Pass::InOrder(counted.take()));
            added.extend(rows);
            let since = grew.len();
            let adder = origin.addon.clone();
            waiting.extend(self.wait(origin, &ops, later, failed, since)?);
            counted = Some(after);
            if db.records.len() != size {
                grew.push(adder);
                if !waiting.is_empty() {
                    counted = None;
                }
                self.retry(db, &mut waiting, &shared, report, &mut added, &grew)?;
            }
        }
        self.retry(db, &mut waiting, &shared, report, &mut added, &grew)?;
        for w in waiting {
            self.settle(&w.origin, w.errors, Some(db))?;
        }
        self.row_ids = row_ids;
        Ok(added)
    }

    fn retry(
        &mut self,
        db: &mut Gvdb,
        waiting: &mut Vec<Waiting>,
        shared: &Shared,
        report: &mut DbReport,
        added: &mut Vec<Added>,
        grew: &[String],
    ) -> Result<()> {
        for Waiting {
            origin,
            ops,
            later,
            since,
            ..
        } in std::mem::take(waiting)
        {
            let names = shared.names(&origin.addon, &later);
            let (failed, rows, _) = self.apply_ops(db, &origin, &ops, &names, report, Pass::Retry);
            added.extend(rows);
            if failed.len() < ops.ops.len() {
                for adder in grew[since.min(grew.len())..]
                    .iter()
                    .filter(|a| **a != origin.addon)
                {
                    self.waits.insert((origin.addon.clone(), adder.clone()));
                }
            }
            waiting.extend(self.wait(origin, &ops, later, failed, since)?);
        }
        Ok(())
    }

    // The failures before `failed`'s first selector that matches no row settle now;
    // that op and the failing ops after it wait.
    fn wait(
        &mut self,
        origin: Origin,
        ops: &DbOps,
        later: HashSet<(String, i64)>,
        mut failed: Vec<(usize, swofty_addon::Error)>,
        since: usize,
    ) -> Result<Option<Waiting>> {
        let at = failed.iter().position(|(_, e)| no_row(e)).unwrap_or(failed.len());
        let rest = failed.split_off(at);
        self.settle(&origin, failed.into_iter().map(|(_, e)| e).collect(), None)?;
        if rest.is_empty() {
            return Ok(None);
        }
        let (picked, errors): (Vec<usize>, Vec<_>) = rest.into_iter().unzip();
        let ops = DbOps {
            schema_version: ops.schema_version,
            fields: ops.fields.clone(),
            strict: ops.strict,
            side_data: ops.side_data.clone(),
            ops: picked.into_iter().map(|i| ops.ops[i].clone()).collect(),
        };
        Ok(Some(Waiting {
            origin,
            ops,
            later,
            errors,
            since,
        }))
    }

    // A lenient addon's failures are skipped with a warning; another's first fails the build.
    // An op on a row that only the core adds at the game's start cannot apply in the build: it is skipped with a warning.
    // So is any addon's op on a row of a track the build leaves out because the game's track lists are full.
    // `last` is the database after every addon's ops, once nothing is left to wait for.
    fn settle(
        &mut self,
        origin: &Origin,
        errors: Vec<swofty_addon::Error>,
        last: Option<&Gvdb>,
    ) -> Result<()> {
        let mut errors = errors;
        let no_id = errors.iter().filter(|e| ids_exhausted(e)).count();
        if no_id > 0 || (!errors.is_empty() && self.id_left_out.contains_key(&origin.addon)) {
            *self.id_left_out.entry(origin.addon.clone()).or_default() += no_id;
            self.skipped.extend(errors.iter().map(|e| {
                format!(
                    "{origin}: left out: {}; a change that depends on a row left out is left out too",
                    e.to_string().trim_start_matches("applying ")
                )
            }));
            return Ok(());
        }
        if last.is_some() {
            let (core, rest): (Vec<_>, Vec<_>) = errors
                .into_iter()
                .partition(|e| !self.core_row_adders(e).is_empty());
            for e in &core {
                let adders = self.core_row_adders(e).join(", ");
                self.skipped.push(format!(
                    "{origin}: change skipped: {e}; the core adds that row from {adders}'s records when the game starts, after the build"
                ));
            }
            errors = Vec::new();
            for e in rest {
                let Some(track) = self.left_out_track(&e).map(str::to_owned) else {
                    errors.push(e);
                    continue;
                };
                let name = self.name(&origin.addon).to_owned();
                self.skipped.push(format!(
                    "{name}: change skipped: {e}; it changes track {track}, which is left out of this build because \
                     the game has room for {} tracks. Turn some track addons off to make room for it",
                    crate::tracks::TRACK_LIST_ROOM
                ));
            }
        }
        let lines: Vec<String> = errors
            .iter()
            .map(|e| match last {
                Some(db) if no_row(e) => format!(
                    "{e}, even after every addon's changes{}",
                    self.removed_note(origin, e, db)
                ),
                _ => e.to_string(),
            })
            .collect();
        match self.lenient.get(&origin.addon) {
            Some(name) => {
                let name = name.clone();
                self.skipped
                    .extend(lines.iter().map(|l| format!("{name}: change skipped: {l}")));
                Ok(())
            }
            None => match lines.first() {
                Some(l) => Err(Error::Other(format!("applying {origin}: {l}"))),
                None => Ok(()),
            },
        }
    }

    // What to do about `origin`'s op that matches no row when an addon before it removed or renamed that row; empty when none did.
    fn removed_note(&self, origin: &Origin, e: &swofty_addon::Error, db: &Gvdb) -> String {
        let by = self.removers(origin, e, db);
        let Some((_, first)) = by.first() else {
            return String::new();
        };
        let verb = match by.iter().all(|(_, v)| v == first) {
            true => *first,
            false => "removed or renamed",
        };
        let names: Vec<&str> = by.iter().map(|(id, _)| self.name(id)).collect();
        let list = match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => names.join(""),
        };
        let them = match names.len() {
            1 => "it",
            _ => "them",
        };
        let edits = self.name(&origin.addon);
        format!(
            ": {edits} edits that row, but {list} {verb} it earlier in the load. Turn {list} off, or move {them} after {edits}"
        )
    }

    // The addons before `origin` in load order whose ops removed or renamed a row that `e`'s path selects, in load
    // order, each with what it did: removed the row, renamed it, or gave its name to another row on the way to it.
    fn removers(&self, origin: &Origin, e: &swofty_addon::Error, db: &Gvdb) -> Vec<(String, &'static str)> {
        let Some(mine) = self.ranks.iter().position(|o| o == origin) else {
            return Vec::new();
        };
        let earlier: Vec<&(usize, Rewrite)> = self
            .rewrites
            .iter()
            .filter(|(rank, _)| *rank < mine && self.ranks[*rank].addon != origin.addon)
            .collect();
        let selectors = no_row_selectors(e);
        let mut found: Vec<(usize, &'static str)> = Vec::new();
        for (n, s) in selectors.iter().enumerate() {
            let failing = n + 1 == selectors.len();
            let count_key = s.count_key();
            let counted = |r: &Rewrite| {
                [&r.was, &r.now]
                    .into_iter()
                    .filter_map(|v| v.as_deref()?.trim().parse::<usize>().ok())
                    .max()
            };
            let rows = earlier
                .iter()
                .filter(|(_, r)| Some(r.key) == count_key)
                .filter_map(|(_, r)| counted(r))
                .fold(4096, usize::max);
            let rows: HashSet<u64> = (0..rows).map(|i| s.field_key(i)).collect();
            let is = |v: &Option<String>| v.as_deref() == Some(s.want.as_str());
            for (rank, r) in &earlier {
                let did = match (rows.contains(&r.key), is(&r.was), is(&r.now)) {
                    (true, true, false) if r.now.is_none() => Some("removed"),
                    (true, true, false) => Some("renamed"),
                    (true, false, true) if !failing => Some("renamed"),
                    (false, ..) if failing && Some(r.key) == count_key && hid(r, s, db) => Some("removed"),
                    _ => None,
                };
                if let Some(did) = did {
                    found.push((*rank, did));
                }
            }
        }
        found.sort_by_key(|(rank, _)| *rank);
        let mut out: Vec<(String, &'static str)> = Vec::new();
        for (rank, did) in found {
            let addon = &self.ranks[rank].addon;
            if !out.iter().any(|(a, _)| a == addon) {
                out.push((addon.clone(), did));
            }
        }
        out
    }

    fn apply_ops(
        &mut self,
        db: &mut Gvdb,
        origin: &Origin,
        ops: &DbOps,
        names: &Reserving,
        report: &mut DbReport,
        pass: Pass,
    ) -> (Vec<(usize, swofty_addon::Error)>, Vec<Added>, owned::Counts) {
        let rank = self.rank(origin);
        let (before, guarded) = match pass {
            Pass::InOrder(Some(counted)) => (counted, Vec::new()),
            Pass::InOrder(None) => (owned::counts(db), Vec::new()),
            Pass::Retry => (owned::counts(db), self.later_records(db, origin)),
        };
        let mut ours = DbReport::default();
        let lists = self.lists.as_ref().filter(|l| l.touched(ops));
        let was = lists.map(|l| l.before(db));
        let (replaced, failed) = apply_db_ops_lenient(db, ops, names, &mut ours);
        report
            .lines
            .extend(ours.lines.into_iter().map(|l| format!("{}: {l}", origin.addon)));
        report.warnings.extend(
            ours.warnings
                .into_iter()
                .map(|w| format!("{}: {w}", origin.addon)),
        );
        self.rewrites.extend(ours.rewrites.into_iter().map(|r| (rank, r)));
        if ours.stock_removals > 0 {
            *self.stock_removals.entry(origin.addon.clone()).or_default() += ours.stock_removals;
        }
        if ours.track_removals > 0 {
            *self.track_removals.entry(origin.addon.clone()).or_default() += ours.track_removals;
        }
        if let (Some(lists), Some(was)) = (lists, was) {
            let kept = lists.missing(db, &was);
            if !kept.is_empty() {
                self.kept_lists
                    .entry(origin.addon.clone())
                    .or_default()
                    .extend(kept);
            }
        }
        self.copies.extend(ours.added_rows.into_iter().map(|r| Copy {
            addon: origin.addon.clone(),
            from: r.from,
            row: r.row,
            ignore: ops.side_data.ignore.clone(),
            quiet: false,
        }));
        for m in ours.moved_stat_ids {
            let Some(held) = &m.holder else {
                report.warnings.push(moved_stat_id_line(&origin.addon, None, &m));
                continue;
            };
            let copy = self.copies.iter().rev().find(|c| &c.row == held);
            // An unlock row's id moves off a stock row too; no addon added that row.
            let game = copy.is_none()
                && stat_row(held)
                    .rsplit_once('.')
                    .is_some_and(|(t, _)| unlock_list(t).is_some());
            let holder = copy.map_or_else(|| origin.addon.clone(), |c| c.addon.clone());
            let named = if game { "the game" } else { holder.as_str() };
            report
                .warnings
                .push(moved_stat_id_line(&origin.addon, Some(named), &m));
            if game {
                continue;
            }
            let keeper = Origin {
                addon: holder,
                item: None,
            };
            if keeper.addon != origin.addon {
                let field = m.field;
                let key = swofty_formats::hash::fnv1a64(format!("{held}.{field}"));
                let name = match &m.holder_label {
                    Some(l) => format!("{}[_metadata_={l}].{field}", stat_row(held)),
                    None => format!("{held}.{field}"),
                };
                self.planned.record_names.insert(key, name);
                self.kept(Target::Record(key), &keeper, origin);
            }
        }
        for key in replaced {
            self.write(Target::Record(key), origin, true);
        }
        restore_later_records(db, guarded);
        let after = owned::counts(db);
        (
            failed,
            owned::added_between(&before, &after, &origin.addon),
            after,
        )
    }

    fn later_records(&self, db: &Gvdb, origin: &Origin) -> Vec<(usize, swofty_formats::gvdb::Record)> {
        let rank = |o: &Origin| self.ranks.iter().position(|r| r == o);
        let mine = rank(origin);
        let keys: HashSet<u64> = self
            .writers
            .iter()
            .filter_map(|(t, ws)| match t {
                Target::Record(k) if ws.iter().any(|w| rank(w) > mine) => Some(*k),
                _ => None,
            })
            .collect();
        db.records
            .iter()
            .enumerate()
            .filter(|(_, r)| keys.contains(&r.key))
            .map(|(i, r)| (i, r.clone()))
            .collect()
    }
}

// Whether `r`, a write of `s`'s table's row count, lowered it past a row whose field `s` selects by still has its value.
fn hid(r: &Rewrite, s: &crate::ops::Selector, db: &Gvdb) -> bool {
    let count = |v: &Option<String>| v.as_deref().and_then(|v| v.trim().parse::<usize>().ok());
    let (Some(was), now) = (count(&r.was), count(&r.now).unwrap_or(0)) else {
        return false;
    };
    let hidden: HashSet<u64> = (now..was).map(|i| s.field_key(i)).collect();
    db.records
        .iter()
        .any(|rec| hidden.contains(&rec.key) && rec.text() == s.want.as_str())
}

pub(super) fn restore_later_records(db: &mut Gvdb, guarded: Vec<(usize, swofty_formats::gvdb::Record)>) {
    if guarded.is_empty() {
        return;
    }
    for (was, record) in guarded {
        match db.records.value_mut(record.key) {
            Some((tag, value)) => (*tag, *value) = (record.tag, record.value),
            None => {
                let to = was.min(db.records.len());
                db.records.insert(to, record)
            }
        }
    }
}
