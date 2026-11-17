mod addons;
mod archives;
mod buildhash;
mod database;
mod failure;
mod files;
mod loader;
mod lock;
mod op_files;
mod originals;
mod picture;
mod registry;
mod run;
mod saved;
mod start;
mod tracks;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use swofty_formats::gvdb::Gvdb;

use self::addons::{Gather, experimental_features};
use self::archives::{
    after_puts, archive_puts, carries_handling, contested_in, keep_menus, merged_later, missing_archives,
    missing_resource, record_merge, slice_merges, stage_erps, stock_holders, stray_asset_group_line,
    stray_asset_groups, ui_merge_notes,
};
use self::buildhash::BuildHash;
use self::database::{added_row_ids, with_row_ids};
use self::files::put_files;
use self::loader::loader_line;
use self::originals::check_originals;
use self::registry::replaced_operations;
use self::run::Run;
use self::saved::Saved;
use self::start::trackless_options;
use self::tracks::{Tracks, record_ui_lists, track_files};
use crate::activation::db::{Registry, TrackHost, activate_with};
use crate::activation::ers::{self, ErsMap, build_ers};
use crate::activation::grid::grid_size;
use crate::activation::handling;
use crate::activation::place::{layer_worlds, slot_asset_group, slot_asset_root};
use crate::activation::ui::{Kept, MapUi, build_ui, plan_lists, stamp as ui_stamp};
use crate::asset::{AssetOp, ItemOp, LangOp, Op, Origin, Source};
use crate::baseline::sha256_file;
use crate::builtin::{CORE_ID, LOADER_ENV, loader_dir, loader_files};
use crate::conflicts::Report;
use crate::dbedit::driver_manager;
use crate::erpbuild::{ErpBuild, Held, reads, writes};
use crate::install::{Game, LOADER_DIR, Plan};
use crate::manifest::relative_inside;
use crate::multiplayer::fingerprint;
use crate::planner::{
    Conflict, Features, Kind, Planner, Route, Target, kept_lists_line, load_features, stock_removals_line,
    track_removals_line, waiting_lines,
};
use crate::resolver::resolve;
use crate::runtime::{
    Prebuilt, Runtime, db_edits, overlay_fits, runtime_loaded_estimate, runtime_record_room,
};
use crate::settings::{OnAddons, SessionMod, SessionSettings};
use crate::store::Store;
use crate::ui::{ERPS as MENU_ERPS, READS as MENU_READS, ui_file};
use crate::{Error, Result, io_err};

pub use self::failure::{named, refused_fix};
pub use self::loader::{Plugins, build_menus, plugin_files, screen_files};
pub use self::lock::{BuildLock, lock};
pub use self::op_files::read_op_file;
pub use self::originals::{original, stock_max_ids};
pub use self::tracks::{TrackPackages, track_packages};

pub const GVDB: &str = "game_package/gamedatabases/gamedatabases.2025.bin";
pub const PLAN_FILE: &str = "plan.json";
pub const SAVED_FILE: &str = "built.json";
const SAVED_VERSION: u32 = 3;
const PACKED_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);
const ARCHIVE_AGE: std::time::Duration = std::time::Duration::from_secs(3 * 24 * 3600);
const ARCHIVE_BYTES: u64 = 10 << 30;
pub const ADDONS_DIR: &str = "addons";
const LOCK_FILE: &str = "build.lock";

#[derive(Debug, Clone, Default)]
pub struct Built {
    pub buildhash: String,
    pub overlay: PathBuf,
    pub order: Vec<String>,
    pub plan: Plan,
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    /// Every side-data finding of the add-row copies, tagged; see [`crate::dbedit::deps::Report`].
    pub side_data: Vec<String>,
    pub settings: Option<SessionSettings>,
    pub conflicts: Vec<Conflict>,
    pub waiting: Vec<Op>,
    pub report: Report,
    /// Each addon's added rows with a stat id, its `track.add` track rows among them, as `<table>.<row>/<label>` to
    /// the id the build gave.
    pub row_ids: BTreeMap<String, BTreeMap<String, i64>>,
    /// The track addons left out because the game's track lists are full.
    pub track_limit: Option<crate::tracks::TrackLimit>,
}

impl Built {
    /// Puts `src` in the plan as the game file `rel` it replaces; a second replacement is an error.
    fn replace(&mut self, rel: String, src: PathBuf) -> Result<()> {
        match self.plan.data.insert(rel.clone(), src) {
            Some(_) => Err(Error::Map(format!("{rel} is replaced twice"))),
            None => Ok(()),
        }
    }

    /// Puts `src` in the plan as the loader file `rel`; a second file there is an error.
    fn place_loader(&mut self, rel: String, src: PathBuf) -> Result<()> {
        match self.plan.loader.insert(rel.clone(), src) {
            Some(_) => Err(Error::Other(format!("{rel} is placed twice"))),
            None => Ok(()),
        }
    }
}

pub fn source_dir(store: &Store, addon: &str, root: crate::asset::Root) -> PathBuf {
    match root {
        crate::asset::Root::Addon => store.addons_dir().join(addon),
        crate::asset::Root::Data => store.addon_data_dir(addon),
        crate::asset::Root::Cache => store.addon_cache_dir(addon),
    }
}

pub fn build(store: &Store, profile: &str, game: &Game) -> Result<Built> {
    build_as(store, profile, game, false)
}

/// The track addons `profile` would lose to the game's full track lists, found by planning the build
/// without installing it; none when too few tracks are on to fill them.
pub fn track_limit(store: &Store, profile: &str, game: &Game) -> Result<Option<crate::tracks::TrackLimit>> {
    let addons = store.registry()?.manifests();
    let p = store.profile(profile)?;
    let order = resolve(&p, &addons)?.order;
    if !crate::tracks::may_fill_track_lists(crate::tracks::added_count(store, &order, &addons)) {
        return Ok(None);
    }
    Ok(build_as(store, profile, game, true)?.track_limit)
}

/// A `checking` build gives addon build hooks a host that lists addons but
/// cannot install, remove, turn on or move them.
pub(crate) fn build_as(store: &Store, profile: &str, game: &Game, checking: bool) -> Result<Built> {
    let registry = store.registry()?;
    let addons = registry.manifests();
    let mut p = store.profile(profile)?;
    let mut skipped = Vec::new();
    for u in store.unreadable_addons() {
        if !addons.contains_key(&u.id) && p.enabled().any(|id| id == u.id) {
            p.set_enabled(&u.id, false)?;
            skipped.push(format!("{} is left out: it {}", u.name, u.reason));
        }
    }
    let lent_settings = crate::row_ids::lent().map(|o| (p.lend(&o), o));
    let resolved = resolve(&p, &addons)?;
    let mut on = OnAddons::new(resolved.order.iter().filter_map(|id| addons.get(id)));
    let mut std16 = match p.is_vanilla() {
        true => BTreeMap::new(),
        false => crate::stdlib::deps::read_all(store, &addons),
    };
    let deps = crate::stdlib::deps::for_order(profile, &resolved.order, &addons, &std16);
    if let Some(stop) = deps.stop_text() {
        return Err(Error::Dependencies(stop));
    }
    skipped.extend(trackless_options(
        store,
        game,
        &p,
        &resolved.order,
        &addons,
        &mut on,
    ));
    let mut built = Built {
        order: resolved.order.clone(),
        warnings: skipped
            .into_iter()
            .chain(resolved.warnings.iter().cloned())
            .collect(),
        plan: Plan {
            profile: profile.into(),
            ..Plan::default()
        },
        ..Built::default()
    };
    if let Some(((ordered, values), lent)) = &lent_settings
        && (*ordered > 0 || *values > 0)
    {
        built.lines.push(format!(
            "launch: using the host's load order and {values} option value(s) for {} addon(s)",
            lent.order
                .iter()
                .chain(lent.options.keys())
                .collect::<BTreeSet<_>>()
                .len()
        ));
    }
    built
        .warnings
        .extend(deps.of(crate::stdlib::deps::Level::Warn).map(|f| f.line()));
    built.lines.extend(
        deps.of(crate::stdlib::deps::Level::Hint)
            .map(|f| format!("registry: {}", f.line())),
    );
    built
        .lines
        .extend(deps.notes.iter().map(|n| format!("registry: {n}")));
    let run = Run {
        store,
        game,
        profile,
        checking,
        addons: &addons,
        order: &resolved.order,
        settings: &p,
        on: &on,
    };
    if !p.is_vanilla() {
        built.warnings.extend(start::career_warnings(run));
    }
    built.report = Report {
        profile: profile.into(),
        built_at: crate::conflicts::now(),
        pairs: Vec::new(),
        order: p.enabled().map(str::to_owned).collect(),
        stock_removals: BTreeMap::new(),
        kept_lists: BTreeMap::new(),
        waits: Vec::new(),
    };
    if p.is_vanilla() {
        built.plan = Plan::vanilla();
        return Ok(built);
    }
    check_originals(&store.root, game)?;
    let _lock = lock(store, profile)?;
    crate::now_doing("reading the addons");
    let mut hash = BuildHash::new();
    let packed = store.root.join("cache").join(crate::cache::PACKED_DIR);
    swofty_formats::erp::cache_compressed(Some(packed.clone()));
    let loader = loader_dir();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    built
        .lines
        .push(loader_line(exe_dir.as_deref(), loader.as_deref()));
    let features = loader
        .as_deref()
        .map(load_features)
        .transpose()?
        .unwrap_or_default();
    let config = store.config();
    let packages: BTreeMap<String, String> = resolved
        .order
        .iter()
        .filter(|id| crate::dbedit::package::is_package(store, id))
        .map(|id| (id.clone(), addons[id].name.clone()))
        .collect();
    let overlay_names = |origin: &Origin, src: &Source| -> Vec<String> {
        src.resolve(&source_dir(store, &origin.addon, src.root))
            .ok()
            .and_then(|p| swofty_formats::erp::ErpFile::open(&p).ok())
            .map(|f| f.erp.resources.into_iter().map(|r| r.name).collect())
            .unwrap_or_default()
    };
    let mipmap_names = |origin: &Origin, src: &Source| -> Vec<String> {
        src.resolve(&source_dir(store, &origin.addon, src.root))
            .map(|p| crate::erpbuild::mipmap_names(&p))
            .unwrap_or_default()
    };
    let mut lenient = packages.clone();
    if let Some(core) = addons.get(CORE_ID) {
        lenient.insert(CORE_ID.to_owned(), core.name.clone());
    }
    let names = resolved
        .order
        .iter()
        .map(|id| (id.clone(), addons[id].name.clone()))
        .collect();
    let mut planner = Planner::new(&features, &config.runtime_assets)
        .build_only(&config.build_assets)
        .lenient(lenient)
        .names(names)
        .overlays(&overlay_names)
        .mipmaps(&mipmap_names);
    if features != Features::default() {
        hash.update(format!("runtime {features:?} {:?};", config.runtime_assets));
        if !config.build_assets.is_empty() {
            hash.update(format!("build {:?};", config.build_assets));
        }
    }
    let mut runtime = Runtime::new(planner.features());
    experimental_features(run, &mut built, &mut hash, &mut runtime);
    let shader_files = picture::grade(run, &mut built, &mut hash, &mut runtime)?;
    let replaced = replaced_operations(&std16, loader_dir().as_deref());
    let gathered = Gather {
        run,
        registry: &registry,
        features: &features,
        packages: &packages,
        replaced: &replaced,
        planner: &mut planner,
        built: &mut built,
        hash: &mut hash,
    }
    .gather()?;
    registry::early_stage(run, &std16, gathered.left_to_core, &mut built, &mut runtime);
    let addons::Gathered {
        outputs,
        driver_picks,
        hud_inputs,
        head_puts,
        ..
    } = gathered;
    if head_puts.pending() {
        let path = run.original(GVDB);
        let stock = crate::heads::labels(&Gvdb::parse(&std::fs::read(&path).map_err(io_err(&path))?)?);
        let added = crate::heads::ops(&head_puts.new_heads(&stock), &stock, head_puts.registered());
        hash.json(&added.ops);
        built
            .lines
            .extend(added.lines.iter().map(|(id, l)| format!("{}: {l}", run.name(id))));
        built.warnings.extend(
            added
                .warnings
                .iter()
                .map(|(id, w)| format!("{}: {w}", run.name(id))),
        );
        for op in added.ops {
            planner.push(op);
        }
    }
    let mut fitted = Tracks::fit(run, &mut planner, &mut built, &mut std16)?;
    let mods: Vec<SessionMod> = planner
        .take_items()
        .into_iter()
        .filter_map(|op| match op.op {
            AssetOp::Item(ItemOp::Show {
                id,
                name,
                author,
                version,
                description,
            }) => Some(SessionMod {
                id,
                name,
                version,
                author,
                description,
            }),
            _ => None,
        })
        .collect();
    fitted.take_slots(&planner, exe_dir.as_deref(), loader.is_some())?;
    fitted.hash(&mut hash)?;
    let resolve = |rel: &str| run.original(rel);
    let addon_dir = |id: &str| run.addon_dir(id);
    let (handling_maps, handling_notes) = {
        let dirs: Vec<(String, PathBuf)> = resolved
            .order
            .iter()
            .map(|id| (id.clone(), addon_dir(id)))
            .collect();
        let options = |addon: &str, name: &str| {
            let m = addons.get(addon)?;
            m.options
                .iter()
                .find(|o| o.id == name)
                .map(|o| on.value(&p, m, o))
        };
        handling::read_maps(&dirs, &options)
    };
    built
        .warnings
        .extend(handling_notes.iter().map(|n| format!("handling: {n}")));
    if !handling_maps.is_empty() {
        let sources = handling::sources(&handling_maps, &resolve, &addon_dir);
        hash.update(handling::stamp(&handling_maps, &sources)?);
    }
    let ui_maps: Vec<MapUi> = fitted.ui_maps();
    if !ui_maps.is_empty() {
        hash.update(ui_stamp(&ui_maps, &resolve)?);
    }
    let record_ui = record_ui_lists(&ui_maps, &fitted, &resolve, &mut built.warnings);
    let mut caches = Vec::new();
    let mut session = match &lent_settings {
        Some(_) => store.session_settings_of(&p)?,
        None => store.session_settings(profile)?,
    };
    session.set_maps(
        fitted
            .kept
            .iter()
            .map(|(_, pkg, origin)| (pkg, origin.addon.as_str())),
    );
    session.set_mods(mods.into_iter());
    hash.json(&session);
    let menus = loader_dir().is_some();
    if menus {
        for rel in MENU_ERPS.iter().chain(MENU_READS) {
            let p = resolve(&ui_file(rel));
            let meta = std::fs::metadata(&p).map_err(io_err(&p))?;
            hash.update(format!("{rel}|{}|{:?}", meta.len(), meta.modified().ok()));
        }
    }
    let db_src = run.original(GVDB);
    hash.update(sha256_file(&db_src)?);
    let order = run.manifests();
    hash.unread(store, game, &order, planner.erp_ops(), &resolve)?;
    let std_index = loader_dir()
        .is_some()
        .then(|| {
            crate::stdlib::index::Index::plan(profile, &resolved.order, &addons, &std16, &store.addons_dir())
        })
        .flatten();
    if let Some(placed) = &std_index {
        hash.update(crate::stdlib::index::Index::stamp(placed)?);
        for id in &resolved.order {
            for path in std16.get(id).into_iter().flat_map(|s| &s.files.missing) {
                built.warnings.push(format!(
                    "{}: its registry data names {path}, which isn't a file or folder inside the addon; the game \
                     reports it missing",
                    addons[id].name
                ));
            }
        }
    }
    let room_asked = std16
        .values()
        .any(|s| resolved.order.contains(&s.id) && s.is_std())
        .then(|| {
            crate::stdlib::room::asked(
                crate::stdlib::reports::Room::read(&store.root, Some(&game.dir)).as_ref(),
                profile,
                &resolved.order,
            )
        })
        .flatten();
    if let Some(asked) = room_asked {
        hash.update(format!("room {asked};"));
    }
    let core_left_out = crate::stdlib::reports::Conflicts::read(&store.root, Some(&game.dir))
        .map(|c| c.left_out)
        .unwrap_or_default();
    let core_warning = crate::stdlib::reports::left_out_warning(
        &core_left_out,
        &BTreeMap::new(),
        &resolved.order,
        &|id: &str| run.name(id),
    );
    let lent_ids = crate::row_ids::lent().filter(|o| o.stamp(&resolved.order).is_some());
    if let Some(stamp) = lent_ids.as_ref().and_then(|o| o.stamp(&resolved.order)) {
        hash.update(stamp);
    }
    built.buildhash = hash.finish();
    built.overlay = store.overlay_dir(profile, &built.buildhash);
    built.warnings.extend(core_warning);
    let (lines_at, warnings_at) = (built.lines.len(), built.warnings.len());
    if let Some(saved) = Saved::load(&built.overlay, &built.buildhash).filter(|s| s.usable(store, game)) {
        saved.restore(&mut built)?;
        built.report.order = p.enabled().map(str::to_owned).collect();
        crate::write_json(&crate::conflicts::path(store, profile), &built.report)?;
        if !checking {
            fingerprint::record_outputs(store, profile, with_row_ids(outputs, &built.row_ids)).ok();
            track_files(store, game, &mut built, &fitted, &record_ui);
        }
        return Ok(built);
    }
    let cancel = || run.cancel();
    cancel()?;
    std::fs::remove_dir_all(&built.overlay).ok();
    crate::now_doing("editing the game database");
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
        let shipped = std::fs::read(&db_src).map_err(io_err(&db_src))?;
        let mut db = Gvdb::parse(&shipped)?;
        let stock: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
        let row_ids_lock = crate::row_ids::lock(&store.root)?;
        let last = crate::row_ids::last_build(&store.overlays_dir(profile), &built.buildhash, GVDB);
        let (mut row_ids, row_id_warnings) =
            crate::row_ids::Store::load(&store.root, Some(&game.dir), profile, last);
        built.warnings.extend(row_id_warnings);
        built.lines.extend(row_ids.recover(&store.sessions_dir()).lines);
        if let Some(lent) = lent_ids.as_ref().filter(|l| !l.rows.is_empty()) {
            let n = row_ids.lend(lent, &resolved.order).map_err(Error::Resolve)?;
            built.lines.push(format!(
                "database: {n} added-row id(s) are the room host's, for this session only; row-ids.json keeps yours"
            ));
        }
        let older = crate::tracks::read_older(&store.root, profile);
        // A full track id pool lends the id of a track whose addon this build leaves out (Store::track_stat_id).
        row_ids.building(resolved.order.iter().cloned());
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
                &resolved.order,
                &|id: &str| run.name(id),
            ));
        }
        let mut kept: BTreeMap<String, usize> = BTreeMap::new();
        for key in crate::tracks::keep_changes(&mut db, &shipped_db, &track_changes) {
            *kept.entry(run.writer_name(&planner, key)).or_default() += 1;
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
                    .filter(|id| !left_codes.contains(&m.track.as_str()) && !resolved.order.contains(*id));
                let by = planner.record_writer(m.key).map(name);
                built
                    .warnings
                    .push(m.line(by.as_deref(), adds.map(|id| name(id)).as_deref()));
            }
        }
        if let Ok(cat) = crate::dbedit::Catalog::open(store, game) {
            for t in crate::dbedit::rows::trim_row_counts(&cat, &mut db) {
                built.warnings.push(t.line(&run.writer_name(&planner, t.key)));
            }
        }
        for c in crate::dbedit::calendar::cut_rounds(&db) {
            built.warnings.push(c.line(&run.writer_name(&planner, c.key)));
        }
        for u in crate::heads::keep_wearable(&mut db, &shipped_db) {
            built.warnings.push(u.line(&run.writer_name(&planner, u.key)));
        }
        for k in crate::livery_kits::keep_kits(&mut db) {
            built.warnings.push(k.line(&run.writer_name(&planner, k.key)));
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
        let row_adders = crate::stdlib::room::row_adders(copies.iter().map(|c| c.addon.as_str()), CORE_ID);
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
            let why = match resolved.order.iter().any(|id| id == driver_manager::ID) {
                true => driver_manager::Why::NotAdded,
                false => driver_manager::Why::Off,
            };
            let names = driver_manager::names(store);
            built
                .warnings
                .extend(driver_manager::warnings(&db, &driver_picks, &names, why));
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
        fitted.place(run, &worlds, grid, &mut planner, &mut built, &mut caches)?;
    }
    let mut menu_kept = Kept::new();
    if !ui_maps.is_empty() {
        let ui = build_ui(&ui_maps, &resolve, &store.root.join("cache/menus"))?;
        caches.push(ui.dir.clone());
        menu_kept = ui.kept;
        built
            .warnings
            .extend(ui.notes.iter().map(|n| format!("menus: {n}")));
        for (code, lists) in &ui.lists {
            built.lines.push(format!(
                "{code}: menu pictures in {} list(s), minimap and track-select scenes",
                lists.len()
            ));
        }
        for (rel, src) in ui.replaced {
            built.replace(rel, src)?;
        }
    }
    let ers_maps: Vec<ErsMap> = fitted
        .kept
        .iter()
        .filter(|(_, pkg, _)| !pkg.ers.is_empty())
        .map(|(_, pkg, _)| ErsMap {
            code: &pkg.track_code,
            base: pkg.activation.base_track().unwrap_or(&pkg.track_code),
            values: &pkg.ers,
        })
        .collect();
    let mut handled: BTreeMap<String, PathBuf> = BTreeMap::new();
    if handling_maps
        .iter()
        .any(|m| matches!(m.target, handling::Target::Stock(_)))
    {
        let hb = handling::build_stock(&handling_maps, &resolve, &store.root.join("cache/handling"))?;
        caches.extend(hb.dir.clone());
        for line in hb.lines {
            match line.strip_prefix(crate::activation::wheel_check::WARNING_PREFIX) {
                Some(w) => built.warnings.push(w.to_owned()),
                None => built.lines.push(line),
            }
        }
        handled = hb.replaced;
    }
    let current = |rel: &str| handled.get(rel).cloned().unwrap_or_else(|| resolve(rel));
    let ers_cars = |rel: &str| !ers_maps.is_empty() && ers::is_car_archive(rel);
    for (rel, src) in &handled {
        if !ers_cars(rel) && built.plan.data.insert(rel.clone(), src.clone()).is_some() {
            return Err(Error::Map(format!("{rel} is replaced twice")));
        }
    }
    if !ers_maps.is_empty() {
        let ers = build_ers(&ers_maps, &game.dir, &current, &store.root.join("cache/ers"))?;
        for (rel, src) in &handled {
            if ers_cars(rel) && !ers.replaced.contains_key(rel) {
                built.plan.data.insert(rel.clone(), src.clone());
            }
        }
        caches.extend(ers.dir.clone());
        built.lines.push(format!(
            "ers: {} handling file(s) in {} car archive(s) get the tracks' settings",
            ers.handling_files,
            ers.replaced.len()
        ));
        built
            .warnings
            .extend(ers.notes.iter().map(|n| format!("ers: {n}")));
        for (rel, src) in ers.replaced {
            built.replace(rel, src)?;
        }
    }
    let file_ops = planner.take_files();
    let mut erp_ops = planner.take_erp();
    let rank = |o: &Origin| {
        resolved
            .order
            .iter()
            .position(|id| *id == o.addon)
            .unwrap_or(usize::MAX)
    };
    let puts = archive_puts(store, &file_ops, &erp_ops, &rank)?;
    for (rel, put) in &puts {
        planner.put_archive(rel, &put.origin);
    }
    erp_ops.retain_mut(|op| {
        let rank = rank(&op.origin);
        after_puts(op, &puts, rank)
    });
    let mut mipmaps = BTreeMap::new();
    let mut built_over = BTreeSet::new();
    let name = |origin: &str| run.origin_name(origin);
    for (id, folder, n) in stray_asset_groups(&game.dir, &file_ops) {
        built
            .warnings
            .push(stray_asset_group_line(&name(&id), &folder, n));
    }
    if !hud_inputs.is_empty() {
        let stock = crate::hud::GameStock::with_baseline(&game.dir, &store.root)
            .unwrap_or_else(|_| crate::hud::GameStock::new(&game.dir));
        let staged = built.plan.data.clone();
        let base = |rel: &str| {
            staged
                .get(rel)
                .or_else(|| puts.get(rel).map(|p| &p.path))
                .cloned()
                .unwrap_or_else(|| resolve(rel))
        };
        let mfd_rows = match menus {
            true => crate::ui::mfd_rows(&session),
            false => Vec::new(),
        };
        let page = crate::ui::mfd_page_rows(&mfd_rows);
        let mfd_edit = |data: &[u8]| crate::ui::mfd::templates(data, &page);
        let put_names: Vec<String> = puts.keys().cloned().collect();
        let composed = crate::hud::integrate::compose_in_build(
            &crate::hud::integrate::Context {
                store,
                stock: &stock,
                base: &base,
                baseline: &resolve,
                puts: &put_names,
                name: &name,
            },
            &erp_ops,
            &hud_inputs,
            (!page.is_empty()).then_some(&mfd_edit as crate::hud::TemplateBase),
        )?;
        let owner = hud_inputs[0].addon.clone();
        composed
            .output
            .write(&store.addon_cache_dir(&owner).join("hud"))?;
        built
            .lines
            .extend(composed.lines.iter().map(|l| format!("hud: {l}")));
        built.warnings.extend(composed.warnings);
        erp_ops.extend(composed.ops);
    }
    if !erp_ops.is_empty() || !puts.is_empty() {
        let staged = built.plan.data.clone();
        let base = |rel: &str| {
            staged
                .get(rel)
                .or_else(|| puts.get(rel).map(|p| &p.path))
                .cloned()
                .unwrap_or_else(|| resolve(rel))
        };
        let mut absent: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        erp_ops.retain(|op| {
            let AssetOp::Erp(e) = &op.op else {
                return true;
            };
            let gone: Vec<&str> = reads(e)
                .into_iter()
                .filter(|rel| relative_inside(rel) && !base(rel).is_file())
                .collect();
            if gone.is_empty() {
                return true;
            }
            absent
                .entry(op.origin.addon.clone())
                .or_default()
                .extend(gone.into_iter().map(str::to_owned));
            false
        });
        for (id, rels) in &absent {
            built.warnings.push(missing_archives(&name(id), rels));
        }
        for (rel, put) in &puts {
            if erp_ops
                .iter()
                .any(|op| matches!(&op.op, AssetOp::Erp(e) if writes(e).contains(&rel.as_str())))
            {
                built.lines.push(format!(
                    "erp: {rel}: later changes apply to the whole archive {} puts",
                    put.origin
                ));
            }
        }
        let mut erps = ErpBuild::new(&base).with_baseline(&resolve);
        for rel in puts.keys() {
            erps.load(rel)?;
        }
        let contested =
            crate::erpbuild::contested_ui(&erp_ops, &|op, s| source_dir(store, &op.origin.addon, s.root));
        for (erp, resource) in menu_kept.keys() {
            erps.keep(erp, resource);
        }
        let runtime_route = planner.route(Kind::Erp) == Route::Runtime;
        // Every origin that writes, in load order: the later wins.
        let mut ranks: Vec<String> = Vec::new();
        let mut prebuilt: Vec<Prebuilt> = Vec::new();
        let slices = slice_merges(&erp_ops);
        for (at, op) in erp_ops.iter().enumerate() {
            cancel()?;
            crate::now_doing(format!("applying {}", op.origin));
            let id = &op.origin.addon;
            let resolve = |s: &Source| Ok(s.resolve(&source_dir(store, id, s.root))?);
            let origin = op.origin.to_string();
            if matches!(op.op, AssetOp::Erp(_)) && !ranks.contains(&origin) {
                ranks.push(origin.clone());
            }
            match &op.op {
                AssetOp::Erp(crate::ops::ErpOp::Overlay {
                    erps: targets,
                    source,
                }) if runtime_route => {
                    let path = resolve(source)?;
                    let file = swofty_formats::erp::ErpFile::open(&path)?;
                    let stale = crate::erpbuild::stale_stats_in(&file)?.len();
                    let menus = file
                        .erp
                        .resources
                        .iter()
                        .any(|r| targets.iter().any(|t| erps.keeps(t, &r.name)));
                    let fits = match merged_later(&slices, at, targets, &path)? {
                        Some(why) => Err(why),
                        None if menus => Err("it carries the track menu pictures".to_owned()),
                        None if targets.iter().any(|t| !crate::erpbuild::served_at_runtime(t)) => {
                            Err("the in-game loader patches only archives under 2025_asset_groups".to_owned())
                        }
                        None if stale > 0 => {
                            Err(format!("{stale} resource(s) in it need their size headers fixed"))
                        }
                        None if carries_handling(&path, targets)? => Err(
                            "it carries car handling files, which the game reads from the game file"
                                .to_owned(),
                        ),
                        None if let Some(uri) = contested_in(&contested, &path, targets)? => Err(format!(
                            "another addon replaces {uri} whole too, and the two are merged node by node"
                        )),
                        None => overlay_fits(&path, targets, &base)?,
                    };
                    match fits {
                        Ok(resources) => prebuilt.push(Prebuilt {
                            origin,
                            path,
                            erps: targets.clone(),
                            resources,
                        }),
                        Err(why) => {
                            built.lines.push(format!(
                                "erp: {origin}: its overlay archive goes in resource by resource; {why}"
                            ));
                            erps.apply_for(
                                &crate::ops::ErpOp::Overlay {
                                    erps: targets.clone(),
                                    source: source.clone(),
                                },
                                &origin,
                                &resolve,
                            )
                            .map_err(|e| Error::Other(format!("applying {}: {e}", op.origin)))?;
                            ui_merge_notes(&mut built, &mut planner, erps.take_ui_merges(), &name);
                        }
                    }
                }
                AssetOp::Erp(erp) => {
                    erps.set_merge_ui(op.origin.item.as_deref() != Some(crate::hud::integrate::ORIGIN));
                    match erps.apply_for(erp, &op.origin.to_string(), &resolve) {
                        Ok(()) => {}
                        Err(Error::ResourceMissing { erp, resource }) if puts.contains_key(&erp) => {
                            built.warnings.push(missing_resource(
                                &name(&op.origin.to_string()),
                                &erp,
                                &resource,
                                &name(&puts[&erp].origin.to_string()),
                            ));
                            continue;
                        }
                        Err(e) => {
                            return Err(Error::Other(format!("applying {}: {e}", op.origin)));
                        }
                    }
                    for m in erps.take_merges() {
                        record_merge(&mut built, &mut planner, &op.origin, m);
                    }
                    ui_merge_notes(&mut built, &mut planner, erps.take_ui_merges(), &name);
                }
                AssetOp::Warn { message } => built.warnings.push(format!("{}: {message}", op.origin)),
                _ => {}
            }
        }
        built.warnings.extend(erps.wheel_warnings());
        if !ers_maps.is_empty() {
            let edited = erps.edit_written(|rel, r| match ers::in_teams(rel) && r.kind() == "VTF" {
                true => ers::edit_vtf(r, &ers_maps),
                false => Ok(None),
            })?;
            if edited > 0 {
                built.lines.push(format!(
                    "ers: {edited} handling file(s) other addons change get the tracks' settings too"
                ));
            }
        }
        let keeper = |h: &Held| {
            let codes = menu_kept.get(&(h.erp.clone(), h.resource.clone()))?;
            fitted
                .kept
                .iter()
                .filter(|(_, p, _)| codes.contains(&p.track_code))
                .map(|(_, _, o)| o)
                .max_by_key(|o| rank(o))
                .cloned()
        };
        let origin = |s: &str| {
            erp_ops
                .iter()
                .find(|op| op.origin.to_string() == s)
                .map(|op| op.origin.clone())
        };
        keep_menus(
            &mut built,
            &mut planner,
            erps.take_held(),
            &keeper,
            &origin,
            &name,
        );
        for n in erps.take_notes() {
            built.lines.push(format!("erp: {n}"));
        }
        for w in erps.check_ui(&name)? {
            built.warnings.push(format!("ui: {w}"));
        }
        for line in erps.cohere_arrays()? {
            built.lines.push(format!("erp: {line}"));
        }
        for (severity, w) in erps.check_textures(&name) {
            let kind = match severity {
                crate::texcheck::Severity::Invalid => "texture the game's renderer may reject",
                _ => "texture",
            };
            built.warnings.push(format!("{kind}: {w}"));
        }
        let stats = erps.repair_stats();
        let by_origin = |list: &[(String, String)]| {
            let mut out: Vec<(String, Vec<String>)> = Vec::new();
            for (origin, r) in list {
                match out.iter_mut().find(|(o, _)| o == origin) {
                    Some((_, rs)) => rs.push(r.clone()),
                    None => out.push((origin.clone(), vec![r.clone()])),
                }
            }
            out
        };
        if let Some((origin, files)) = by_origin(&stats.broken).first() {
            let who = name(origin);
            let fix = format!("Turn {who} off in this profile.");
            return Err(Error::Other(crate::erpbuild::unrepairable_message(
                &who, files, &fix,
            )));
        }
        for (origin, files) in by_origin(&stats.repaired) {
            built.lines.push(format!(
                "erp: {}",
                crate::erpbuild::repaired_note(&name(&origin), &files)
            ));
        }
        mipmaps = erps.take_mipmaps();
        if let Some(note) = planner.route_note(Kind::Erp) {
            built.lines.push(format!("erp: {note}"));
        }
        let resized = if planner.route(Kind::Erp) == Route::Runtime {
            let outside = erps.take_outside(&built.overlay)?;
            for rel in &outside {
                built.lines.push(format!(
                    "erp: {rel} built into the game files; the in-game loader patches only archives under 2025_asset_groups"
                ));
            }
            let resized = erps.take_resized(&built.overlay)?;
            for r in &resized {
                let (uri, why) = &r.resources[0];
                built.lines.push(format!(
                    "erp: {} built into the game files; {} resource(s) change their stock layout, which the in-game loader cannot load, such as {uri}: {why}",
                    r.erp,
                    r.resources.len()
                ));
            }
            let built_in: Vec<String> = resized.iter().map(|r| r.erp.clone()).chain(outside).collect();
            if !prebuilt.is_empty() {
                built.lines.push(format!(
                    "erp: {} ready overlay archive(s) load as they are",
                    prebuilt.len()
                ));
            }
            let notes = runtime.resources(erps.resources(), &ranks, &prebuilt, &built.overlay)?;
            built.warnings.extend(notes);
            stage_erps(&mut built, &staged, built_in)?;
            resized
        } else {
            let resized = erps.resized();
            let archives = store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
            let (written, reused) = erps.write_cached(&built.overlay, Some(&archives), &cancel)?;
            if reused > 0 {
                built.lines.push(format!(
                    "erp: {reused} of {} archive(s) unchanged since an earlier build, linked from the cache",
                    written.len()
                ));
            }
            stage_erps(&mut built, &staged, written)?;
            resized
        };
        built.warnings.extend(stock_holders(&game.dir, &resized));
        let in_plan: BTreeSet<String> = built.plan.data.keys().map(|r| r.to_ascii_lowercase()).collect();
        let staged_l: BTreeSet<String> = staged.keys().map(|r| r.to_ascii_lowercase()).collect();
        built_over = puts
            .keys()
            .filter(|rel| {
                let l = rel.to_ascii_lowercase();
                in_plan.contains(&l) && !staged_l.contains(&l)
            })
            .cloned()
            .collect();
    }
    cancel()?;
    crate::now_doing("placing files");
    put_files(
        &mut built,
        store,
        game,
        mipmaps,
        file_ops,
        &built_over,
        (&ers_maps, &handling_maps),
    )?;
    match loader_dir() {
        Some(dir) => {
            built.plan.loader = loader_files(&dir)?;
            built.plan.registry_override = true;
            built.lines.push(format!(
                "loader: {} file(s) from {}",
                built.plan.loader.len(),
                dir.display()
            ));
            if menus {
                let lang = planner.route(Kind::Lang) == Route::Runtime;
                let writer = |erp: &str, res: &str| {
                    let targets = [
                        Target::Resource {
                            erp: erp.to_owned(),
                            uri: res.to_owned(),
                        },
                        Target::File(erp.to_owned()),
                    ];
                    planner.last_writer(&targets).map(|o| name(&o.to_string()))
                };
                for (key, value) in build_menus(&mut built, &mut session, &resolve, &writer, lang)? {
                    planner.push(Op {
                        op: AssetOp::Lang(LangOp::Set {
                            language: "*".into(),
                            key,
                            value,
                        }),
                        origin: Origin {
                            addon: CORE_ID.into(),
                            item: None,
                        },
                    });
                }
            }
            let files = screen_files(&order, &store.addons_dir())?;
            if !files.is_empty() {
                built.lines.push(format!("ui: {} screen(s) under {LOADER_DIR}/ui", files.len()));
            }
            for (rel, src) in files {
                built.place_loader(rel, src)?;
            }
            let core = format!("{LOADER_DIR}/{ADDONS_DIR}/core/");
            let taken: Vec<String> = built
                .plan
                .loader
                .keys()
                .filter_map(|rel| rel.strip_prefix(&core))
                .filter(|n| n.ends_with(".dll") && !n.contains('/'))
                .map(str::to_owned)
                .collect();
            let plugins = plugin_files(store, order.iter().copied(), taken)?;
            if !plugins.dlls.is_empty() {
                built.lines.push(format!(
                    "native: {} plugin(s), {} file(s) under {LOADER_DIR}/{ADDONS_DIR}",
                    plugins.dlls.len(),
                    plugins.files.len()
                ));
            }
            for (rel, src) in plugins.files {
                built.place_loader(rel, src)?;
            }
            session.plugins = plugins.dlls;
            if !udp_ids.is_empty() {
                let rel = format!("{LOADER_DIR}/{}", crate::udp_ids::FILE);
                let src = built.overlay.join(crate::udp_ids::FILE);
                std::fs::create_dir_all(&built.overlay).map_err(io_err(&built.overlay))?;
                crate::write_json(&src, &udp_ids)?;
                built.plan.loader.insert(rel, src);
                built.lines.push(format!(
                    "udp: telemetry reports {} added team id(s) and {} added driver id(s) under stock ids, {} track(s) by Swofty track id",
                    udp_ids.teams.len(),
                    udp_ids.drivers.len(),
                    udp_ids.track_ids.len()
                ));
            }
            if !id_aliases.is_empty() {
                let rel = format!("{LOADER_DIR}/{}", crate::row_ids::ALIASES_FILE);
                let src = built.overlay.join(crate::row_ids::ALIASES_FILE);
                std::fs::create_dir_all(&built.overlay).map_err(io_err(&built.overlay))?;
                id_aliases.write(&src)?;
                built.plan.loader.insert(rel, src);
                let n: usize = id_aliases.tables.values().map(|t| t.len()).sum();
                built.lines.push(format!(
                    "database: {LOADER_DIR}/{} maps {n} earlier row id(s) to the ids the rows have now, so saves made under the earlier ids load",
                    crate::row_ids::ALIASES_FILE
                ));
            }
            if let Some(ids) = lent_ids
                .as_ref()
                .and_then(|l| l.room_stat_ids(&resolved.order))
            {
                let rel = format!("{LOADER_DIR}/{}", crate::row_ids::ROOM_STAT_IDS_FILE);
                let src = built.overlay.join(crate::row_ids::ROOM_STAT_IDS_FILE);
                std::fs::create_dir_all(&built.overlay).map_err(io_err(&built.overlay))?;
                crate::write_json(&src, &ids)?;
                built.plan.loader.insert(rel, src);
                built.lines.push(format!(
                    "registry: {LOADER_DIR}/{} lends the host's stat ids for this session",
                    crate::row_ids::ROOM_STAT_IDS_FILE
                ));
            }
            if let Some(placed) = &std_index {
                let index = built.overlay.join("registry-index.json");
                std::fs::create_dir_all(&built.overlay).map_err(io_err(&built.overlay))?;
                std::fs::write(&index, placed.index.to_json()).map_err(io_err(&index))?;
                for (rel, src) in placed
                    .files
                    .iter()
                    .map(|(r, s)| (r.clone(), s.clone()))
                    .chain([(crate::stdlib::index::PATH.to_owned(), index)])
                {
                    match built.plan.loader.insert(rel.clone(), src.clone()) {
                        Some(before) if before != src => {
                            return Err(Error::Other(format!("{rel} is placed twice")));
                        }
                        _ => {}
                    }
                }
                let data: usize = placed.index.addons.iter().map(|a| a.data.len()).sum();
                let named: usize = placed
                    .index
                    .addons
                    .iter()
                    .filter_map(|a| std16.get(&a.id))
                    .map(|s| s.files.files.len())
                    .sum();
                built.lines.push(format!(
                    "registry: index for {} addon(s) with {data} data file(s) and {named} file(s) they name, under \
                     {LOADER_DIR}/registry and {LOADER_DIR}/{ADDONS_DIR}",
                    placed.index.addons.len()
                ));
            }
        }
        None => built.warnings.push(format!(
            "{CORE_ID}: the in-game loader was not found (native/dist, loader/ beside the swofty command, or {LOADER_ENV}); the game starts without it"
        )),
    }
    built.settings = Some(session);
    let mut changes = planner.changes();
    let mut planned = planner.finish();
    for op in planned.runtime {
        let resolve = |op: &Op, s: &Source| Ok(s.resolve(&source_dir(store, &op.origin.addon, s.root))?);
        runtime.push(op, &resolve)?;
    }
    if let Some(runtime) = runtime.finish(&built.overlay)? {
        built.lines.push(runtime.summary());
        for (rel, src) in runtime.files {
            built.place_loader(rel, src)?;
        }
    }
    for (rel, bytes) in shader_files.files {
        let path = built.overlay.join(crate::runtime::SWOFTY).join(&rel);
        let dir = path.parent().expect("has parent");
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        std::fs::write(&path, bytes).map_err(io_err(&path))?;
        built
            .plan
            .loader
            .insert(format!("{}/{rel}", crate::runtime::SWOFTY), path);
    }
    built.warnings.extend(waiting_lines(&planned.waiting, &features));
    for (_, _, origin) in &fitted.kept {
        changes.entry(origin.addon.clone()).or_default().tracks += 1;
    }
    let name = |id: &str| run.name(id);
    for id in resolved.order.iter().filter(|id| *id != CORE_ID) {
        if let Some(c) = changes.get(id).filter(|c| !c.is_empty()) {
            built.lines.push(format!("{}: {c}", name(id)));
        }
    }
    for c in &mut planned.conflicts {
        if matches!(&c.target, Target::File(f) if f == crate::builtin::CORE_INTRO_CACHED)
            || matches!(&c.target, Target::String { key, .. } if key == crate::builtin::CORE_INTRO_SUBTITLE)
        {
            c.overridden.retain(|o| o.addon != CORE_ID);
        }
    }
    planned.conflicts.retain(|c| !c.overridden.is_empty());
    let records = planned
        .conflicts
        .iter()
        .any(|c| matches!(c.target, Target::Record(_)));
    let catalog = records
        .then(|| crate::dbedit::Catalog::open(store, game).ok())
        .flatten();
    let record = |key: u64| {
        planned.record_names.get(&key).cloned().or_else(|| {
            catalog
                .as_deref()
                .and_then(|c| crate::conflicts::record_name(c, key))
        })
    };
    let (report, within) = crate::conflicts::summarise(profile, &planned.conflicts, &record);
    built.warnings.extend(report.pairs.iter().map(|p| p.line(&name)));
    built.warnings.extend(within);
    built.report = Report {
        order: built.report.order.clone(),
        stock_removals,
        kept_lists,
        waits,
        ..report
    };
    built.conflicts = planned.conflicts;
    built.waiting = planned.waiting;
    if !built.plan.is_vanilla() {
        let plan_tracks: Vec<crate::tracks::PlanTrack> = fitted
            .kept
            .iter()
            .map(|(_, pkg, origin)| crate::tracks::PlanTrack {
                code: pkg.track_code.clone(),
                name: pkg.track_name.clone().filter(|n| !n.trim().is_empty()),
                addon: Some(origin.addon.clone()),
            })
            .collect();
        let listing = serde_json::json!({
            "profile": profile,
            "buildhash": built.buildhash,
            "tracks": plan_tracks,
            "replace": built.plan.data,
            "add": built.plan.added,
            "loader": built.plan.loader,
        });
        crate::write_json(&built.overlay.join(PLAN_FILE), &listing)?;
    }
    cancel()?;
    crate::now_doing("saving the build");
    let saved = (!built.plan.is_vanilla())
        .then(|| Saved::of(&built, lines_at, warnings_at))
        .transpose()?;
    caches.push(built.overlay.clone());
    caches.extend(
        [&built.plan.data, &built.plan.added, &built.plan.loader]
            .into_iter()
            .flat_map(|m| m.values().cloned()),
    );
    let removed = crate::cache::prune(&store.root.join("cache"), profile, &caches);
    if removed > 0 {
        built
            .lines
            .push(format!("cache: {removed} stale build folder(s) removed"));
    }
    crate::cache::prune_packed(&packed, PACKED_AGE);
    crate::cache::prune_archives(
        &store.root.join("cache").join(crate::cache::ARCHIVES_DIR),
        ARCHIVE_AGE,
        ARCHIVE_BYTES,
    );
    // A check keeps no row ids, so a build that reused this one would never keep them
    // either, and a later build could give those rows other ids than a career holds.
    if let Some(saved) = saved.filter(|_| !row_ids_unsaved) {
        crate::write_json(&built.overlay.join(SAVED_FILE), &saved)?;
    }
    crate::write_json(&crate::conflicts::path(store, profile), &built.report)?;
    if !checking {
        fingerprint::record_outputs(store, profile, with_row_ids(outputs, &built.row_ids)).ok();
        track_files(store, game, &mut built, &fitted, &record_ui);
    }
    Ok(built)
}

#[cfg(test)]
mod tests;
