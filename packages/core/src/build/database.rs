use std::collections::{BTreeMap, HashMap, HashSet};

use swofty_formats::gvdb::Gvdb;

use super::inputs::Inputs;
use super::{GVDB, Pipeline};
use crate::activation::db::{Registry, TrackHost, activate_with};
use crate::activation::grid::grid_size;
use crate::activation::place::{layer_worlds, slot_asset_group, slot_asset_root};
use crate::activation::ui::plan_lists;
use crate::builtin::CORE_ID;
use crate::dbedit::driver_manager;
use crate::multiplayer::fingerprint;
use crate::planner::{Kind, Route, kept_lists_line, stock_removals_line, track_removals_line};
use crate::runtime::{db_edits, runtime_loaded_estimate, runtime_record_room};
use crate::{Error, Result, io_err};

/// What editing the game database leaves for the later stages.
pub(super) struct DbEdits {
    /// How many stock records each addon's ops would remove, which the build keeps.
    pub stock_removals: BTreeMap<String, usize>,
    /// The entrant lists each addon renames or removes, which the build keeps.
    pub kept_lists: BTreeMap<String, Vec<String>>,
    pub waits: Vec<crate::conflicts::Wait>,
    pub udp_ids: crate::udp_ids::UdpIds,
    pub id_aliases: crate::row_ids::IdAliases,
    /// A check gave row ids it did not keep, so its build must not be saved for a later one to reuse.
    pub row_ids_unsaved: bool,
}

impl Pipeline<'_, '_> {
    /// Adds the tracks' rows, applies the addons' database ops, keeps what the game can't load without,
    /// writes the game database or its runtime edits, and places the track files.
    pub(super) fn edit_database(&mut self, inputs: &Inputs) -> Result<DbEdits> {
        let run = self.run;
        let (store, game, profile, checking) = (run.store, run.game, run.profile, run.checking);
        let Self {
            planner,
            runtime,
            built,
            session,
            caches,
            ..
        } = self;
        let fitted = &inputs.tracks;
        let (room_asked, lent_ids, db_src) = (inputs.room_asked, &inputs.lent_ids, &inputs.db_src);
        let driver_picks = &inputs.driver_picks;
        let ui_maps = fitted.ui_maps();
        let resolve = |rel: &str| run.original(rel);
        let ui_lists = if ui_maps.is_empty() {
            Default::default()
        } else {
            plan_lists(&ui_maps, &resolve)?.0
        };
        let mut stock_removals = BTreeMap::new();
        let mut kept_lists = BTreeMap::new();
        let mut waits = Vec::new();
        let mut udp_ids = crate::udp_ids::UdpIds::default();
        let mut id_aliases = crate::row_ids::IdAliases::default();
        let mut row_ids_unsaved = false;
        if !fitted.is_empty() || planner.has_db() || room_asked.is_some() {
            let left_codes = fitted.left_codes();
            let reg = Registry::builtin();
            let shipped = std::fs::read(db_src).map_err(io_err(&db_src))?;
            let mut db = Gvdb::parse(&shipped)?;
            let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
            let row_ids_lock = crate::row_ids::lock(&store.root)?;
            let last = crate::row_ids::last_build(&store.overlays_dir(profile), &built.buildhash, GVDB);
            let (mut row_ids, row_id_warnings) =
                crate::row_ids::Store::load(&store.root, Some(&game.dir), profile, last);
            built.warnings.extend(row_id_warnings);
            built.lines.extend(row_ids.recover(&store.sessions_dir()).lines);
            if let Some(lent) = lent_ids.as_ref().filter(|l| !l.rows.is_empty()) {
                let n = row_ids.lend(lent, run.order).map_err(Error::Resolve)?;
                built.lines.push(format!(
                    "database: {n} added-row id(s) are the room host's, for this session only; row-ids.json keeps yours"
                ));
            }
            let older = crate::tracks::read_older(&store.root, profile);
            // A full track id pool lends the id of a track whose addon this build leaves out (Store::track_stat_id).
            row_ids.building(run.order.iter().cloned());
            let mut track_udp: Vec<(String, String, Option<i64>)> = Vec::new();
            let mut track_ids: Vec<(String, String, i64)> = Vec::new();
            for ((_, pkg, origin), slot) in fitted.kept.iter().zip(&fitted.slots) {
                if fitted.recorded.contains(&origin.addon) {
                    track_udp.push((origin.addon.clone(), pkg.track_code.clone(), None));
                    built.lines.push(format!(
                        "{}: files only; its track record adds its rows in game",
                        pkg.track_code
                    ));
                    continue;
                }
                let names = slot.map(|s| (slot_asset_group(s), format!("{s}.assetgroup")));
                let root = slot_asset_root();
                let route = match &names {
                    Some((asset_group, key)) => TrackHost::Slot {
                        asset_group,
                        asset_root: &root,
                        key,
                    },
                    None => TrackHost::Natural,
                };
                let lists = ui_lists.get(&pkg.track_code).cloned().unwrap_or_default();
                let seed = older
                    .iter()
                    .find(|o| o.addon == origin.addon && o.code == pkg.track_code)
                    .and_then(|o| o.stat_id)
                    .map(i64::from);
                let mut given = None;
                let mut give = |top: i64, held: &dyn Fn(i64) -> bool| {
                    let id = row_ids.track_stat_id(&origin.addon, &pkg.track_code, seed, top, held);
                    given = id.as_ref().ok().map(|(id, _)| *id);
                    id
                };
                let report = activate_with(&mut db, pkg, &reg, Some(&route), &lists, Some(&mut give))?;
                track_udp.push((origin.addon.clone(), pkg.track_code.clone(), given));
                if let Some(id) = given {
                    track_ids.push((origin.addon.clone(), pkg.track_code.clone(), id));
                }
                for l in report.lines {
                    built.lines.push(format!("{}: {l}", pkg.track_code));
                }
                for w in report.warnings {
                    built.warnings.push(format!("{}: {w}", pkg.track_code));
                }
            }
            let tracked: HashSet<u64> = db
                .records
                .iter()
                .map(|r| r.key)
                .filter(|k| !stock.contains(k))
                .collect();
            let shipped_db = Gvdb::parse(&shipped)?;
            let track_changes = match fitted.is_empty() {
                true => Vec::new(),
                false => crate::tracks::changed_records(&db, &shipped_db),
            };
            let staged = (planner.route(Kind::Db) == Route::Runtime && planner.has_db()).then(|| db.clone());
            let mut db_report = crate::ops::DbReport::default();
            planner.keep_entrant_lists(crate::entrant_lists::StockLists::new(&shipped_db));
            planner.keep_row_ids(row_ids);
            let added = planner.apply_db_report(&mut db, &stock, &tracked, &mut db_report)?;
            built.lines.extend(crate::virtual_lists::settle(&mut db, &stock));
            built.lines.extend(crate::rnd_budget::report(&db));
            if let Some(ids) = planner.take_row_ids() {
                if let Some(missed) = ids.lent_missed() {
                    return Err(Error::Resolve(missed));
                }
                id_aliases = ids.id_aliases();
                built.lines.extend(ids.summary());
                if checking {
                    row_ids_unsaved = ids.unsaved();
                } else {
                    ids.save(&store.root)?;
                }
            }
            drop(row_ids_lock);
            built.lines.extend(db_report.lines);
            built.warnings.extend(db_report.warnings);
            built.warnings.extend(planner.take_skipped());
            let id_left_out = planner.take_id_left_out();
            if !id_left_out.is_empty() {
                built.warnings.extend(crate::stdlib::reports::left_out_warning(
                    &BTreeMap::new(),
                    &id_left_out,
                    run.order,
                    &|id: &str| run.name(id),
                ));
            }
            let mut kept: BTreeMap<String, usize> = BTreeMap::new();
            for key in crate::tracks::keep_changes(&mut db, &shipped_db, &track_changes) {
                *kept.entry(run.writer_name(planner, key)).or_default() += 1;
            }
            for (by, n) in kept {
                built.lines.push(format!(
                    "{by}: kept the tracks' changes to {n} database value(s) it sets back to the game's own"
                ));
            }
            if !left_codes.is_empty() {
                let weekends = crate::tracks::restore_weekends(&mut db, &shipped_db, &left_codes)
                    .map_err(|stuck| Error::Resolve(stuck.join("; ")))?;
                built.warnings.extend(weekends);
                built
                    .warnings
                    .extend(crate::tracks::drop_left_out_orders(&mut db, &left_codes));
            }
            let in_game: Vec<&str> = fitted
                .kept
                .iter()
                .filter(|(_, _, o)| fitted.recorded.contains(&o.addon))
                .map(|(_, p, _)| p.track_code.as_str())
                .collect();
            let missing = crate::tracks::keep_track_refs(&mut db, &shipped_db, &in_game, &left_codes);
            if !missing.is_empty() {
                let codes = missing.iter().map(|m| m.track.clone()).collect();
                let adders = crate::tracks::adders(store, &codes);
                let name = |id: &str| run.name(id);
                for m in &missing {
                    let adds = adders
                        .get(&m.track)
                        .filter(|id| !left_codes.contains(&m.track.as_str()) && !run.order.contains(*id));
                    let by = planner.record_writer(m.key).map(name);
                    built
                        .warnings
                        .push(m.line(by.as_deref(), adds.map(|id| name(id)).as_deref()));
                }
            }
            if let Ok(cat) = crate::dbedit::Catalog::open(store, game) {
                for t in crate::dbedit::rows::trim_row_counts(&cat, &mut db) {
                    built.warnings.push(t.line(&run.writer_name(planner, t.key)));
                }
            }
            for c in crate::dbedit::calendar::cut_rounds(&db) {
                built.warnings.push(c.line(&run.writer_name(planner, c.key)));
            }
            for u in crate::heads::keep_wearable(&mut db, &shipped_db) {
                built.warnings.push(u.line(&run.writer_name(planner, u.key)));
            }
            for k in crate::livery_kits::keep_kits(&mut db) {
                built.warnings.push(k.line(&run.writer_name(planner, k.key)));
            }
            stock_removals = planner.take_stock_removals();
            for (id, n) in &stock_removals {
                built.warnings.push(stock_removals_line(&run.name(id), *n));
            }
            for (id, n) in planner.take_track_removals() {
                built.warnings.push(track_removals_line(&run.name(&id), n));
            }
            waits = planner
                .take_waits()
                .into_iter()
                .map(|(addon, on)| crate::conflicts::Wait { addon, on })
                .collect();
            kept_lists = planner.take_kept_lists();
            for (id, lists) in &kept_lists {
                built.warnings.push(kept_lists_line(&run.name(id), lists));
            }
            let mut copies = planner.take_copies();
            for c in &mut copies {
                c.quiet = c.addon == CORE_ID;
            }
            let row_adders =
                crate::stdlib::room::row_adders(copies.iter().map(|c| c.addon.as_str()), CORE_ID);
            built.row_ids = added_row_ids(&db, &copies);
            for (addon, code, id) in &track_ids {
                built
                    .row_ids
                    .entry(addon.clone())
                    .or_default()
                    .insert(crate::row_ids::track_row(code), *id);
            }
            udp_ids = crate::udp_ids::build(&db, &copies);
            for (addon, code, stat_id) in &track_udp {
                udp_ids.add_track(addon, code, *stat_id);
            }
            let side = crate::dbedit::deps::check(&db, &copies, &|id: &str| run.name(id));
            built.warnings.extend(side.warnings);
            built.side_data = side.findings;
            if driver_picks.iter().any(|(_, p)| !p.is_empty()) {
                let why = match run.order.iter().any(|id| id == driver_manager::ID) {
                    true => driver_manager::Why::NotAdded,
                    false => driver_manager::Why::Off,
                };
                let names = driver_manager::names(store);
                built
                    .warnings
                    .extend(driver_manager::warnings(&db, driver_picks, &names, why));
            }
            if !added.is_empty() {
                let found = crate::owned::owned_items(&db, &Gvdb::parse(&shipped)?, &added);
                built.lines.extend(found.lines);
                built.warnings.extend(found.warnings);
                session.owned_items = found.items;
            }
            let grid = grid_size(&db);
            let worlds = layer_worlds(&db);
            let mut on_file = true;
            if let Some(staged) = staged {
                let loaded = runtime_loaded_estimate(staged.records.len());
                let room = runtime_record_room(loaded);
                let known: HashSet<u64> = staged.records.iter().map(|r| r.key).collect();
                let created = db.records.iter().filter(|r| !known.contains(&r.key)).count();
                let route = crate::stdlib::room::route(loaded, created, room_asked);
                match db_edits(&staged, &db) {
                    Ok(_) if !row_adders.is_empty() => built.lines.push(format!(
                        "database: built into the game file; {} add rows to its tables, which the game indexes while it \
                         loads the file and cannot take as runtime edits",
                        row_adders.join(", ")
                    )),
                    Ok(_) if created > room => built.lines.push(format!(
                        "database: built into the game file; the addons add {created} record(s) and the game's \
                         database takes {room} more once it has loaded"
                    )),
                    Ok(edits) if route == crate::stdlib::room::Route::Runtime => {
                        runtime.db(edits);
                        db = staged;
                        on_file = false;
                    }
                    Ok(_) => {}
                    Err(why) => built
                        .warnings
                        .push(format!("database: built into the game file; {why}")),
                }
            }
            if let Some(asked) = room_asked.filter(|_| on_file)
                && let crate::stdlib::room::Route::File { pad_to, short } =
                    crate::stdlib::room::file_route(db.records.len(), asked)
            {
                let added = crate::stdlib::room::pad(&mut db, pad_to);
                if added > 0 || short > 0 {
                    built
                        .lines
                        .push(crate::stdlib::room::line(pad_to, added, asked, short));
                }
            }
            let bytes = db.to_bytes();
            if bytes == shipped {
                built.lines.push("database: unchanged".into());
            } else {
                let out = built.overlay.join(GVDB);
                let dir = out.parent().expect("has parent");
                std::fs::create_dir_all(dir).map_err(io_err(dir))?;
                std::fs::write(&out, bytes).map_err(io_err(&out))?;
                built.plan.data.insert(GVDB.into(), out);
            }
            fitted.place(run, &worlds, grid, planner, built, caches)?;
        }
        Ok(DbEdits {
            stock_removals,
            kept_lists,
            waits,
            udp_ids,
            id_aliases,
            row_ids_unsaved,
        })
    }
}

// Each addon's added rows of top-level tables with a stat id, as `<table>.<row>/<label>` to its id.
pub(super) fn added_row_ids(
    db: &Gvdb,
    copies: &[crate::dbedit::deps::Copy],
) -> BTreeMap<String, BTreeMap<String, i64>> {
    let rows: HashSet<&str> = copies.iter().map(|c| c.row.as_str()).collect();
    let mut wanted: HashMap<u64, (&str, &str, &str)> = HashMap::new();
    for c in copies {
        let Some((table, last)) = c.row.rsplit_once('.') else {
            continue;
        };
        let Some((row, _)) = last.rsplit_once('_') else {
            continue;
        };
        if crate::ops::row_shape(table).contains("_N") {
            continue;
        }
        let field = crate::ops::stat_id_field(table, row);
        wanted.insert(
            swofty_formats::hash::fnv1a64(format!("{}.{field}", c.row)),
            (&c.addon, &c.row, "id"),
        );
        wanted.insert(
            swofty_formats::hash::fnv1a64(format!("{}._metadata_", c.row)),
            (&c.addon, &c.row, "label"),
        );
    }
    let mut found: BTreeMap<&str, (&str, Option<String>, Option<i64>)> = BTreeMap::new();
    for r in &db.records {
        let Some(&(addon, row, what)) = wanted.get(&r.key) else {
            continue;
        };
        let e = found.entry(row).or_insert((addon, None, None));
        match what {
            "label" => e.1 = Some(r.text().into_owned()),
            _ => e.2 = r.text().trim().parse().ok(),
        }
    }
    let mut out: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for (row, (addon, label, id)) in found {
        let (Some(label), Some(id)) = (label, id) else {
            continue;
        };
        if !rows.contains(row) {
            continue;
        }
        let table = row.rsplit_once('_').map_or(row, |(t, _)| t);
        out.entry(addon.to_owned())
            .or_default()
            .insert(format!("{table}/{label}"), id);
    }
    out
}

pub(super) fn with_row_ids(
    mut outputs: BTreeMap<String, fingerprint::Output>,
    row_ids: &BTreeMap<String, BTreeMap<String, i64>>,
) -> BTreeMap<String, fingerprint::Output> {
    for (addon, ids) in row_ids {
        if let Some(o) = outputs.get_mut(addon) {
            o.row_ids = ids.clone();
        }
    }
    outputs
}
