//! The plain store: the launcher's copy of a profile's save containers, written by the core from inside
//! the game as plain data and read here.
//!
//! ```text
//! <plain root>/
//!   CURRENT                         the newest complete generation, decimal text
//!   generations/000000000041/
//!     manifest.json
//!     containers/<fnv64>.bag
//!   inject.json                     launcher requests for the core
//!   inject.applying.json            the requests a profile load took and has not answered yet
//!   injected.json                   the core's answers
//!   pins.json                       generations the launcher keeps (launcher-owned)
//! ```
//!
//! A generation counts only when it is not newer than `CURRENT`, its manifest parses and every container
//! file it lists is there with the length the manifest states. A torn or partial generation is skipped.

pub mod inject;
mod loss;
mod manifest;
mod prune;
pub mod rewrite;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use loss::{Loss, Lost, PutBack, put_backs};
pub use manifest::{Container, Event, Manifest, SCHEMA, SLOT_SUFFIX, Slot, Source, mode_of};
pub use prune::{Keep, Pruned, prune_all_held};

use crate::{Result, io_err};

pub const DIR: &str = "plain";
pub const CURRENT: &str = "CURRENT";
pub const GENERATIONS: &str = "generations";
const PINS: &str = "pins.json";

pub fn root_of_profile(data: &Path, profile: &str) -> PathBuf {
    crate::saves::launcher::root(data).join(profile).join(DIR)
}

pub fn root_of_copy(data: &Path, source: &str) -> PathBuf {
    data.join(DIR).join(source)
}

/// The plain root of a vault source: `launcher-<profile>` keeps it beside the profile's launcher save,
/// `steam-<id3>` and `ea-<user>` under `$DATA/plain/`.
pub fn root_of_source(data: &Path, source: &str) -> PathBuf {
    if let Some(p) = source
        .strip_prefix("launcher-ea.")
        .or_else(|| source.strip_prefix("launcher-"))
    {
        return root_of_profile(data, p);
    }
    root_of_copy(data, source)
}

const COPY_SOURCE: &str = "state/save-source";

/// Note the vault source of the save the active copy uses, which the launcher finds at start and before Play,
/// so the settings of a session on the copy's own save name its plain root without looking for the copy.
pub fn remember_copy_source(data: &Path, source: &str) {
    let path = data.join(COPY_SOURCE);
    if std::fs::read_to_string(&path).is_ok_and(|s| s.trim() == source) {
        return;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&path, source).ok();
}

/// The plain root a session of `profile` writes, which `settings.json` hands the core. A session on the copy's
/// own save has none until the launcher has found the copy's save once.
pub fn root_of_session(data: &Path, profile: &str, on_launcher_saves: bool) -> Option<PathBuf> {
    match on_launcher_saves {
        true => Some(root_of_profile(data, profile)),
        false => {
            let source = std::fs::read_to_string(data.join(COPY_SOURCE)).ok()?;
            let source = source.trim();
            (!source.is_empty()).then(|| root_of_copy(data, source))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plain {
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerRow {
    pub mode: String,
    pub container: String,
    pub registered: bool,
    pub source: Source,
    pub slot: Slot,
    pub generation: u64,
    pub event: Event,
}

impl Plain {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn of_source(data: &Path, source: &str) -> Self {
        Self::at(root_of_source(data, source))
    }

    pub fn current(&self) -> Option<u64> {
        std::fs::read_to_string(self.root.join(CURRENT))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    pub fn generation_dir(&self, n: u64) -> PathBuf {
        self.root.join(GENERATIONS).join(format!("{n:012}"))
    }

    pub fn bag_path(&self, n: u64, c: &Container) -> PathBuf {
        self.generation_dir(n).join("containers").join(c.file())
    }

    fn numbers(&self) -> Vec<u64> {
        let Ok(read) = std::fs::read_dir(self.root.join(GENERATIONS)) else {
            return Vec::new();
        };
        let mut out: Vec<u64> = read
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .collect();
        out.sort_unstable_by(|a, b| b.cmp(a));
        out
    }

    pub fn manifest(&self, n: u64) -> Option<Manifest> {
        let bytes = std::fs::read(self.generation_dir(n).join("manifest.json")).ok()?;
        let m: Manifest = serde_json::from_slice(&bytes).ok()?;
        (m.generation == n && m.schema == SCHEMA && self.complete(&m)).then_some(m)
    }

    fn complete(&self, m: &Manifest) -> bool {
        m.containers.iter().all(|c| {
            std::fs::metadata(self.bag_path(m.generation, c)).is_ok_and(|f| f.is_file() && f.len() == c.bytes)
        })
    }

    pub fn generations(&self) -> Vec<Manifest> {
        let Some(current) = self.current() else {
            return Vec::new();
        };
        self.numbers()
            .into_iter()
            .filter(|n| *n <= current)
            .filter_map(|n| self.manifest(n))
            .collect()
    }

    pub fn newest(&self) -> Option<Manifest> {
        let current = self.current()?;
        self.numbers()
            .into_iter()
            .filter(|n| *n <= current)
            .find_map(|n| self.manifest(n))
    }

    pub fn careers(&self) -> Vec<CareerRow> {
        let Some(m) = self.newest() else {
            return Vec::new();
        };
        let mut out: Vec<CareerRow> = m
            .containers
            .iter()
            .flat_map(|c| {
                c.slots.iter().map(|s| CareerRow {
                    mode: c.mode(),
                    container: c.name.clone(),
                    registered: c.registered,
                    source: c.source,
                    slot: s.clone(),
                    generation: m.generation,
                    event: m.event,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.mode
                .cmp(&b.mode)
                .then(b.slot.last_played.cmp(&a.slot.last_played))
        });
        out
    }

    pub fn pins(&self) -> BTreeMap<u64, Vec<String>> {
        crate::read_json::<BTreeMap<String, Vec<String>>>(&self.root.join(PINS))
            .map(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn pin(&self, n: u64, why: &str) -> Result<()> {
        let mut pins = self.pins();
        let whys = pins.entry(n).or_default();
        if whys.iter().any(|w| w == why) {
            return Ok(());
        }
        whys.push(why.to_owned());
        std::fs::create_dir_all(&self.root).map_err(io_err(&self.root))?;
        let out: BTreeMap<String, Vec<String>> = pins.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        crate::write_json(&self.root.join(PINS), &out)
    }

    pub fn pending(&self) -> Vec<inject::Request> {
        inject::pending(&self.root)
    }

    pub fn answers(&self) -> Vec<inject::Answer> {
        inject::answers(&self.root)
    }

    pub fn applying(&self) -> Vec<inject::Request> {
        inject::applying(&self.root)
    }

    /// Ask the core to put a mode back from `generation`: both halves, `gm_<mode>` and
    /// `<mode>_SaveSlotGameModeComponent`, in one request each from the same generation. `mode` may also be
    /// the name of either half.
    pub fn restore_mode(&self, generation: u64, mode: &str, now: i64) -> Result<Vec<inject::Request>> {
        let mode = mode_of(mode);
        let m = self.manifest(generation).ok_or_else(|| {
            crate::Error::Other(format!(
                "generation {generation} is not complete in the plain store"
            ))
        })?;
        let names: Vec<String> = m.halves(&mode).map(|c| c.name.clone()).collect();
        if names.is_empty() {
            return Err(crate::Error::Other(format!(
                "generation {generation} has no containers of mode {mode}"
            )));
        }
        self.pin(generation, "restore")?;
        names
            .iter()
            .map(|n| inject::ask(&self.root, generation, n, true, now))
            .collect()
    }
}

/// Withdraw the requests every plain store under `data` holds from a profile load that stopped before it
/// answered them, so the next Play does not apply them again. Only while F1 25 is closed. Lines for the
/// log.
pub fn withdraw_all_unanswered(data: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for root in prune::roots(data) {
        match inject::withdraw_unanswered(&root) {
            Ok(withdrawn) => out.extend(withdrawn.iter().map(|r| {
                format!(
                    "saves: withdrew restore request {} ({} from generation {}): F1 25 stopped while it was applied, so the \
                     next Play loads the save without it",
                    r.id, r.container, r.generation
                )
            })),
            Err(e) => out.push(format!(
                "saves: could not withdraw the unanswered restore requests of {}: {e}",
                root.display()
            )),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
