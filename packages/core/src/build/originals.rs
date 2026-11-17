use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_formats::gvdb::Gvdb;

use super::GVDB;
use crate::baseline::{Baseline, Objects};
use crate::install::Game;
use crate::{Error, Result};

/// The recorded originals of the last baseline read, by its path and when it
/// was written: a build looks originals up dozens of times.
type Originals = (
    PathBuf,
    (std::time::SystemTime, u64),
    std::sync::Arc<BTreeMap<String, String>>,
);
static ORIGINALS: std::sync::Mutex<Option<Originals>> = std::sync::Mutex::new(None);

fn originals(data: &Path, game: &Game) -> std::sync::Arc<BTreeMap<String, String>> {
    let path = Baseline::path(data, &game.identity);
    let stamp = std::fs::metadata(&path)
        .and_then(|m| Ok((m.modified()?, m.len())))
        .ok();
    let mut cached = ORIGINALS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, s, files)) = cached.as_ref()
        && *p == path
        && Some(*s) == stamp
    {
        return files.clone();
    }
    let files: std::sync::Arc<BTreeMap<String, String>> = std::sync::Arc::new(
        Baseline::load_or_new(data, &game.identity)
            .map(|b| b.files.into_iter().map(|(rel, rec)| (rel, rec.sha256)).collect())
            .unwrap_or_default(),
    );
    *cached = stamp.map(|stamp| (path, stamp, files.clone()));
    files
}

pub fn original(data: &Path, game: &Game, rel: &str) -> PathBuf {
    match originals(data, game).get(rel) {
        Some(sha256) => Objects {
            root: data.join("baseline/objects"),
        }
        .path(sha256),
        None => game.dir.join(rel),
    }
}

/// The highest stat id the stock game database gives each team, vehicle and driver table
/// (`row_ids::stock_max_ids`). Empty when the database can't be read.
pub fn stock_max_ids(data: &Path, game: &Game) -> BTreeMap<String, i64> {
    std::fs::read(original(data, game, GVDB))
        .ok()
        .and_then(|b| Gvdb::parse(&b).ok())
        .map(|db| crate::row_ids::stock_max_ids(&db))
        .unwrap_or_default()
}

/// Stops a build whose record of original game files names backups that are
/// gone, and sends the player through the setup again, which starts the record
/// over.
pub(super) fn check_originals(data: &Path, game: &Game) -> Result<()> {
    let objects = Objects {
        root: data.join("baseline/objects"),
    };
    let missing: Vec<String> = originals(data, game)
        .iter()
        .filter(|(_, sha256)| !objects.has(sha256))
        .map(|(rel, _)| rel.clone())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    crate::setup::redo(data)?;
    Err(Error::Other(crate::setup::backups_missing(&missing)))
}
