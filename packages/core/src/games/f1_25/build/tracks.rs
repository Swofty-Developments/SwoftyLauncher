use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_formats::gvdb::Gvdb;

use super::buildhash::BuildHash;
use super::failure::loader_missing;
use super::inputs::Inputs;
use super::op_files::read_op_file;
use super::registry::replaced_operations;
use super::run::Run;
use super::{Built, GVDB, Pipeline, original, source_dir};
use crate::activation::place::{
    AiTeam, LayerWorld, PlaceInput, SLOTS, ai_alias_lines, missing_worlds, place, room_for, slot_asset_group,
    slot_file, track_group,
};
use crate::activation::ui::{Kept, MapUi, build_ui, plan_lists};
use crate::addons::builtin::loader_dir;
use crate::addons::settings::OnAddons;
use crate::asset::{AssetOp, GroupOp, Op, Origin, TrackOp};
use crate::asset::{FileKind, Kind};
use crate::content::track_package::{TrackPackage, Unusable};
use crate::game_folder::baseline::{Baseline, sha256_opt};
use crate::game_folder::placed::Placed;
use crate::install::Game;
use crate::manifest::AddonManifest;
use crate::planner::{Planner, Route, Target};
use crate::profile::Profile;
use crate::stdlib::deps::Std;
use crate::store::Store;
use crate::{Error, Result, io_err};

pub type TrackPackages = (Vec<(PathBuf, TrackPackage, Origin)>, Vec<(Origin, Unusable)>);

/// The packages `track.add` ops add, and the ones a build leaves out.
pub fn track_packages(store: &Store, ops: Vec<Op>) -> Result<TrackPackages> {
    let mut out: Vec<(PathBuf, TrackPackage, Origin)> = Vec::new();
    let mut left_out = Vec::new();
    for op in ops {
        let AssetOp::Track(TrackOp::Add { source }) = op.op else {
            continue;
        };
        let dir = source.resolve(&source_dir(store, &op.origin.addon, source.root))?;
        let pkg = match crate::content::track_package::read_package(&dir) {
            Ok(pkg) => pkg,
            Err(why) => {
                left_out.push((op.origin, why));
                continue;
            }
        };
        if let Some((_, _, first)) = out.iter().find(|(_, p, _)| p.track_code == pkg.track_code) {
            return Err(Error::Map(match first.addon == op.origin.addon {
                true => format!("track {} is added twice by {}", pkg.track_code, first.addon),
                false => format!(
                    "track {} is added by both {} and {}; turn one of them off on the Addons page",
                    pkg.track_code, first.addon, op.origin.addon
                ),
            }));
        }
        out.push((dir, pkg, op.origin));
    }
    Ok((out, left_out))
}

/// The build's track packages, fitted to the game's track lists.
pub(super) struct FittedTracks {
    /// The packages the build adds.
    pub tracks: Vec<(PathBuf, TrackPackage, Origin)>,
    /// The packages the build leaves out, and why.
    pub left_out: Vec<(Origin, Unusable)>,
    /// The addons whose track records the core adds in game.
    pub recorded: BTreeSet<String>,
    /// The code and name of each track the full lists leave out that no kept package adds.
    pub dropped: Vec<(String, String)>,
    /// The build's notice for the tracks the full lists leave out.
    pub warning: Option<String>,
    pub limit: Option<crate::tracks::TrackLimit>,
}

/// The packages `ops` and the track records of `order` add, with the tracks past the game's track lists' room
/// left out, as a build and its track check both fit them.
pub(super) fn fit_tracks(
    store: &Store,
    game: &Game,
    addons: &BTreeMap<String, AddonManifest>,
    order: &[String],
    std16: &BTreeMap<String, Std>,
    ops: Vec<Op>,
) -> Result<FittedTracks> {
    let (mut tracks, mut left_out) = track_packages(store, ops)?;
    let recorded = crate::tracks::with_records(order, std16);
    let (more, unusable) = crate::tracks::record_packages(store, order, std16, &tracks);
    tracks.extend(more);
    left_out.extend(unusable);
    let mut fitted = FittedTracks {
        tracks,
        left_out,
        recorded,
        dropped: Vec::new(),
        warning: None,
        limit: None,
    };
    if !crate::tracks::may_fill_track_lists(fitted.tracks.len()) {
        return Ok(fitted);
    }
    let listed = listed_tracks(&original(&store.root, game, GVDB))?;
    let tracks = std::mem::take(&mut fitted.tracks);
    let (kept, full) = crate::tracks::fit_track_lists(&listed, tracks, &fitted.recorded);
    fitted.tracks = kept;
    let names: Vec<String> = full
        .iter()
        .map(|((_, pkg, origin), _)| {
            let addon = addons
                .get(&origin.addon)
                .map_or(origin.addon.as_str(), |m| m.name.as_str());
            match pkg.display_name() == addon {
                true => addon.to_owned(),
                false => format!("{} ({addon})", pkg.display_name()),
            }
        })
        .collect();
    for (((_, pkg, _), _), name) in full.iter().zip(&names) {
        if !fitted
            .tracks
            .iter()
            .any(|(_, p, _)| p.track_code == pkg.track_code)
        {
            fitted.dropped.push((pkg.track_code.clone(), name.clone()));
        }
    }
    fitted.warning = crate::tracks::track_lists_full_warning(&listed, &names);
    fitted.limit = crate::tracks::TrackLimit::new(
        &listed,
        names,
        full.iter()
            .map(|((_, _, origin), _)| origin.addon.clone())
            .collect(),
    );
    fitted
        .left_out
        .extend(full.into_iter().map(|((_, _, origin), why)| (origin, why)));
    Ok(fitted)
}

/// The tracks the game database at `path` puts in the game's track lists, kept while the file's size and time
/// stay the same: the shipped database is the same for every build and check until the game updates.
fn listed_tracks(path: &Path) -> Result<crate::tracks::ListedTracks> {
    type Cached = (
        PathBuf,
        u64,
        Option<std::time::SystemTime>,
        crate::tracks::ListedTracks,
    );
    static CACHE: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    let meta = std::fs::metadata(path).map_err(io_err(path))?;
    let (len, modified) = (meta.len(), meta.modified().ok());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, l, m, listed)) = cache.as_ref()
        && at == path
        && *l == len
        && *m == modified
        && modified.is_some()
    {
        return Ok(listed.clone());
    }
    let shipped = Gvdb::parse(&std::fs::read(path).map_err(io_err(path))?)?;
    let listed = crate::tracks::ListedTracks::of(&shipped);
    *cache = Some((path.to_owned(), len, modified, listed.clone()));
    Ok(listed)
}

/// The `track.add` ops of the addons of `order`, from their asset op files as a build renders them, in load order.
/// Tracks an addon's program adds when a build runs it are not among them.
pub(super) fn track_ops(
    store: &Store,
    p: &Profile,
    on: &OnAddons,
    order: &[String],
    addons: &BTreeMap<String, AddonManifest>,
    std16: &BTreeMap<String, Std>,
) -> Result<Vec<Op>> {
    let replaced = replaced_operations(std16, loader_dir().as_deref());
    let mut ops = Vec::new();
    for id in order {
        let Some(m) = addons.get(id) else {
            continue;
        };
        let dir = store.addons_dir().join(id);
        for rel in &m.operations.assets {
            if replaced.get(id.as_str()).is_some_and(|r| r.replaces(rel)) {
                continue;
            }
            let file = read_op_file(&dir.join(rel), FileKind::Assets, m, p, on)?;
            ops.extend(
                file.ops
                    .into_iter()
                    .filter(|op| matches!(op, AssetOp::Track(_)))
                    .map(|op| Op {
                        op,
                        origin: Origin {
                            addon: id.clone(),
                            item: None,
                        },
                    }),
            );
        }
    }
    Ok(ops)
}

/// The tracks a build adds.
pub(super) struct Tracks {
    /// Each track package kept, with its folder and the origin of the op that adds it, in load order.
    pub kept: Vec<(PathBuf, TrackPackage, Origin)>,
    /// The slot each kept track takes, when tracks go in slots rather than in asset groups of their own.
    pub slots: Vec<Option<&'static str>>,
    /// The addons whose track records add their rows in game.
    pub recorded: BTreeSet<String>,
    /// The codes of the tracks left out that no kept track stands for.
    left_codes: Vec<String>,
}

impl Tracks {
    /// The tracks the ops and track records add, less the ones the game's track lists have no room for,
    /// each left-out one with a warning.
    pub fn fit(
        run: Run,
        planner: &mut Planner,
        built: &mut Built,
        std16: &mut BTreeMap<String, Std>,
    ) -> Result<Self> {
        let store = run.store;
        let FittedTracks {
            tracks,
            left_out,
            recorded,
            dropped,
            warning,
            limit,
        } = fit_tracks(
            store,
            run.game,
            run.addons,
            run.order,
            std16,
            planner.take_tracks(),
        )?;
        for (code, name) in &dropped {
            planner.leave_out_track(code, name);
        }
        built.warnings.extend(warning);
        built.track_limit = limit;
        crate::tracks::keep_track_folders(store, &tracks, std16);
        for (origin, why) in &left_out {
            let name = run.name(&origin.addon);
            built
                .warnings
                .push(crate::tracks::left_out_line(&name, &origin.addon, why));
            planner.add_lenient(&origin.addon, &name);
        }
        let left_codes = left_out
            .iter()
            .filter_map(|(_, why)| why.code.as_deref())
            .filter(|c| !tracks.iter().any(|(_, p, _)| p.track_code == *c))
            .map(str::to_owned)
            .collect();
        Ok(Self {
            kept: tracks,
            slots: Vec::new(),
            recorded,
            left_codes,
        })
    }

    /// Gives each track its slot when asset groups aren't served at runtime; the slots are few.
    pub fn take_slots(&mut self, planner: &Planner, exe_dir: Option<&Path>, loader: bool) -> Result<()> {
        let natural = planner.route(Kind::Group) == Route::Runtime;
        if !natural && self.kept.len() > SLOTS.len() {
            if !loader {
                return Err(Error::Other(loader_missing(exe_dir)));
            }
            return Err(Error::Map(format!(
                "at most {} tracks can be enabled at once",
                SLOTS.len()
            )));
        }
        self.slots = (0..self.kept.len())
            .map(|i| (!natural).then(|| SLOTS[i]))
            .collect();
        Ok(())
    }

    pub fn left_codes(&self) -> Vec<&str> {
        self.left_codes.iter().map(String::as_str).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    pub fn hash(&self, hash: &mut BuildHash) -> Result<()> {
        hash.update(crate::activation::db::REGISTRY_JSON);
        for (dir, pkg, _) in &self.kept {
            hash.json(pkg);
            let erp = dir.join(&pkg.erp);
            let meta = std::fs::metadata(&erp).map_err(io_err(&erp))?;
            hash.update(format!("{}|{:?}", meta.len(), meta.modified().ok()));
        }
        Ok(())
    }

    /// The kept tracks that ship menu pictures, each with the asset group it loads in.
    pub fn ui_maps(&self) -> Vec<MapUi<'_>> {
        self.kept
            .iter()
            .zip(&self.slots)
            .filter(|((_, pkg, _), _)| pkg.ui.is_some())
            .map(|((dir, pkg, _), slot)| MapUi {
                pkg,
                dir,
                asset_group: match slot {
                    Some(slot) => slot_asset_group(slot),
                    None => track_group(&pkg.track_code, &pkg.track_code),
                },
            })
            .collect()
    }
}

impl Pipeline<'_, '_> {
    /// Places each kept track's archive, world files and the AI data the added AI teams borrow in the plan,
    /// and registers its asset groups.
    pub(super) fn place_tracks(
        &mut self,
        tracks: &Tracks,
        worlds: &[LayerWorld],
        ai_teams: &[AiTeam],
        grid: u32,
    ) -> Result<()> {
        let (run, planner, built, caches) = (self.run, &mut self.planner, &mut self.built, &mut self.caches);
        let (store, game) = (run.store, run.game);
        let packer = &run.eager_packer();
        for ((dir, pkg, origin), slot) in tracks.kept.iter().zip(&tracks.slots) {
            let slot_original = slot.map(|s| run.original(&slot_file(s)));
            let track_worlds = missing_worlds(worlds, &pkg.track_code, |group| {
                planner.last_writer(&[Target::Group(group.to_owned())]).is_some()
            });
            let cars = room_for(&track_worlds, grid);
            let track_ai: Vec<_> = ai_teams
                .iter()
                .filter(|t| {
                    planner
                        .last_writer(&[Target::Group(t.group(&pkg.track_code))])
                        .is_none()
                })
                .cloned()
                .collect();
            let placement = place(
                &PlaceInput {
                    pkg,
                    dir,
                    game: &game.dir,
                    slot: slot.zip(slot_original.as_deref()),
                    cars,
                    worlds: &track_worlds,
                    ai_teams: &track_ai,
                },
                &store.root.join("cache/maps").join(&pkg.track_code),
                packer,
            )?;
            caches.push(placement.dir.clone());
            let at = match slot {
                Some(slot) => format!("slot {slot}"),
                None => placement.archive.clone(),
            };
            built.lines.push(format!(
                "{}: track archive goes in {at}; {} texture(s) use its own mip files",
                pkg.track_code, placement.mips
            ));
            if !placement.groups.is_empty() {
                built.lines.push(format!(
                    "{}: {} asset group(s) registered in game",
                    pkg.track_code,
                    placement.groups.len()
                ));
            }
            for group in placement.groups {
                planner.push(Op {
                    op: AssetOp::Group(GroupOp::Register {
                        group,
                        erp: Some(placement.archive.clone()),
                        erps: Vec::new(),
                        deps: Vec::new(),
                        template: None,
                    }),
                    origin: origin.clone(),
                });
            }
            if let Some(g) = &placement.grid {
                built.lines.push(format!(
                    "{}: world extended from {} to {} grid slots, {} to {} garages and {} to {} pit boxes for {cars} cars",
                    pkg.track_code,
                    g.before.grid_slots,
                    g.after.grid_slots,
                    g.before.garages,
                    g.after.garages,
                    g.before.pit_boxes,
                    g.after.pit_boxes
                ));
            }
            for (alias, to) in &placement.aliases {
                built
                    .lines
                    .push(format!("{}: world {alias} stands in with {to}", pkg.track_code));
            }
            for line in ai_alias_lines(&placement.ai_aliases) {
                built.lines.push(format!("{}: {line}", pkg.track_code));
            }
            if !placement.repaired.is_empty() {
                built.lines.push(format!(
                    "{}: {}",
                    pkg.track_code,
                    crate::erp::repaired_note(pkg.display_name(), &placement.repaired)
                ));
            }
            if !placement.spaced.is_empty() {
                built.lines.push(format!(
                    "{}: {}",
                    pkg.track_code,
                    crate::activation::ailine::fixed_note(pkg.display_name(), &placement.spaced)
                ));
            }
            built
                .warnings
                .extend(placement.notes.iter().map(|n| format!("{}: {n}", pkg.track_code)));
            let (left, replaced): (Vec<_>, Vec<_>) = placement
                .replaced
                .into_iter()
                .partition(|(rel, _)| left_over(store, game, rel));
            for (rel, src) in replaced {
                built.replace(rel, src)?;
            }
            for (rel, src) in placement.added.into_iter().chain(left) {
                if built.plan.added.insert(rel.clone(), src).is_some() {
                    return Err(Error::Map(format!("{rel} is placed by two tracks")));
                }
            }
        }
        Ok(())
    }
}

/// The menu UI lists the build makes a track record's pictures for: its rows come from the core
/// at boot, so the core points their `TrackTextures` at those pictures from the facts file.
pub(super) fn record_ui_lists(
    ui_maps: &[MapUi],
    tracks: &Tracks,
    resolve: &dyn Fn(&str) -> PathBuf,
    warnings: &mut Vec<String>,
) -> crate::activation::ui::Lists {
    let codes: BTreeSet<&str> = tracks
        .kept
        .iter()
        .filter(|(_, _, o)| tracks.recorded.contains(&o.addon))
        .map(|(_, p, _)| p.track_code.as_str())
        .collect();
    let maps: Vec<MapUi> = ui_maps
        .iter()
        .filter(|m| codes.contains(m.pkg.track_code.as_str()))
        .map(|m| MapUi {
            pkg: m.pkg,
            dir: m.dir,
            asset_group: m.asset_group.clone(),
        })
        .collect();
    if maps.is_empty() {
        return Default::default();
    }
    match plan_lists(&maps, resolve) {
        Ok((lists, _)) => lists,
        Err(e) => {
            warnings.push(format!(
                "tracks: the record tracks' menu pictures can't be planned: {e}"
            ));
            Default::default()
        }
    }
}

/// Writes `$DATA/registry/track-facts.json` and `older-tracks.json` for a build of a profile with
/// addons. A file that can't be written is a warning; the build stands.
pub(super) fn track_files(
    store: &Store,
    game: &Game,
    built: &mut Built,
    tracks: &Tracks,
    record_ui: &crate::activation::ui::Lists,
) {
    let (slots, recorded) = (&tracks.slots, &tracks.recorded);
    if built.plan.is_vanilla() {
        return;
    }
    let file = |rel: &str| {
        built
            .plan
            .data
            .get(rel)
            .or_else(|| built.plan.added.get(rel))
            .cloned()
            .unwrap_or_else(|| original(&store.root, game, rel))
    };
    let placed_archive = |code: &str, slot: Option<&str>| match slot {
        Some(s) => slot_file(s),
        None => crate::activation::place::track_file(code),
    };
    let added: Vec<crate::activation::facts::Added> = tracks
        .kept
        .iter()
        .zip(slots)
        .map(|((dir, pkg, origin), slot)| crate::activation::facts::Added {
            code: pkg.track_code.clone(),
            shipped: dir.join(&pkg.erp),
            placed: file(&placed_archive(&pkg.track_code, *slot)),
            base: recorded
                .contains(&origin.addon)
                .then(|| pkg.activation.base_track().map(str::to_owned))
                .flatten(),
            ui_lists: match recorded.contains(&origin.addon) {
                true => record_ui
                    .get(&pkg.track_code)
                    .map(|l| l.iter().cloned().collect())
                    .unwrap_or_default(),
                false => Vec::new(),
            },
        })
        .collect();
    let scan = crate::activation::facts::Scan {
        profile: &built.plan.profile,
        db: file(GVDB),
        game: &game.dir,
        file: &file,
        placed: built
            .plan
            .data
            .keys()
            .chain(built.plan.added.keys())
            .cloned()
            .collect(),
        added,
        edits: built
            .plan
            .loader
            .get(crate::content::runtime_assets::ASSETS_FILE)
            .cloned(),
    };
    let mut warnings = Vec::new();
    if let Err(e) = crate::activation::facts::write(&store.root, &scan) {
        warnings.push(format!(
            "tracks: {} could not be written: {e}",
            crate::activation::facts::FILE
        ));
    }
    let older: Vec<(&str, &TrackPackage)> = tracks
        .kept
        .iter()
        .filter(|(_, _, o)| !recorded.contains(&o.addon))
        .map(|(_, pkg, o)| (o.addon.as_str(), pkg))
        .collect();
    let written = crate::tracks::write_older(
        &store.root,
        &built.plan.profile,
        &older,
        &original(&store.root, game, GVDB),
        &file(GVDB),
    );
    if let Err(e) = written {
        warnings.push(format!(
            "tracks: {} could not be written: {e}",
            crate::tracks::OLDER_FILE
        ));
    }
    built.warnings.extend(warnings);
}

/// A file an earlier session created that still holds exactly what it wrote
/// is a leftover to clear, not a game file to replace.
pub(super) fn left_over(store: &Store, game: &Game, rel: &str) -> bool {
    let Ok(placed) = Placed::load(&store.root) else {
        return false;
    };
    let Some(p) = placed.get(rel).filter(|p| !p.replaced) else {
        return false;
    };
    let stock = Baseline::load_or_new(&store.root, &game.identity).is_ok_and(|b| b.files.contains_key(rel));
    !stock && sha256_opt(&game.dir.join(rel)).ok().flatten().as_deref() == Some(p.sha256.as_str())
}

impl Pipeline<'_, '_> {
    /// Builds the menu pictures of the kept tracks; the result is the menu resources the build keeps
    /// over an addon's own, by the tracks they belong to.
    pub(super) fn track_menus(&mut self, inputs: &Inputs) -> Result<Kept> {
        let packer = self.run.eager_packer();
        let (run, built, caches) = (self.run, &mut self.built, &mut self.caches);
        let store = run.store;
        let resolve = |rel: &str| run.original(rel);
        let ui_maps = inputs.tracks.ui_maps();
        let mut menu_kept = Kept::new();
        if !ui_maps.is_empty() {
            let ui = build_ui(&ui_maps, &resolve, &store.root.join("cache/menus"), &packer)?;
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
        Ok(menu_kept)
    }
}
