use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Built, SAVED_FILE, SAVED_VERSION};
use crate::asset::Op;
use crate::baseline::Baseline;
use crate::builtin::{loader_dir, loader_files};
use crate::conflicts::Report;
use crate::install::Game;
use crate::planner::Conflict;
use crate::settings::SessionSettings;
use crate::store::Store;
use crate::{Result, io_err};

/// The record of a finished build in its overlay folder, which a later build with the same
/// build hash reuses instead of building again.
#[derive(Serialize, Deserialize)]
pub(super) struct Saved {
    version: u32,
    buildhash: String,
    data: BTreeMap<String, PathBuf>,
    added: BTreeMap<String, PathBuf>,
    loader: BTreeMap<String, PathBuf>,
    registry_override: bool,
    sizes: BTreeMap<PathBuf, u64>,
    #[serde(default)]
    stamps: BTreeMap<PathBuf, (u128, u64)>,
    settings: Option<SessionSettings>,
    lines: Vec<String>,
    warnings: Vec<String>,
    conflicts: Vec<Conflict>,
    waiting: Vec<Op>,
    #[serde(default)]
    report: Option<Report>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    row_ids: BTreeMap<String, BTreeMap<String, i64>>,
}

fn output_stamp(meta: &std::fs::Metadata) -> (u128, u64) {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(meta);
    #[cfg(not(unix))]
    let ino = 0;
    (modified, ino)
}

impl Saved {
    pub(super) fn of(built: &Built, lines: usize, warnings: usize) -> Result<Self> {
        let mut sizes = BTreeMap::new();
        let mut stamps = BTreeMap::new();
        for src in [&built.plan.data, &built.plan.added]
            .into_iter()
            .flat_map(|m| m.values())
        {
            let meta = std::fs::metadata(src).map_err(io_err(src))?;
            sizes.insert(src.clone(), meta.len());
            stamps.insert(src.clone(), output_stamp(&meta));
        }
        Ok(Self {
            version: SAVED_VERSION,
            buildhash: built.buildhash.clone(),
            data: built.plan.data.clone(),
            added: built.plan.added.clone(),
            loader: built_files(&built.plan.loader, loader_dir().as_deref()),
            registry_override: built.plan.registry_override,
            sizes,
            stamps,
            settings: built.settings.clone(),
            lines: built.lines[lines..].to_vec(),
            warnings: built.warnings[warnings..].to_vec(),
            conflicts: built.conflicts.clone(),
            waiting: built.waiting.clone(),
            report: Some(built.report.clone()),
            row_ids: built.row_ids.clone(),
        })
    }

    pub(super) fn load(overlay: &Path, buildhash: &str) -> Option<Self> {
        let saved: Self = crate::read_json(&overlay.join(SAVED_FILE)).ok()?;
        (saved.version == SAVED_VERSION && saved.buildhash == buildhash).then_some(saved)
    }

    // The game folder decides whether a file is replaced or added, so a saved
    // plan only holds while each file it adds is still absent and each file it
    // replaces is still a game file.
    pub(super) fn usable(&self, store: &Store, game: &Game) -> bool {
        let sources = self
            .sizes
            .iter()
            .all(|(p, len)| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() == *len));
        let untouched = self
            .stamps
            .iter()
            .all(|(p, stamp)| std::fs::metadata(p).is_ok_and(|m| output_stamp(&m) == *stamp));
        if !sources
            || !untouched
            || !self.loader.values().all(|p| p.is_file())
            || self
                .added
                .keys()
                .any(|rel| std::fs::symlink_metadata(game.dir.join(rel)).is_ok())
        {
            return false;
        }
        let Ok(baseline) = Baseline::load_or_new(&store.root, &game.identity) else {
            return false;
        };
        self.data
            .keys()
            .all(|rel| baseline.files.contains_key(rel) || game.dir.join(rel).is_file())
    }

    /// The loader's own files come from the launcher's loader folder as it is
    /// now: a launcher update changes them without changing what the build
    /// wrote. The rest of the placed files, such as screens, runtime assets and
    /// addon plugins, are the saved ones.
    pub(super) fn restore(self, built: &mut Built) -> Result<()> {
        built.plan.data = self.data;
        built.plan.added = self.added;
        built.plan.loader = self.loader;
        if let Some(dir) = loader_dir() {
            built.plan.loader.extend(loader_files(&dir)?);
        }
        built.plan.registry_override = self.registry_override;
        built.settings = self.settings;
        built.lines.push(format!(
            "build: nothing changed since overlay {} was built; reusing it",
            built.buildhash
        ));
        built.lines.extend(self.lines);
        built.warnings.extend(self.warnings);
        built.report = match self.report {
            Some(report) => report,
            None => crate::conflicts::summarise(&built.plan.profile, &self.conflicts, &|_| None).0,
        };
        built.report.built_at = crate::conflicts::now();
        built.conflicts = self.conflicts;
        built.waiting = self.waiting;
        built.row_ids = self.row_ids;
        Ok(())
    }
}

/// The placed files that do not come from the loader folder `loader`.
pub(super) fn built_files(
    placed: &BTreeMap<String, PathBuf>,
    loader: Option<&Path>,
) -> BTreeMap<String, PathBuf> {
    placed
        .iter()
        .filter(|(_, src)| loader.is_none_or(|dir| !src.starts_with(dir)))
        .map(|(rel, src)| (rel.clone(), src.clone()))
        .collect()
}
