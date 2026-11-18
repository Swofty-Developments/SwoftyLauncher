use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::game_folder::baseline::{Baseline, DepotHashes, Identity, Objects};
use crate::profile::VANILLA;

pub const PROXY: &str = "version.dll";
pub const LOADER_DIR: &str = "swofty";
pub const OVERRIDE_HEADER: &str = "[Software\\\\Wine\\\\AppDefaults\\\\F1_25.exe\\\\DllOverrides]";
const OVERRIDE_VALUE: &str = "\"version\"=\"native,builtin\"";

#[derive(Debug, Clone)]
pub struct Game {
    pub dir: PathBuf,
    pub user_reg: Option<PathBuf>,
    pub identity: Identity,
}

mod apply;
mod checks;
mod copy;
mod files;
mod originals;
mod record;
mod restore;
mod rollback;
mod sweep;
mod wine_override;

pub(crate) use files::copy_replacing;
use files::{quiet, unwatched};
pub use wine_override::{add_override, has_override};

impl Game {
    /// The game in `dir`, with the Proton prefix's user.reg when Steam
    /// installed it there, or the EA app's prefix from `launch.ea_prefix`.
    pub fn locate(data: &Path, dir: PathBuf) -> Result<Self> {
        let found = crate::platform::steam::find_game().filter(|g| g.dir == dir);
        let user_reg = found
            .as_ref()
            .and_then(|g| g.prefix())
            .map(|p| p.join("user.reg"))
            .filter(|p| p.is_file())
            .or_else(|| {
                (found.is_none() && !cfg!(windows) && Source::detect(&dir) == Source::EaApp)
                    .then(|| {
                        crate::store::Store::new(data.to_path_buf())
                            .config()
                            .launch
                            .ea_prefix
                    })
                    .flatten()
                    .and_then(|p| crate::platform::ea_app::prefix_user_reg(&p))
            });
        let identity = Identity::detect_cached(data, &dir, found.and_then(|g| g.build_id))?;
        Ok(Self {
            dir,
            user_reg,
            identity,
        })
    }
}

/// The store a game folder came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Steam,
    EaApp,
    Unknown,
}

impl Source {
    /// The store whose install `dir` is, by the first of [`crate::platform::PLATFORMS`] that claims it.
    pub fn detect(dir: &Path) -> Self {
        crate::platform::installed_by(dir).map_or(Self::Unknown, |p| p.source())
    }

    /// The store this source names; none for a folder no store installed.
    pub fn platform(self) -> Option<&'static dyn crate::platform::LaunchPlatform> {
        crate::platform::for_source(self)
    }
}

pub const EA_APP_EXPERIMENTAL: &str = "this is the EA app version of F1 25; Swofty's support for it is \
     experimental, so modded play may have bugs. Report them on the Swofty Developments Discord";

/// The `config.news_seen` key for the one-time EA app warning.
pub const EA_APP_SEEN: &str = "ea-app-experimental";

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub profile: String,
    pub data: BTreeMap<String, PathBuf>,
    pub added: BTreeMap<String, PathBuf>,
    pub loader: BTreeMap<String, PathBuf>,
    pub registry_override: bool,
}

impl Plan {
    pub fn vanilla() -> Self {
        Self {
            profile: VANILLA.into(),
            ..Self::default()
        }
    }

    pub fn is_vanilla(&self) -> bool {
        self.data.is_empty() && self.added.is_empty() && self.loader.is_empty() && !self.registry_override
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drift {
    Changed(String),
    Missing(String),
    Addition(String),
    RegistryOverride,
    OpenJournal(String),
    NewBuild { baseline: String, current: String },
    LeftInPlace(String),
    Stranded(String),
    NoOriginal(String),
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Drift::Changed(p) => write!(f, "{p} differs from the baseline"),
            Drift::Missing(p) => write!(f, "{p} is missing"),
            Drift::Addition(p) => write!(f, "{p} is not part of the game"),
            Drift::RegistryOverride => write!(f, "the prefix has the version.dll override"),
            Drift::OpenJournal(id) => write!(f, "journal {id} is open"),
            Drift::NewBuild { baseline, current } => {
                write!(f, "game build {current} differs from baseline {baseline}")
            }
            Drift::LeftInPlace(p) => write!(
                f,
                "{p} changed since it was written (game update?); left in place"
            ),
            Drift::Stranded(p) => write!(
                f,
                "{p} is not what the earlier session wrote in the old game folder nor its original; left in place"
            ),
            Drift::NoOriginal(p) => write!(
                f,
                "{p} still holds a file swofty wrote and its original is not on record; verify the game files in Steam"
            ),
        }
    }
}

/// A file in the way of a plan: a file the plan adds that is already there,
/// or a game file the plan replaces that is not the original. `owned` means
/// it is provably the launcher's own (the bytes it recorded writing there, or
/// the bytes it is about to write), so it can be cleared without asking.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub path: String,
    pub sha256: Option<String>,
    pub owned: bool,
    pub replaced: bool,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p = &self.path;
        match (self.replaced, self.owned) {
            (true, true) => write!(f, "{p} still holds what an unfinished session wrote"),
            (false, true) => write!(f, "{p} was left behind by an unfinished session"),
            (true, false) => write!(f, "{p} is not the original, and swofty did not write it"),
            (false, false) => write!(
                f,
                "{p} is already in the game folder, and swofty did not put it there"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RolledBack {
    pub id: String,
    pub profile: String,
    pub session: bool,
    pub changes: usize,
    pub drift: Vec<Drift>,
    /// The files addons wrote that went to `addon-storage/` in the data
    /// folder, as game paths.
    pub kept: Vec<String>,
}

pub struct Installer<'a> {
    pub data: &'a Path,
    pub game: &'a Game,
    pub depot: &'a dyn DepotHashes,
    pub stop_after: Option<usize>,
    pub session: bool,
    /// Where a long install says how far it got.
    pub out: &'a dyn Fn(&str),
    /// Files copied so far, of how many, after each one.
    pub progress: &'a dyn Fn(usize, usize),
    pub free_space: fn(&Path) -> Option<u64>,
    /// The volume a folder is on; tells Play whether the build output and the
    /// game folder are on different drives.
    pub volume: fn(&Path) -> Option<crate::platform::disk::Volume>,
    /// Copy every file instead of hard-linking build outputs; set by
    /// `SWOFTY_INSTALL_COPY=1`.
    pub copy_only: bool,
}

impl<'a> Installer<'a> {
    pub fn new(data: &'a Path, game: &'a Game) -> Self {
        Self {
            data,
            game,
            depot: &(),
            stop_after: None,
            session: false,
            out: &quiet,
            progress: &unwatched,
            free_space: crate::platform::disk::free_space,
            volume: crate::platform::disk::volume,
            copy_only: std::env::var_os("SWOFTY_INSTALL_COPY").is_some_and(|v| v == "1"),
        }
    }

    fn objects(&self) -> Objects {
        Objects {
            root: self.data.join("baseline/objects"),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.game.dir.join(rel)
    }

    pub fn baseline(&self) -> Result<Baseline> {
        Baseline::load_or_new(self.data, &self.game.identity)
    }
}

#[cfg(test)]
mod tests;
