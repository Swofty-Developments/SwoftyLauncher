use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::database::DbEdits;
use super::inputs::Inputs;
use super::{ADDONS_DIR, Built, Pipeline};
use crate::activation::handling;
use crate::addons::builtin::{CORE_ID, LOADER_ENV, loader_dir, loader_files};
use crate::addons::settings::{SessionSettings, SessionUi, screen_file, screens};
use crate::asset::Kind;
use crate::asset::{AssetOp, LangOp, Op, Origin};
use crate::install::LOADER_DIR;
use crate::manifest::AddonManifest;
use crate::planner::{Route, Target};
use crate::stdlib::deps::Std;
use crate::store::Store;
use crate::{Error, Result, io_err};

// Page screens have no file: the ui build generates them.
pub fn screen_files(order: &[&AddonManifest], addons_dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
    Ok(screens(order.iter().copied())?
        .into_iter()
        .filter(|(_, s)| s.page.is_none())
        .map(|(m, s)| {
            (
                format!("{LOADER_DIR}/{}", screen_file(&s.name)),
                addons_dir.join(&m.id).join(&s.file),
            )
        })
        .collect())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plugins {
    pub files: BTreeMap<String, PathBuf>,
    pub dlls: Vec<String>,
}

pub fn plugin_files<'a>(
    store: &Store,
    order: impl IntoIterator<Item = &'a AddonManifest>,
    taken: impl IntoIterator<Item = String>,
) -> Result<Plugins> {
    let mut out = Plugins::default();
    let mut names: BTreeMap<String, String> = taken
        .into_iter()
        .map(|n| (n.to_ascii_lowercase(), CORE_ID.to_owned()))
        .collect();
    for m in order
        .into_iter()
        .filter(|m| !m.operations.native.is_empty() && !store.is_builtin(&m.id))
    {
        let dir = store.addons_dir().join(&m.id);
        for n in &m.operations.native {
            let name = n.dll.rsplit('/').next().unwrap_or(&n.dll);
            if let Some(other) = names.insert(name.to_ascii_lowercase(), m.id.clone()) {
                return Err(Error::Other(format!(
                    "{}: plugin {name} has the same file name as a plugin of {other}; rename one",
                    m.id
                )));
            }
            out.dlls.push(format!("{ADDONS_DIR}/{}/{}", m.id, n.dll));
            for rel in std::iter::once(&n.dll).chain(&n.files) {
                let rel = rel.trim_end_matches('/');
                let src = dir.join(rel);
                let found = if src.is_dir() {
                    loader_files(&src)?
                        .into_iter()
                        .filter(|(r, _)| !r.split('/').any(|p| p.starts_with('.')))
                        .map(|(r, p)| (format!("{rel}/{r}"), p))
                        .collect()
                } else if src.is_file() {
                    BTreeMap::from([(rel.to_owned(), src)])
                } else {
                    return Err(Error::Other(format!(
                        "{}: {rel} is missing; build the plugin and install the addon again",
                        m.id
                    )));
                };
                for (rel, src) in found {
                    out.files
                        .insert(format!("{LOADER_DIR}/{ADDONS_DIR}/{}/{rel}", m.id), src);
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn loader_line(exe_dir: Option<&Path>, loader: Option<&Path>) -> String {
    let show = |p: Option<&Path>| p.map_or_else(|| "none".to_owned(), |p| p.display().to_string());
    format!("loader: {}; swofty.exe in {}", show(loader), show(exe_dir))
}

pub fn build_menus(
    built: &mut Built,
    session: &mut SessionSettings,
    original: &dyn Fn(&str) -> PathBuf,
    writer: &crate::ui::Writer,
    lang: bool,
    archives: &crate::ui::ArchiveWriter,
) -> Result<Vec<(String, String)>> {
    let input = |rel: &str| built.plan.data.get(rel).cloned().unwrap_or_else(|| original(rel));
    let writer = |erp: &str, res: &str| {
        built
            .plan
            .data
            .contains_key(erp)
            .then(|| writer(erp, res))
            .flatten()
    };
    let menus = crate::ui::build(session, &input, &writer, &built.overlay, lang, archives)?;
    built.lines.push(format!(
        "ui: ADDON SETTINGS with {} screen(s) under {LOADER_DIR}/ui, {} game archive(s) patched",
        menus.session.len(),
        menus.erps.len()
    ));
    if !menus.strings.is_empty() {
        built.lines.push(format!(
            "ui: the online services popup reads its text from {} language string(s)",
            menus.strings.len()
        ));
    }
    built.lines.extend(menus.lines);
    built.warnings.extend(menus.warnings);
    built.plan.data.extend(menus.erps);
    for (rel, src) in menus.screens {
        if built.plan.loader.insert(rel.clone(), src).is_some() {
            return Err(Error::Other(format!("{rel} is placed twice")));
        }
    }
    let ui = session.ui.get_or_insert_with(|| SessionUi {
        register: true,
        strict: false,
        recover: true,
        screens: Vec::new(),
    });
    if let Some(s) = ui
        .screens
        .iter()
        .find(|s| menus.session.iter().any(|m| m.name == s.name))
    {
        return Err(Error::Other(format!(
            "screen {} is reserved for ADDON SETTINGS; rename it in its addon",
            s.name
        )));
    }
    ui.screens.retain(|s| s.page.is_none());
    ui.screens.splice(0..0, menus.session);
    Ok(menus.strings)
}

fn pending_english(store: &Store, planner: &crate::planner::Planner) -> Option<swofty_formats::lng::Lng> {
    let mut out = swofty_formats::lng::Lng::default();
    for op in planner.pending_lang() {
        let AssetOp::Lang(lang) = &op.op else { continue };
        match lang {
            LangOp::Set { language, key, value } if matches!(language.as_str(), "*" | "eng") => {
                out.entries.retain(|e| e.key != *key);
                out.entries.push(swofty_formats::lng::Entry {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
            LangOp::Table { language, source } if matches!(language.as_str(), "*" | "eng") => {
                let dir = super::source_dir(store, &op.origin.addon, source.root);
                let read = source
                    .resolve(&dir)
                    .ok()
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|b| swofty_formats::lng::Lng::parse(&b).ok());
                for e in read.into_iter().flat_map(|l| l.entries) {
                    out.entries.retain(|x| x.key != e.key);
                    out.entries.push(e);
                }
            }
            _ => {}
        }
    }
    (!out.entries.is_empty()).then_some(out)
}

impl Pipeline<'_, '_> {
    /// Places the in-game loader's files and what the build hands it: the launcher's menus, addon screens
    /// and plugins, the telemetry and row id maps and the registry index.
    pub(super) fn loader_files(
        &mut self,
        inputs: &Inputs,
        edits: &DbEdits,
        std16: &BTreeMap<String, Std>,
    ) -> Result<()> {
        let packer = self.run.eager_packer();
        let run = self.run;
        let store = run.store;
        let archive_cache = store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
        let archives = crate::ui::ArchiveWriter::new(&packer).cached(&archive_cache);
        let Self {
            planner,
            built,
            session,
            caches,
            ..
        } = self;
        let (udp_ids, id_aliases) = (&edits.udp_ids, &edits.id_aliases);
        let (lent_ids, std_index, menus) = (&inputs.lent_ids, &inputs.std_index, inputs.menus);
        let order = run.manifests();
        let resolve = |rel: &str| run.original(rel);
        let name = |origin: &str| run.origin_name(origin);
        match loader_dir() {
            Some(dir) => {
                built.plan.loader = loader_files(&dir)?;
                built.plan.registry_override = true;
                built.lines.push(format!(
                    "loader: {} file(s) from {}",
                    built.plan.loader.len(),
                    dir.display()
                ));
                if menus && crate::stdlib::careers::resume_list_wanted() {
                    let aliases = edits.id_aliases.clone();
                    let pending = pending_english(store, planner);
                    match crate::stdlib::careers::resume_list_for(
                        store,
                        run.game,
                        run.profile,
                        &built.overlay,
                        order.as_slice(),
                        aliases,
                        pending.as_ref(),
                    ) {
                        Some((needs, rows)) => {
                            built.lines.push(format!(
                                "ui: the Resume Career list screen has {} career(s); {} need something this build lacks",
                                rows.len(),
                                rows.iter().filter(|r| !r.ready).count()
                            ));
                            for r in rows.iter().filter(|r| r.driver.is_empty() || r.team.is_empty()) {
                                built.warnings.push(format!(
                                    "ui: career {} on the Career list has no {} name; its saved ids name no row in this build",
                                    r.key,
                                    if r.driver.is_empty() { "driver" } else { "team" }
                                ));
                            }
                            session.resume_needs = needs;
                            session.resume_list = rows;
                            session.resume_list_on = true;
                        }
                        None => built
                            .warnings
                            .push("ui: the Resume Career list screen is off: the build's database could not be read".into()),
                    }
                }
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
                    for (key, value) in build_menus(built, session, &resolve, &writer, lang, &archives)? {
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
                    let rel = format!("{LOADER_DIR}/{}", crate::game_db::udp_ids::FILE);
                    let src = built.overlay.join(crate::game_db::udp_ids::FILE);
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
                    if !run.checking {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64);
                        built.lines.extend(crate::saves::plain::rewrite::prepare(
                            &store.root,
                            run.profile,
                            session.launcher_saves.is_some(),
                            id_aliases,
                            now,
                        ));
                    }
                    built.lines.push(format!(
                        "database: {LOADER_DIR}/{} maps {n} earlier row id(s) to the ids the rows have now, so saves made under the earlier ids load",
                        crate::row_ids::ALIASES_FILE
                    ));
                }
                if let Some(ids) = lent_ids
                    .as_ref()
                    .and_then(|l| l.room_stat_ids(run.order))
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
                    let addon_dir = |id: &str| run.addon_dir(id);
                    let worn = handling::build_liveries(
                        &inputs.liveries,
                        &inputs.handling_maps,
                        &addon_dir,
                        &store.root.join("cache").join(crate::cache::LIVERIES_DIR),
                        &packer,
                    );
                    caches.extend(worn.dirs.iter().cloned());
                    for line in worn.lines.iter().cloned() {
                        match line.strip_prefix(crate::activation::wheel_check::WARNING_PREFIX) {
                            Some(w) => built.warnings.push(w.to_owned()),
                            None => built.lines.push(line),
                        }
                    }
                    built.warnings.extend(worn.warnings.iter().cloned());
                    let addons_at = format!("{LOADER_DIR}/{ADDONS_DIR}/");
                    for (rel, src) in placed
                        .files
                        .iter()
                        .map(|(r, s)| {
                            let edited = r
                                .strip_prefix(&addons_at)
                                .and_then(|at| at.split_once('/'))
                                .and_then(|(addon, file)| worn.file(addon, file));
                            (r.clone(), edited.unwrap_or(s).clone())
                        })
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
        Ok(())
    }
}
