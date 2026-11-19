use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::buildhash::small_file_digest;
use super::{Built, SAVED_FILE, SAVED_VERSION};
use crate::addons::builtin::{loader_dir, loader_files};
use crate::addons::settings::SessionSettings;
use crate::asset::Op;
use crate::diagnostics::phases;
use crate::game_folder::baseline::Baseline;
use crate::install::Game;
use crate::planner::Conflict;
use crate::profile::conflicts::Report;
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
    /// The SHA-256 of each small source: a build hook rewrites its small outputs on every build, which
    /// moves their stamps but leaves them the same file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    digests: BTreeMap<PathBuf, String>,
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
        let mut digests = BTreeMap::new();
        for src in [&built.plan.data, &built.plan.added]
            .into_iter()
            .flat_map(|m| m.values())
        {
            let meta = std::fs::metadata(src).map_err(io_err(src))?;
            sizes.insert(src.clone(), meta.len());
            stamps.insert(src.clone(), output_stamp(&meta));
            if let Some(sha) = small_file_digest(src, &meta)? {
                digests.insert(src.clone(), sha);
            }
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
            digests,
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
        let why = match crate::read_json::<Self>(&overlay.join(SAVED_FILE)) {
            Ok(saved) if saved.version == SAVED_VERSION && saved.buildhash == buildhash => {
                return Some(saved);
            }
            Ok(saved) if saved.version != SAVED_VERSION => "it was saved in another format",
            Ok(_) => "it was saved for other inputs",
            Err(_) => "nothing was saved for it",
        };
        phases::note(format!("reuse no {buildhash}: {why}"));
        None
    }

    /// Whether this saved build still holds, with the reason in the Play log either way.
    pub(super) fn usable(&self, store: &Store, game: &Game) -> bool {
        match self.unusable(store, game) {
            None => {
                phases::note(format!("reuse yes {}", self.buildhash));
                true
            }
            Some(why) => {
                phases::note(format!("reuse no {}: {why}", self.buildhash));
                false
            }
        }
    }

    // The game folder decides whether a file is replaced or added, so a saved
    // plan only holds while each file it adds is still absent and each file it
    // replaces is still a game file.
    fn unusable(&self, store: &Store, game: &Game) -> Option<String> {
        for (p, len) in &self.sizes {
            match std::fs::metadata(p) {
                Ok(m) if m.is_file() && m.len() == *len => {}
                Ok(_) => return Some(format!("{} changed size", p.display())),
                Err(_) => return Some(format!("{} is gone", p.display())),
            }
        }
        for (p, stamp) in &self.stamps {
            let Ok(meta) = std::fs::metadata(p) else {
                return Some(format!("{} is gone", p.display()));
            };
            if output_stamp(&meta) == *stamp {
                continue;
            }
            let same = self
                .digests
                .get(p)
                .is_some_and(|want| small_file_digest(p, &meta).ok().flatten().as_ref() == Some(want));
            if !same {
                return Some(format!("{} was changed after the build", p.display()));
            }
        }
        if let Some(p) = self.loader.values().find(|p| !p.is_file()) {
            return Some(format!("{} is gone", p.display()));
        }
        if let Some(rel) = self
            .added
            .keys()
            .find(|rel| std::fs::symlink_metadata(game.dir.join(rel)).is_ok())
        {
            return Some(format!("{rel} is already in the game folder"));
        }
        let Ok(baseline) = Baseline::load_or_new(&store.root, &game.identity) else {
            return Some("the record of the original game files can't be read".to_owned());
        };
        self.data
            .keys()
            .find(|rel| !baseline.files.contains_key(*rel) && !game.dir.join(rel).is_file())
            .map(|rel| format!("{rel} is not a game file"))
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
            None => crate::profile::conflicts::summarise(&built.plan.profile, &self.conflicts, &|_| None).0,
        };
        built.report.built_at = crate::profile::conflicts::now();
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
