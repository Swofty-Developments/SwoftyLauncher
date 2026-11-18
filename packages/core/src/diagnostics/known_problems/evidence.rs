use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use crate::install::LOADER_DIR;
use crate::manifest::{AddonManifest, Category};
use crate::profile::conflicts::Pair;
use crate::session::SessionRecord;
use crate::store::Store;

use super::crash_record::report_name;

/// The most of a loader.log or crash report that is read.
pub(super) const READ_CAP: u64 = 32 << 20;

/// What a crashed session left behind, as the rules read it.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// The session's loader.log.
    pub loader: String,
    /// The crash report the core wrote for the session, when it can be found.
    pub report: Option<String>,
    /// The session's perf.log.
    pub perf: Option<String>,
    /// The names of the files and folders in the game folder.
    pub game_files: Vec<String>,
    /// Addon names by id, for addons the rules name.
    pub names: BTreeMap<String, String>,
    /// The installed version of each addon, by id.
    pub versions: BTreeMap<String, String>,
    /// The profile's enabled addons in load order, from its last build.
    pub enabled: Vec<String>,
    /// The enabled addons that are tracks.
    pub tracks: Vec<String>,
    /// The installed track and season addons the profile has off.
    pub off: Vec<String>,
    /// Which addon overrode which in the profile's last build.
    pub conflicts: Vec<Pair>,
    /// Per addon id, the game's entrant lists it renamed or removed that the last build kept.
    pub kept_lists: BTreeMap<String, Vec<String>>,
    /// The installed addons that are built in or managed by the launcher, which a profile can't turn off.
    pub builtin: BTreeSet<String>,
    /// The enabled addons' native plugins: the id of the addon that ships each, by the dll's file name in lower case.
    pub plugins: BTreeMap<String, String>,
    /// The game's exit code, or the crash code the session recorded.
    pub code: Option<u32>,
    /// The game exe's version, when it can be read.
    pub game_version: Option<String>,
    /// The game exe's size and PE link timestamp, the pair that identifies a build.
    pub game_build: Option<(u64, u32)>,
}

impl Evidence {
    pub fn read(
        session: &SessionRecord,
        game: Option<&Path>,
        names: BTreeMap<String, String>,
        data: Option<&Path>,
    ) -> Self {
        let loader = ["loader.log", "game-logs/loader.log"]
            .iter()
            .map(|rel| session.dir.join(rel))
            .find(|p| p.is_file())
            .and_then(|p| read_capped(&p))
            .unwrap_or_default();
        let perf = read_perf(&session.dir);
        let report = report_name(&loader).and_then(|name| {
            let mut dirs = vec![session.dir.clone(), session.dir.join("game-logs")];
            if let Some(game) = game {
                dirs.insert(0, game.join(LOADER_DIR));
            }
            if let Some(data) = data {
                dirs.push(crate::diagnostics::crash_report::fallback_dir(data));
            }
            dirs.iter()
                .map(|d| d.join(&name))
                .find(|p| p.is_file())
                .and_then(|p| read_capped(&p))
        });
        let game_files = game
            .and_then(|g| std::fs::read_dir(g).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let code = session
            .end
            .as_ref()
            .and_then(|e| e.crash_code.or(e.exit_code.map(|c| c as u32)))
            .or(session.exit_code.map(|c| c as u32));
        let game_version = game.and_then(|g| crate::game_folder::exe_version::read(&g.join(crate::GAME_EXE)));
        let game_build = game.and_then(|g| crate::build_support::identity(&g.join(crate::GAME_EXE)));
        Self {
            loader,
            report,
            perf,
            game_files,
            names,
            code,
            game_version,
            game_build,
            ..Default::default()
        }
    }

    /// The profile's addons from its last build in `store`.
    pub fn with_profile(mut self, store: &Store, addons: &BTreeMap<String, AddonManifest>) -> Self {
        self.builtin = addons
            .keys()
            .filter(|id| store.is_builtin(id) || crate::profile::order::is_pinned(id))
            .cloned()
            .collect();
        self.versions = addons
            .iter()
            .map(|(id, m)| (id.clone(), m.version.to_string()))
            .collect();
        let profile = store.active_profile();
        if let Ok(p) = store.profile(&profile) {
            self.off = p
                .addons
                .into_iter()
                .filter(|a| !a.enabled)
                .filter(|a| {
                    addons
                        .get(&a.id)
                        .is_some_and(|m| matches!(m.category, Some(Category::Tracks | Category::Seasons)))
                })
                .map(|a| a.id)
                .collect();
        }
        if let Some(report) = crate::profile::conflicts::last(store, &profile) {
            self.tracks = report
                .order
                .iter()
                .filter(|id| {
                    addons
                        .get(*id)
                        .is_some_and(|m| m.category == Some(Category::Tracks))
                })
                .cloned()
                .collect();
            self.enabled = report.order;
            self.conflicts = report.pairs;
            self.kept_lists = report.kept_lists;
        }
        self.plugins = self
            .enabled
            .iter()
            .filter(|id| !self.builtin.contains(*id))
            .filter_map(|id| Some((id, addons.get(id)?)))
            .flat_map(|(id, m)| m.operations.native.iter().map(move |p| (id, &p.dll)))
            .filter_map(|(id, dll)| {
                let file = dll.rsplit(['/', '\\']).next()?;
                Some((file.to_ascii_lowercase(), id.clone()))
            })
            .collect();
        self
    }
}

pub(super) fn evidence(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Evidence {
    let addons = store.addons().unwrap_or_default();
    let names = addons
        .iter()
        .map(|(id, m)| (id.clone(), m.name.clone()))
        .collect();
    Evidence::read(session, game, names, Some(&store.root)).with_profile(store, &addons)
}

pub(crate) fn read_perf(dir: &Path) -> Option<String> {
    ["perf.log", "game-logs/perf.log"]
        .iter()
        .find_map(|rel| read_capped(&dir.join(rel)))
}

pub(super) fn read_capped(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(READ_CAP)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}
