use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::addons::other_game::Packed;
use crate::install::LOADER_DIR;
use crate::manifest::{AddonManifest, Category};
use crate::profile::conflicts::Pair;
use crate::session::SessionRecord;
use crate::store::Store;

use super::crash_record::report_name;

/// The most of a loader.log or crash report that is read.
pub(super) const READ_CAP: u64 = 32 << 20;

/// Of a file over [`READ_CAP`], the start that is kept: the boot, build and system lines.
const HEAD_CAP: u64 = 4 << 20;

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
    /// The enabled addons that put F1 25's packed game data, by id, in load order; only a crash reads them.
    pub packed: Vec<(String, Packed)>,
    /// The enabled addons whose installed version was withdrawn from the Addon Browser
    /// ([`crate::catalog::withdrawn`]): each id with that version, in load order.
    pub withdrawn: Vec<(String, String)>,
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
        self.withdrawn = withdrawn_on(store, &profile, &self.enabled, &self.builtin, addons);
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

    /// The enabled addons that put F1 25's packed game data ([`crate::addons::other_game::scan`]).
    pub fn with_packed(mut self, store: &Store, addons: &BTreeMap<String, AddonManifest>) -> Self {
        self.packed = self
            .enabled
            .iter()
            .filter(|id| !self.builtin.contains(*id))
            .filter_map(|id| {
                let packed = crate::addons::other_game::scan(&store.addons_dir().join(id), addons.get(id)?)?;
                Some((id.clone(), packed))
            })
            .collect();
        self
    }
}

/// The addons on in `profile` whose installed version was withdrawn: those of its last build, or of
/// the profile itself when it has none.
pub(super) fn withdrawn_on(
    store: &Store,
    profile: &str,
    built: &[String],
    builtin: &BTreeSet<String>,
    addons: &BTreeMap<String, AddonManifest>,
) -> Vec<(String, String)> {
    let checks = crate::catalog::withdrawn::Checks::load(&store.root);
    if checks.0.values().all(|c| !c.withdrawn) {
        return Vec::new();
    }
    let saved: Vec<String> = match built.is_empty() {
        true => store
            .profile(profile)
            .map(|p| p.enabled().map(str::to_owned).collect())
            .unwrap_or_default(),
        false => Vec::new(),
    };
    let on = if built.is_empty() { &saved } else { built };
    checks
        .among(
            addons,
            on.iter().map(String::as_str).filter(|id| !builtin.contains(*id)),
        )
        .into_iter()
        .map(|w| (w.id, w.version.to_string()))
        .collect()
}

pub(super) fn evidence(session: &SessionRecord, game: Option<&Path>, store: &Store) -> Evidence {
    evidence_with(session, game, store, &store.addons().unwrap_or_default())
}

pub(super) fn evidence_with(
    session: &SessionRecord,
    game: Option<&Path>,
    store: &Store,
    addons: &BTreeMap<String, AddonManifest>,
) -> Evidence {
    let names = addons
        .iter()
        .map(|(id, m)| (id.clone(), m.name.clone()))
        .collect();
    Evidence::read(session, game, names, Some(&store.root)).with_profile(store, addons)
}

pub(crate) fn read_perf(dir: &Path) -> Option<String> {
    ["perf.log", "game-logs/perf.log"]
        .iter()
        .find_map(|rel| read_capped(&dir.join(rel)))
}

pub(super) fn read_capped(path: &Path) -> Option<String> {
    read_head_and_tail(std::fs::File::open(path).ok()?, READ_CAP, HEAD_CAP)
}

pub(super) fn read_head_and_tail<R: Read + Seek>(mut file: R, cap: u64, head: u64) -> Option<String> {
    let len = file.seek(SeekFrom::End(0)).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    if len <= cap {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        return Some(String::from_utf8_lossy(&bytes).into_owned());
    }
    let mut start = vec![0; head as usize];
    file.read_exact(&mut start).ok()?;
    let keep = start.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    start.truncate(keep);
    let tail_len = cap - head;
    file.seek(SeekFrom::Start(len - tail_len)).ok()?;
    let mut end = Vec::with_capacity(tail_len as usize);
    file.read_to_end(&mut end).ok()?;
    let skip = end.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut text = String::from_utf8_lossy(&start).into_owned();
    text.push_str(&format!(
        "[log] {} bytes of the middle of this log were not read\n",
        len - start.len() as u64 - (end.len() - skip) as u64
    ));
    text.push_str(&String::from_utf8_lossy(&end[skip..]));
    Some(text)
}
