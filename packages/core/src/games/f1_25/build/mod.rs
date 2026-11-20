mod addons;
mod archives;
mod buildhash;
mod cars;
mod database;
pub(crate) mod digests;
mod failure;
mod files;
mod finish;
mod hooks;
mod inputs;
mod loader;
mod lock;
mod op_files;
mod originals;
mod picture;
mod registry;
mod run;
mod saved;
mod setup;
mod start;
mod tracks;

use std::collections::BTreeMap;
use std::path::PathBuf;

use self::hooks::Hooks;
use self::inputs::Inputs;
use self::originals::check_originals;
use self::run::Run;
use self::saved::Saved;
use self::setup::Setup;
use self::start::{AsBuilt, Start};
use crate::addons::settings::{OnAddons, SessionSettings};
use crate::asset::Op;
use crate::content::runtime_assets::Runtime;
use crate::install::{Game, Plan};
use crate::planner::{Conflict, Planner};
use crate::profile::conflicts::Report;
use crate::profile::resolver::resolve;
use crate::store::Store;
use crate::{Error, Result};

pub use self::failure::{check_refusal, named, refused_fix, stale_text};
pub use self::loader::{Plugins, build_menus, plugin_files, screen_files};
pub use self::lock::{BuildLock, lock};
pub use self::op_files::read_op_file;
pub use self::originals::{heal_stale, original, stock_max_ids};
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
    /// The build restored a saved build with the same hash instead of building again.
    pub reused: bool,
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
    let mut healed: Vec<String> = Vec::new();
    let mut notes = Vec::new();
    loop {
        match build_as(store, profile, game, false) {
            Err(Error::StaleInput { rel, detail }) => {
                let note = match healed.contains(&rel) {
                    true => None,
                    false => heal_stale(&store.root, game, &rel)?,
                };
                let Some(note) = note else {
                    return Err(Error::Other(stale_text(&detail, &rel, false)));
                };
                notes.push(format!("stale: {note}"));
                healed.push(rel);
            }
            Ok(mut built) => {
                built.lines.splice(0..0, notes);
                return Ok(built);
            }
            Err(e) => return Err(e),
        }
    }
}

/// What [`track_limit`] found and how its check went.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackLimitCheck {
    /// The tracks the on addons add with `track.add`.
    pub added: usize,
    /// The track addons left out because the game's track lists are full.
    pub limit: Option<crate::tracks::TrackLimit>,
    /// How long reading the profile and counting its tracks took.
    pub counting: std::time::Duration,
    /// Whether the tracks were fitted to the game's track lists: false when too few are on to fill them.
    pub fitted: bool,
    /// How long reading the tracks' op files and fitting them to the lists took.
    pub fitting: std::time::Duration,
}

/// The track addons `profile` would lose to the game's full track lists: the tracks the on addons' op files and
/// track records add, fitted to the lists the way a build fits them, without planning or placing the rest of the
/// build and without starting any addon's program. `std16` is [`crate::stdlib::deps::read_all`] of the installed
/// addons. None, and no fitting, when too few tracks are on to fill the lists.
pub fn track_limit(
    store: &Store,
    profile: &str,
    game: &Game,
    std16: &BTreeMap<String, crate::stdlib::deps::Std>,
) -> Result<TrackLimitCheck> {
    let started = std::time::Instant::now();
    let addons = store.registry()?.manifests();
    let AsBuilt { profile: p, .. } = start::as_built(store, profile, &addons)?;
    let order = resolve(&p, &addons)?.order;
    let mut check = TrackLimitCheck {
        added: crate::tracks::added_count(store, &order, &addons),
        counting: started.elapsed(),
        ..TrackLimitCheck::default()
    };
    if !crate::tracks::may_fill_track_lists(check.added) {
        return Ok(check);
    }
    let fitting = std::time::Instant::now();
    if let Some(stop) = crate::stdlib::deps::for_order(profile, &order, &addons, std16).stop_text() {
        return Err(Error::Dependencies(stop));
    }
    let mut on = OnAddons::new(order.iter().filter_map(|id| addons.get(id)));
    let shipped = originals::Shipped::new(&store.root, game);
    start::trackless_options(store, &shipped, &p, &order, &addons, &mut on);
    let ops = tracks::track_ops(store, &p, &on, &order, &addons, std16)?;
    let fit = tracks::fit_tracks(store, game, &addons, &order, std16, ops)?;
    check.fitted = true;
    check.limit = fit.limit;
    check.fitting = fitting.elapsed();
    Ok(check)
}

/// One build of a profile once its addons are read: the state every later stage changes.
struct Pipeline<'a, 'p> {
    run: Run<'a>,
    planner: Planner<'p>,
    runtime: Runtime,
    built: Built,
    session: SessionSettings,
    /// The cache folders the build uses, which the cache prune keeps.
    caches: Vec<PathBuf>,
}

/// A `checking` build gives addon build hooks a host that lists addons but
/// cannot install, remove, turn on or move them.
pub(crate) fn build_as(store: &Store, profile: &str, game: &Game, checking: bool) -> Result<Built> {
    let registry = store.registry()?;
    let addons = registry.manifests();
    let shipped = originals::Shipped::new(&store.root, game);
    let Start {
        settings,
        order,
        on,
        mut std16,
        lent,
        mut built,
    } = Start::read(store, profile, &addons, &shipped)?;
    let packer = swofty_formats::erp::Packer::cached(setup::packed_dir(store)).deferring(true);
    let run = Run {
        store,
        game,
        profile,
        checking,
        addons: &addons,
        order: &order,
        settings: &settings,
        on: &on,
        packer: &packer,
    };
    if settings.is_vanilla() {
        built.report = start::report(run);
        built.plan = Plan::vanilla();
        return Ok(built);
    }
    let _lock = lock(store, profile)?;
    let mut hooks = match setup::loader_features() {
        Ok(features) => Hooks::start(run, &registry, &features),
        Err(_) => Hooks::default(),
    };
    built.warnings.extend(start::career_warnings(run, &shipped));
    drop(shipped);
    built.report = start::report(run);
    check_originals(&store.root, game)?;
    crate::now_doing("reading the addons");
    let setup = Setup::read(run, &mut built)?;
    let (mut pipeline, inputs) = Inputs::read(
        run,
        &setup,
        &registry,
        &mut hooks,
        &mut std16,
        lent.is_some(),
        built,
    )?;
    if let Some(saved) =
        Saved::load(&pipeline.built.overlay, &pipeline.built.buildhash).filter(|s| s.usable(store, game))
    {
        return pipeline.reuse(saved, inputs);
    }
    run.cancel()?;
    std::fs::remove_dir_all(&pipeline.built.overlay).ok();
    crate::now_doing("editing the game database");
    let edits = crate::diagnostics::phases::time_part("build.database.edit", || {
        pipeline.edit_database(&inputs, &std16)
    })?;
    let menu_kept =
        crate::diagnostics::phases::time_part("build.database.menus", || pipeline.track_menus(&inputs))?;
    let ers_maps =
        crate::diagnostics::phases::time_part("build.database.cars", || pipeline.car_archives(&inputs))?;
    let archives = pipeline.apply_archives(&inputs, &ers_maps, &menu_kept)?;
    run.cancel()?;
    crate::now_doing("placing files");
    pipeline.place_files(&inputs, archives, &ers_maps)?;
    pipeline.loader_files(&inputs, &edits, &std16)?;
    pipeline.finish(inputs, edits, &setup)
}

#[cfg(test)]
mod tests;
