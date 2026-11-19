use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use swofty_formats::gvdb::{Gvdb, Record};

use super::inputs::Inputs;
use super::run::Run;
use super::tracks::Tracks;
use super::{GVDB, Pipeline};
use crate::activation::db::{Registry, TrackHost, activate_with};
use crate::activation::grid::grid_size;
use crate::activation::place::{
    AiTeam, STOCK_AI, ai_teams, difficulty_stand_ins, layer_worlds, own_ai_folders, slot_asset_group,
    slot_asset_root,
};
use crate::activation::ui::{Lists, plan_lists};
use crate::addons::builtin::CORE_ID;
use crate::asset::Kind;
use crate::content::runtime_assets::{db_edits, runtime_loaded_estimate, runtime_record_room};
use crate::dbedit::driver_manager;
use crate::game_db::owned::Added;
use crate::online::multiplayer::fingerprint;
use crate::planner::{Route, kept_lists_line, stock_removals_line, track_removals_line};
use crate::stdlib::deps::Std;
use crate::{Error, Result, io_err};

/// What editing the game database leaves for the later stages.
#[derive(Default)]
pub(super) struct DbEdits {
    /// How many stock records each addon's ops would remove, which the build keeps.
    pub stock_removals: BTreeMap<String, usize>,
    /// The entrant lists each addon renames or removes, which the build keeps.
    pub kept_lists: BTreeMap<String, Vec<String>>,
    pub waits: Vec<crate::profile::conflicts::Wait>,
    pub udp_ids: crate::game_db::udp_ids::UdpIds,
    pub id_aliases: crate::row_ids::IdAliases,
    /// A check gave row ids it did not keep, so its build must not be saved for a later one to reuse.
    pub row_ids_unsaved: bool,
}

/// The game database as the build edits it, next to the file the game ships.
struct Database {
    db: Gvdb,
    shipped: Vec<u8>,
    shipped_db: Gvdb,
    /// The keys of the shipped database's records.
    stock: HashSet<u64>,
}

impl Database {
    fn read(src: &Path) -> Result<Self> {
        let shipped = std::fs::read(src).map_err(io_err(src))?;
        let db = Gvdb::parse(&shipped)?;
        let stock = db.records.iter().map(|r| r.key).collect();
        let shipped_db = Gvdb::parse(&shipped)?;
        Ok(Self {
            db,
            shipped,
            shipped_db,
            stock,
        })
    }

    /// The keys of the records the database holds that the shipped one does not.
    fn added_keys(&self) -> HashSet<u64> {
        self.db
            .records
            .iter()
            .map(|r| r.key)
            .filter(|k| !self.stock.contains(k))
            .collect()
    }
}

/// The track rows the build added: each track's stat id for the UDP ids (none for a track its record adds
/// in game), and the ids the build gave, by addon and track code.
#[derive(Default)]
struct TrackRows {
    udp: Vec<(String, String, Option<i64>)>,
    ids: Vec<(String, String, i64)>,
}

/// What recording the added rows gives the later steps.
struct Recorded {
    /// The addons that add rows to the game's tables, which only the game file can take.
    row_adders: Vec<String>,
    /// The AI teams the addons added by copying another team's row.
    ai_teams: Vec<AiTeam>,
}

/// What applying the addons' database ops gives the later steps.
struct Applied {
    added: Vec<Added>,
    id_aliases: crate::row_ids::IdAliases,
    row_ids_unsaved: bool,
}

impl Pipeline<'_, '_> {
    /// Adds the tracks' rows, applies the addons' database ops, keeps what the game can't load without,
    /// writes the game database or its runtime edits, and places the track files.
    pub(super) fn edit_database(
        &mut self,
        inputs: &Inputs,
        std16: &BTreeMap<String, Std>,
    ) -> Result<DbEdits> {
        let fitted = &inputs.tracks;
        if fitted.is_empty() && !self.planner.has_db() && inputs.room_asked.is_none() {
            return Ok(DbEdits::default());
        }
        let (store, game) = (self.run.store, self.run.game);
        std::thread::scope(|scope| {
            scope.spawn(|| crate::dbedit::Catalog::open(store, game).map(drop));
            self.edit_database_rows(inputs, std16)
        })
    }

    fn edit_database_rows(&mut self, inputs: &Inputs, std16: &BTreeMap<String, Std>) -> Result<DbEdits> {
        let fitted = &inputs.tracks;
        let store = self.run.store;
        let ui_lists = crate::diagnostics::phases::time_part("build.database.ui_lists", || {
            track_ui_lists(self.run, fitted)
        })?;
        let mut db =
            crate::diagnostics::phases::time_part("build.database.read", || Database::read(&inputs.db_src))?;
        let row_ids_lock = crate::row_ids::lock(&store.root)?;
        let mut row_ids = self.row_ids(inputs)?;
        let tracks = crate::diagnostics::phases::time_part("build.database.tracks", || {
            self.add_tracks(&mut db.db, fitted, &ui_lists, &mut row_ids)
        })?;
        let tracked = db.added_keys();
        let track_changes = match fitted.is_empty() {
            true => Vec::new(),
            false => crate::tracks::changed_records(&db.db, &db.shipped_db),
        };
        let staged =
            (self.planner.route(Kind::Db) == Route::Runtime && self.planner.has_db()).then(|| db.db.clone());
        let applied = crate::diagnostics::phases::time_part("build.database.ops", || {
            self.apply_db_ops(&mut db, &tracked, row_ids)
        })?;
        drop(row_ids_lock);
        crate::diagnostics::phases::time_part("build.database.keep", || {
            self.keep_what_the_game_needs(&mut db, &track_changes, inputs, std16)
        })?;
        let mut edits = self.take_db_reports();
        edits.id_aliases = applied.id_aliases;
        edits.row_ids_unsaved = applied.row_ids_unsaved;
        let recorded = crate::diagnostics::phases::time_part("build.database.record", || {
            self.record_rows(&mut db, &applied.added, &tracks, inputs, &mut edits)
        });
        let grid = grid_size(&db.db);
        let worlds = layer_worlds(&db.db);
        crate::diagnostics::phases::time_part("build.database.write", || {
            self.write_database(db, staged, &recorded.row_adders, inputs.room_asked)
        })?;
        crate::diagnostics::phases::time_part("build.database.place", || {
            self.place_tracks(fitted, &worlds, &recorded.ai_teams, grid)
        })?;
        Ok(edits)
    }

    /// The row ids the build gives: the store's, with the room host's lent for this session, and a full
    /// track id pool free to lend the id of a track whose addon this build leaves out.
    fn row_ids(&mut self, inputs: &Inputs) -> Result<crate::row_ids::Store> {
        let run = self.run;
        let (store, profile) = (run.store, run.profile);
        let built = &mut self.built;
        let last = crate::row_ids::last_build(&store.overlays_dir(profile), &built.buildhash, GVDB);
        let (mut row_ids, row_id_warnings) =
            crate::row_ids::Store::load(&store.root, Some(&run.game.dir), profile, last);
        built.warnings.extend(row_id_warnings);
        built.lines.extend(row_ids.recover(&store.sessions_dir()).lines);
        if let Some(lent) = inputs.lent_ids.as_ref().filter(|l| !l.rows.is_empty()) {
            let n = row_ids.lend(lent, run.order).map_err(Error::Resolve)?;
            built.lines.push(format!(
                "database: {n} added-row id(s) are the room host's, for this session only; row-ids.json keeps yours"
            ));
        }
        // A full track id pool lends the id of a track whose addon this build leaves out (Store::track_stat_id).
        row_ids.building(run.order.iter().cloned());
        Ok(row_ids)
    }

    /// Adds each kept track's rows, with the stat id the row id store gives it.
    fn add_tracks(
        &mut self,
        db: &mut Gvdb,
        fitted: &Tracks,
        ui_lists: &Lists,
        row_ids: &mut crate::row_ids::Store,
    ) -> Result<TrackRows> {
        let run = self.run;
        let built = &mut self.built;
        let mut rows = TrackRows::default();
        let reg = Registry::builtin();
        let older = crate::tracks::read_older(&run.store.root, run.profile);
        for ((_, pkg, origin), slot) in fitted.kept.iter().zip(&fitted.slots) {
            if fitted.recorded.contains(&origin.addon) {
                rows.udp
                    .push((origin.addon.clone(), pkg.track_code.clone(), None));
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
            let report = activate_with(db, pkg, &reg, Some(&route), &lists, Some(&mut give))?;
            rows.udp
                .push((origin.addon.clone(), pkg.track_code.clone(), given));
            if let Some(id) = given {
                rows.ids.push((origin.addon.clone(), pkg.track_code.clone(), id));
            }
            for l in report.lines {
                built.lines.push(format!("{}: {l}", pkg.track_code));
            }
            for w in report.warnings {
                built.warnings.push(format!("{}: {w}", pkg.track_code));
            }
        }
        Ok(rows)
    }

    /// Applies the addons' database ops over the tracks' rows, and keeps the row ids they were given.
    fn apply_db_ops(
        &mut self,
        db: &mut Database,
        tracked: &HashSet<u64>,
        row_ids: crate::row_ids::Store,
    ) -> Result<Applied> {
        let run = self.run;
        let Self { planner, built, .. } = self;
        let mut db_report = crate::ops::DbReport::default();
        planner.keep_entrant_lists(crate::game_db::entrant_lists::StockLists::new(&db.shipped_db));
        planner.keep_row_ids(row_ids);
        let added = planner.apply_db_report(&mut db.db, &db.stock, tracked, &mut db_report)?;
        built
            .lines
            .extend(crate::game_db::virtual_lists::settle(&mut db.db, &db.stock));
        built
            .lines
            .extend(crate::game_db::track_layer_overrides::drop_missing(&mut db.db));
        built.lines.extend(crate::game_db::rnd_budget::report(&db.db));
        let mut applied = Applied {
            added,
            id_aliases: Default::default(),
            row_ids_unsaved: false,
        };
        if let Some(ids) = planner.take_row_ids() {
            if let Some(missed) = ids.lent_missed() {
                return Err(Error::Resolve(missed));
            }
            applied.id_aliases = ids.id_aliases();
            built.lines.extend(ids.summary());
            if run.checking {
                applied.row_ids_unsaved = ids.unsaved();
            } else {
                ids.save(&run.store.root)?;
            }
        }
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
        Ok(applied)
    }

    /// Keeps what the game can't load without: the tracks' changes the ops set back, the weekends and
    /// track references of tracks left out, row counts, calendar rounds, wearable heads and livery kits.
    fn keep_what_the_game_needs(
        &mut self,
        db: &mut Database,
        track_changes: &[Record],
        inputs: &Inputs,
        std16: &BTreeMap<String, Std>,
    ) -> Result<()> {
        let run = self.run;
        let (store, game) = (run.store, run.game);
        let Self { planner, built, .. } = self;
        let fitted = &inputs.tracks;
        let left_codes = fitted.left_codes();
        let Database { db, shipped_db, .. } = db;
        let mut kept: BTreeMap<String, usize> = BTreeMap::new();
        for key in crate::tracks::keep_changes(db, shipped_db, track_changes) {
            *kept.entry(run.writer_name(planner, key)).or_default() += 1;
        }
        for (by, n) in kept {
            built.lines.push(format!(
                "{by}: kept the tracks' changes to {n} database value(s) it sets back to the game's own"
            ));
        }
        if !left_codes.is_empty() {
            let weekends = crate::tracks::restore_weekends(db, shipped_db, &left_codes)
                .map_err(|stuck| Error::Resolve(stuck.join("; ")))?;
            built.warnings.extend(weekends);
            built
                .warnings
                .extend(crate::tracks::drop_left_out_orders(db, &left_codes));
        }
        let in_game: Vec<&str> = fitted
            .kept
            .iter()
            .filter(|(_, _, o)| fitted.recorded.contains(&o.addon))
            .map(|(_, p, _)| p.track_code.as_str())
            .collect();
        let missing = crate::tracks::keep_track_refs(db, shipped_db, &in_game, &left_codes);
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
            for t in crate::dbedit::rows::trim_row_counts(&cat, db) {
                built.warnings.push(t.line(&run.writer_name(planner, t.key)));
            }
        }
        for c in crate::dbedit::calendar::cut_rounds(db) {
            built.warnings.push(c.line(&run.writer_name(planner, c.key)));
        }
        for u in crate::game_db::heads::keep_wearable(db, shipped_db) {
            built.warnings.push(u.line(&run.writer_name(planner, u.key)));
        }
        let added_heads = crate::game_db::heads::registry_heads(&store.addons_dir(), run.order, std16);
        for h in crate::game_db::heads::wear_stand_ins(db, shipped_db, &added_heads) {
            built.lines.push(h.line());
        }
        for k in crate::game_db::livery_kits::keep_kits(db) {
            built.warnings.push(k.line(&run.writer_name(planner, k.key)));
        }
        Ok(())
    }

    /// The stock records, track records and entrant lists the ops would remove and the build keeps, and
    /// the addons that wait on others, each with its warning.
    fn take_db_reports(&mut self) -> DbEdits {
        let run = self.run;
        let Self { planner, built, .. } = self;
        let stock_removals = planner.take_stock_removals();
        for (id, n) in &stock_removals {
            built.warnings.push(stock_removals_line(&run.name(id), *n));
        }
        for (id, n) in planner.take_track_removals() {
            built.warnings.push(track_removals_line(&run.name(&id), n));
        }
        let waits = planner
            .take_waits()
            .into_iter()
            .map(|(addon, on)| crate::profile::conflicts::Wait { addon, on })
            .collect();
        let kept_lists = planner.take_kept_lists();
        for (id, lists) in &kept_lists {
            built.warnings.push(kept_lists_line(&run.name(id), lists));
        }
        DbEdits {
            stock_removals,
            kept_lists,
            waits,
            ..DbEdits::default()
        }
    }

    /// Records the rows each addon added: their ids, the UDP ids, the side data they lack, the drivers
    /// the driver picks name and the items they add to a career, and the AI teams they copy from another.
    fn record_rows(
        &mut self,
        db: &mut Database,
        added: &[Added],
        tracks: &TrackRows,
        inputs: &Inputs,
        edits: &mut DbEdits,
    ) -> Recorded {
        let run = self.run;
        let Self {
            planner,
            built,
            session,
            ..
        } = self;
        let mut copies = planner.take_copies();
        for c in &mut copies {
            c.quiet = c.addon == CORE_ID;
        }
        let row_adders = crate::stdlib::room::row_adders(copies.iter().map(|c| c.addon.as_str()), CORE_ID);
        built.row_ids = added_row_ids(&db.db, &copies);
        let ai_teams = ai_teams(
            &db.db,
            &db.shipped_db,
            copies.iter().map(|c| (c.from.as_str(), c.row.as_str())),
        );
        let fitted = &inputs.tracks;
        let row_codes: Vec<&str> = fitted
            .kept
            .iter()
            .filter(|(_, _, o)| !fitted.recorded.contains(&o.addon))
            .map(|(_, p, _)| p.track_code.as_str())
            .collect();
        let own_ai = own_ai_folders(&db.db, &db.shipped_db);
        for (code, folder) in difficulty_stand_ins(&mut db.db, &own_ai, &row_codes) {
            built.lines.push(format!(
                "{code}: AI difficulty for {folder} stands in with the track's {STOCK_AI}"
            ));
        }
        for (addon, code, id) in &tracks.ids {
            built
                .row_ids
                .entry(addon.clone())
                .or_default()
                .insert(crate::row_ids::track_row(code), *id);
        }
        edits.udp_ids = crate::game_db::udp_ids::build(&db.db, &copies);
        for (addon, code, stat_id) in &tracks.udp {
            edits.udp_ids.add_track(addon, code, *stat_id);
        }
        let side = crate::dbedit::deps::check(&db.db, &copies, &|id: &str| run.name(id));
        built.warnings.extend(side.warnings);
        built.side_data = side.findings;
        let driver_picks = &inputs.driver_picks;
        if driver_picks.iter().any(|(_, p)| !p.is_empty()) {
            let why = match run.order.iter().any(|id| id == driver_manager::ID) {
                true => driver_manager::Why::NotAdded,
                false => driver_manager::Why::Off,
            };
            let names = driver_manager::names(run.store);
            built
                .warnings
                .extend(driver_manager::warnings(&db.db, driver_picks, &names, why));
        }
        if !added.is_empty() {
            let found = crate::game_db::owned::owned_items(&db.db, &db.shipped_db, added);
            built.lines.extend(found.lines);
            built.warnings.extend(found.warnings);
            session.owned_items = found.items;
        }
        Recorded { row_adders, ai_teams }
    }

    /// Hands the database edits to the runtime route when they fit there, else writes the game file,
    /// padded to the room asked for.
    fn write_database(
        &mut self,
        db: Database,
        staged: Option<Gvdb>,
        row_adders: &[String],
        room_asked: Option<usize>,
    ) -> Result<()> {
        let Self { runtime, built, .. } = self;
        let Database { mut db, shipped, .. } = db;
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
        Ok(())
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

/// The UI lists each kept track's map adds its pictures to, by track code.
fn track_ui_lists(run: Run, fitted: &Tracks) -> Result<Lists> {
    let ui_maps = fitted.ui_maps();
    if ui_maps.is_empty() {
        return Ok(Lists::default());
    }
    Ok(plan_lists(&ui_maps, &|rel: &str| run.original(rel))?.0)
}
