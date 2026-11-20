use std::collections::BTreeMap;
use std::path::PathBuf;

use swofty_formats::gvdb::Gvdb;

use super::addons::{Gather, Gathered, experimental_features};
use super::buildhash::BuildHash;
use super::digests::Digests;
use super::hooks::Hooks;
use super::registry::replaced_operations;
use super::run::Run;
use super::setup::Setup;
use super::tracks::{Tracks, record_ui_lists};
use super::{Built, GVDB, Pipeline, picture, registry};
use crate::activation::handling::{self, HandlingMap};
use crate::activation::ui::{Lists, MapUi, stamp as ui_stamp};
use crate::addons::builtin::loader_dir;
use crate::addons::registry::Registry;
use crate::addons::settings::SessionMod;
use crate::asset::{AssetOp, ItemOp};
use crate::content::runtime_assets::Runtime;
use crate::content::shaders::BuildFiles;
use crate::game_folder::baseline::sha256_file;
use crate::online::multiplayer::fingerprint;
use crate::row_ids::Overrides;
use crate::stdlib::deps::Std;
use crate::ui::{ERPS as MENU_ERPS, READS as MENU_READS, ui_file};
use crate::{Result, io_err};

/// Everything a build reads besides its ops, which its build hash is the digest of along with them.
pub(super) struct Inputs {
    pub tracks: Tracks,
    pub handling_maps: Vec<HandlingMap>,
    /// The livery archives, which wear the handling of the car they are for.
    pub liveries: Vec<handling::Livery>,
    /// The menu lists the track records' pictures go in.
    pub record_ui: Lists,
    /// What the build made of each addon, for its fingerprint.
    pub outputs: BTreeMap<String, fingerprint::Output>,
    /// The Driver Manager drivers each DB Editor package picks, by the package's name.
    pub driver_picks: Vec<(String, Vec<String>)>,
    pub hud_inputs: Vec<crate::hud::Input>,
    pub shader_files: BuildFiles,
    pub std_index: Option<crate::stdlib::index::Placed>,
    /// The record room a standard library addon asks the database for.
    pub room_asked: Option<usize>,
    /// A room launch's lent row ids, when they concern the addons on.
    pub lent_ids: Option<Overrides>,
    /// The game database the build edits: the original one.
    pub db_src: PathBuf,
    /// The in-game loader is there, so the launcher's menus are built.
    pub menus: bool,
    /// The careers the Resume Career list shows, when the list is on.
    pub resume_save: Option<crate::stdlib::careers::ListSave>,
    /// How many lines and warnings the build had before the ones a saved build keeps.
    pub saved_from: (usize, usize),
}

pub(super) fn stamp_resume_list(
    hash: &mut BuildHash,
    data: &std::path::Path,
    profile: &crate::profile::Profile,
    game: &crate::install::Game,
    wanted: bool,
) -> Result<Option<crate::stdlib::careers::ListSave>> {
    if !wanted {
        hash.update("resume list|off;");
        return Ok(None);
    }
    let careers = crate::stdlib::careers::data_path(data);
    let stamp = match careers.is_file() {
        true => sha256_file(&careers)?,
        false => "absent".to_owned(),
    };
    let ids = match crate::stdlib::reports::found(data, Some(&game.dir), crate::stdlib::reports::IDS) {
        Some(p) => sha256_file(&p)?,
        None => "absent".to_owned(),
    };
    let launcher_saves = profile.launcher_saves && !profile.is_vanilla();
    let saved = crate::stdlib::careers::list_save(data, &profile.name, launcher_saves, game);
    hash.update(format!("resume list|{stamp}|{ids}|{};", saved.state()));
    Ok(Some(saved))
}

impl Inputs {
    /// Reads every addon's ops into the planner and the rest of what the build reads, and names the
    /// build by the digest of it all.
    pub fn read<'a, 'p>(
        run: Run<'a>,
        setup: &'p Setup<'a>,
        registry: &Registry,
        hooks: &mut Hooks,
        std16: &mut BTreeMap<String, Std>,
        lent: bool,
        mut built: Built,
    ) -> Result<(Pipeline<'a, 'p>, Self)> {
        let (store, game, profile) = (run.store, run.game, run.profile);
        let (addons, p, on) = (run.addons, run.settings, run.on);
        let mut planner = setup.planner(run);
        let mut hash = BuildHash::with_digests(Digests::load(source_digests_path(store, profile)));
        setup.hash(&mut hash);
        let mut runtime = Runtime::new(planner.features());
        experimental_features(run, &mut built, &mut hash, &mut runtime);
        let shader_files = picture::grade(run, &mut built, &mut hash, &mut runtime)?;
        let replaced = replaced_operations(std16, loader_dir().as_deref());
        let mut gathered = Gather {
            run,
            registry,
            hooks,
            features: &setup.features,
            packages: &setup.packages,
            replaced: &replaced,
            planner: &mut planner,
            built: &mut built,
            hash: &mut hash,
        }
        .gather()?;
        registry::early_stage(run, std16, gathered.left_to_core, &mut built, &mut runtime);
        let head_puts = std::mem::take(&mut gathered.head_puts);
        let gathered_rest = gathered;
        if head_puts.pending() {
            let path = run.original(GVDB);
            let stock =
                crate::game_db::heads::labels(&Gvdb::parse(&std::fs::read(&path).map_err(io_err(&path))?)?);
            let added =
                crate::game_db::heads::ops(&head_puts.new_heads(&stock), &stock, head_puts.registered());
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
        let mut fitted = Tracks::fit(run, &mut planner, &mut built, std16)?;
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
        fitted.take_slots(&planner, setup.exe_dir.as_deref(), setup.loader.is_some())?;
        fitted.hash(&mut hash)?;
        let resolve = |rel: &str| run.original(rel);
        let addon_dir = |id: &str| run.addon_dir(id);
        let handling::Read {
            maps: handling_maps,
            notes: handling_notes,
            liveries,
        } = {
            let dirs: Vec<(String, PathBuf)> =
                run.order.iter().map(|id| (id.clone(), addon_dir(id))).collect();
            let options = |addon: &str, name: &str| {
                let m = addons.get(addon)?;
                m.options.iter().find(|o| o.id == name).map(|o| on.value(p, m, o))
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
        let caches = Vec::new();
        let mut session = match lent {
            true => store.session_settings_of(p)?,
            false => store.session_settings(profile)?,
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
        let mut resume_save = None;
        if menus {
            for rel in MENU_ERPS.iter().chain(MENU_READS) {
                let p = resolve(&ui_file(rel));
                let meta = std::fs::metadata(&p).map_err(io_err(&p))?;
                hash.update(format!("{rel}|{}|{:?}", meta.len(), meta.modified().ok()));
            }
            resume_save = stamp_resume_list(
                &mut hash,
                &store.root,
                p,
                game,
                crate::stdlib::careers::resume_list_wanted(),
            )?;
        }
        let db_src = run.original(GVDB);
        hash.update(sha256_file(&db_src)?);
        let order = run.manifests();
        hash.unread(store, game, &order, planner.erp_ops(), &resolve)?;
        let std_index = loader_dir()
            .is_some()
            .then(|| {
                crate::stdlib::index::Index::plan(profile, run.order, run.addons, std16, &store.addons_dir())
            })
            .flatten();
        if let Some(placed) = &std_index {
            hash.update(crate::stdlib::index::Index::stamp(placed)?);
            for id in run.order {
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
            .any(|s| run.order.contains(&s.id) && s.is_std())
            .then(|| {
                crate::stdlib::room::asked(
                    crate::stdlib::reports::Room::read(&store.root, Some(&game.dir)).as_ref(),
                    profile,
                    run.order,
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
            run.order,
            &|id: &str| run.name(id),
        );
        let lent_ids = crate::row_ids::lent().filter(|o| o.stamp(run.order).is_some());
        if let Some(stamp) = lent_ids.as_ref().and_then(|o| o.stamp(run.order)) {
            hash.update(stamp);
        }
        hash.save_digests();
        built.buildhash = hash.finish();
        built.overlay = store.overlay_dir(profile, &built.buildhash);
        built.warnings.extend(core_warning);
        let saved_from = (built.lines.len(), built.warnings.len());
        let Gathered {
            outputs,
            driver_picks,
            hud_inputs,
            ..
        } = gathered_rest;
        let inputs = Self {
            tracks: fitted,
            handling_maps,
            liveries,
            record_ui,
            outputs,
            driver_picks,
            hud_inputs,
            shader_files,
            std_index,
            room_asked,
            lent_ids,
            db_src,
            menus,
            resume_save,
            saved_from,
        };
        let pipeline = Pipeline {
            run,
            planner,
            runtime,
            built,
            session,
            caches,
        };
        Ok((pipeline, inputs))
    }
}

/// Where a profile's small build sources' digests are kept between its builds.
/// Where the reply of `addon`'s build hook is kept for the profile's next build.
pub(super) fn hook_reply_path(store: &crate::store::Store, profile: &str, addon: &str) -> PathBuf {
    store
        .root
        .join("state")
        .join("profiles")
        .join(profile)
        .join("hooks")
        .join(format!("{addon}.json"))
}

pub(super) fn source_digests_path(store: &crate::store::Store, profile: &str) -> PathBuf {
    store
        .root
        .join("state")
        .join("profiles")
        .join(profile)
        .join("source-digests.json")
}
