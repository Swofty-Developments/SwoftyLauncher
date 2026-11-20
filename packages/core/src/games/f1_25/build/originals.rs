use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_formats::gvdb::Gvdb;

use super::GVDB;
use crate::game_folder::baseline::{Baseline, Objects};
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
    Shipped::new(data, game).max_ids()
}

/// Puts a clean copy of the game file `rel` back where the build found the launcher's own changes of an
/// earlier session in the game's original: the game file when the record of originals holds a stale copy and the
/// game file differs from it, else the newest original backed up for another game build. None when no journal
/// may be open, F1 25 runs or no clean copy is on record.
pub fn heal_stale(data: &Path, game: &Game, rel: &str) -> Result<Option<String>> {
    if !crate::game_folder::journal::Journal::open(data)?.is_empty()
        || crate::platform::process::game_running_in(&game.dir)
    {
        return Ok(None);
    }
    let objects = Objects {
        root: data.join("baseline/objects"),
    };
    let mut baseline = Baseline::load_or_new(data, &game.identity)?;
    let path = game.dir.join(rel);
    let now = crate::game_folder::baseline::sha256_opt(&path)?;
    let recorded = baseline.files.get(rel).map(|r| r.sha256.clone());
    let Some(stale) = recorded.clone().or_else(|| now.clone()) else {
        return Ok(None);
    };
    if recorded.is_some() && now.as_ref().is_some_and(|n| *n != stale) {
        let size = std::fs::metadata(&path).map_err(crate::io_err(&path))?.len();
        let sha256 = objects.put_file(&path)?;
        baseline.files.insert(
            rel.to_owned(),
            crate::game_folder::baseline::FileRecord {
                size,
                sha256,
                source: crate::game_folder::baseline::Source::FirstSight,
            },
        );
        baseline.save(data)?;
        return Ok(Some(format!(
            "the recorded original of {rel} held the launcher's changes from an earlier session; the game folder's copy \
             is the original now"
        )));
    }
    let Some((build, clean)) = crate::game_folder::baseline::older_originals(data, &game.identity, rel)
        .into_iter()
        .find(|(_, r)| r.sha256 != stale)
    else {
        return Ok(None);
    };
    objects.restore_to(&clean.sha256, &path)?;
    if recorded.is_some() {
        baseline.files.insert(rel.to_owned(), clean);
        baseline.save(data)?;
    }
    Ok(Some(format!(
        "{rel} in the game folder still held the launcher's changes from an earlier session; put back its original \
         backed up for game build {build}"
    )))
}

#[cfg(test)]
thread_local! {
    pub(super) static SHIPPED_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The game's own database, read and parsed on first use, so the checks of one
/// build share a parse; none when it can't be read.
pub(super) struct Shipped<'a> {
    data: &'a Path,
    game: &'a Game,
    db: std::cell::OnceCell<Option<Gvdb>>,
}

impl<'a> Shipped<'a> {
    pub(super) fn new(data: &'a Path, game: &'a Game) -> Self {
        Self {
            data,
            game,
            db: std::cell::OnceCell::new(),
        }
    }

    pub(super) fn get(&self) -> Option<&Gvdb> {
        self.db
            .get_or_init(|| {
                #[cfg(test)]
                SHIPPED_READS.with(|n| n.set(n.get() + 1));
                std::fs::read(original(self.data, self.game, GVDB))
                    .ok()
                    .and_then(|b| Gvdb::parse(&b).ok())
            })
            .as_ref()
    }

    pub(super) fn max_ids(&self) -> BTreeMap<String, i64> {
        self.get().map(crate::row_ids::stock_max_ids).unwrap_or_default()
    }
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
    crate::game_folder::setup::redo(data)?;
    Err(Error::Other(crate::game_folder::setup::backups_missing(&missing)))
}
