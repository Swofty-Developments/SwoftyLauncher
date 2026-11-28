//! The Overcooked 2 addon store: `<data>/games/overcooked_2/addons/<id>/`, the ones on in `enabled.json`, each
//! addon's own data folder, and installs from a file, a folder or the Addon Browser.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::manifest::Manifest;
use crate::catalog::requires::Event;
use crate::catalog::{self, Addon, Stage, Version};
use crate::games::{Installed, Listing, Unreadable};
use crate::manifest::AddonManifest;
use crate::{Error, Result, io_err};

pub const GAME_DIR: &str = "games/overcooked_2";
pub const ADDONS_DIR: &str = "addons";
pub const ENABLED_FILE: &str = "enabled.json";
pub const ADDON_DATA_DIR: &str = "addon-data";
pub const SESSIONS_DIR: &str = "sessions";
pub const CATALOG_FILE: &str = "catalog.json";
pub const DOWNLOADS_DIR: &str = "downloads";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Enabled {
    #[serde(default)]
    enabled: Vec<String>,
}

/// One installed addon with its manifest, as the session and the launcher read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub enabled: bool,
}

/// The Overcooked 2 folder of the data folder `root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(GAME_DIR),
        }
    }

    pub fn addons_dir(&self) -> PathBuf {
        self.dir.join(ADDONS_DIR)
    }

    pub fn addon_data_dir(&self, id: &str) -> PathBuf {
        self.dir.join(ADDON_DATA_DIR).join(id)
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.dir.join(SESSIONS_DIR)
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.dir.join(CATALOG_FILE)
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.dir.join(DOWNLOADS_DIR)
    }

    fn enabled_path(&self) -> PathBuf {
        self.dir.join(ENABLED_FILE)
    }

    /// The ids on, in the order the core loads them.
    pub fn enabled(&self) -> Vec<String> {
        crate::read_json::<Enabled>(&self.enabled_path())
            .map(|e| e.enabled)
            .unwrap_or_default()
    }

    fn save_enabled(&self, ids: Vec<String>) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(io_err(&self.dir))?;
        crate::write_json(&self.enabled_path(), &Enabled { enabled: ids })
    }

    /// Every installed addon, by name, and the folders that can't be read.
    pub fn read(&self) -> (Vec<Stored>, Vec<Unreadable>) {
        let enabled = self.enabled();
        let mut addons = Vec::new();
        let mut unreadable = Vec::new();
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(self.addons_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !hidden(p))
            .collect();
        dirs.sort();
        for dir in dirs {
            let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            match Manifest::load(&dir) {
                Ok(m) if m.id != id => unreadable.push(Unreadable {
                    id,
                    reason: format!("its swofty.json names {}", m.id),
                }),
                Ok(m) => addons.push(Stored {
                    enabled: enabled.contains(&m.id),
                    manifest: m,
                    dir,
                }),
                Err(e) => unreadable.push(Unreadable {
                    id,
                    reason: e.to_string(),
                }),
            }
        }
        addons.sort_by_key(|a| (a.manifest.name.to_lowercase(), a.manifest.id.clone()));
        (addons, unreadable)
    }

    /// The addons on that are installed and whole, in load order.
    pub fn on(&self) -> Vec<Stored> {
        let (addons, _) = self.read();
        self.enabled()
            .iter()
            .filter_map(|id| addons.iter().find(|a| &a.manifest.id == id))
            .filter(|a| a.manifest.missing(&a.dir).is_empty())
            .cloned()
            .collect()
    }

    pub fn listing(&self) -> Listing {
        let (addons, unreadable) = self.read();
        Listing {
            addons: addons.iter().map(installed).collect(),
            unreadable,
        }
    }

    pub fn set_enabled(&self, id: &str, on: bool) -> Result<()> {
        let mut ids = self.enabled();
        if on {
            if !self
                .addons_dir()
                .join(id)
                .join(crate::manifest::MANIFEST_FILE)
                .is_file()
            {
                return Err(Error::Other(format!("no addon named {id} is installed")));
            }
            if !ids.iter().any(|i| i == id) {
                ids.push(id.to_owned());
            }
        } else {
            ids.retain(|i| i != id);
        }
        self.save_enabled(ids)
    }

    /// Deletes the addon and takes it off; its data folder is kept.
    pub fn uninstall(&self, id: &str) -> Result<()> {
        if !super::manifest::valid_id(id) {
            return Err(Error::Other(format!("{id:?} is not an addon id")));
        }
        let dir = self.addons_dir().join(id);
        if !dir.is_dir() {
            return Err(Error::Other(format!("no addon named {id} is installed")));
        }
        std::fs::remove_dir_all(&dir).map_err(io_err(&dir))?;
        let mut ids = self.enabled();
        ids.retain(|i| i != id);
        self.save_enabled(ids)
    }

    /// Every addon the folder, zip or rar at `src` holds, installed and on.
    pub fn install(&self, src: &Path) -> Vec<Result<Stored>> {
        if !src.exists() {
            return vec![Err(Error::Other(format!("{} is not there", src.display())))];
        }
        if src.is_dir() {
            let root = crate::addons::import::addon_root(src).unwrap_or_else(|| src.to_path_buf());
            return vec![self.install_folder(&root, None)];
        }
        let scratch = match self.unpack(src) {
            Ok(s) => s,
            Err(e) => return vec![Err(e)],
        };
        let found = crate::addons::import::found_addons(&scratch.0);
        if found.is_empty() {
            return vec![Err(Error::Other(format!(
                "{} has no {}",
                src.display(),
                crate::manifest::MANIFEST_FILE
            )))];
        }
        found.iter().map(|dir| self.install_folder(dir, None)).collect()
    }

    fn unpack(&self, archive: &Path) -> Result<Scratch> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = self
            .downloads_dir()
            .join(format!(".unpack-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
        let scratch = Scratch(dir);
        crate::addons::import::unpack(archive, &scratch.0)?;
        Ok(scratch)
    }

    /// Installs the addon folder `src`; `listed` is the id and version the catalog says it holds.
    pub fn install_folder(&self, src: &Path, listed: Option<(&str, &semver::Version)>) -> Result<Stored> {
        let m = Manifest::load(src)?;
        if let Some((id, version)) = listed
            && (m.id != id || m.version != *version)
        {
            return Err(Error::Other(format!(
                "the registry lists {id} {version}, the download holds {} {}",
                m.id, m.version
            )));
        }
        let missing = m.missing(src);
        if !missing.is_empty() {
            return Err(Error::Other(format!(
                "{} lists {} but the addon doesn't have it",
                m.name,
                missing.join(", ")
            )));
        }
        let addons = self.addons_dir();
        std::fs::create_dir_all(&addons).map_err(io_err(&addons))?;
        let dest = addons.join(&m.id);
        let staging = addons.join(format!(".staging-{}", m.id));
        std::fs::remove_dir_all(&staging).ok();
        copy_dir(src, &staging).inspect_err(|_| {
            std::fs::remove_dir_all(&staging).ok();
        })?;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(io_err(&dest))?;
        }
        std::fs::rename(&staging, &dest).map_err(io_err(&dest))?;
        self.set_enabled(&m.id, true)?;
        Ok(Stored {
            manifest: m,
            dir: dest,
            enabled: true,
        })
    }

    /// Downloads `v` of `addon`, checks its size and SHA-256 against the catalog and installs it.
    pub fn install_listed(
        &self,
        root: &Path,
        addon: &Addon,
        v: &Version,
        progress: &dyn Fn(Event),
    ) -> Result<Vec<AddonManifest>> {
        progress(Event::Addon(addon, v));
        let downloads = self.downloads_dir();
        std::fs::create_dir_all(&downloads).map_err(io_err(&downloads))?;
        let file = downloads.join(catalog::file_name(addon, v));
        catalog::download_resumable(v, &file, root, &|s| progress(Event::Stage(s)))?;
        progress(Event::Stage(Stage::Installing));
        let installed = self.unpack(&file).and_then(|scratch| {
            let found = crate::addons::import::found_addons(&scratch.0);
            match found.as_slice() {
                [one] => self.install_folder(one, Some((&addon.id, &v.version))),
                _ => Err(Error::Other(format!(
                    "the download of {} holds {} addons, not one",
                    addon.id,
                    found.len()
                ))),
            }
        });
        std::fs::remove_file(&file).ok();
        let listing = installed?.manifest.listing();
        progress(Event::Installed(&listing));
        Ok(vec![listing])
    }
}

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

pub(super) fn installed(a: &Stored) -> Installed {
    Installed {
        manifest: a.manifest.listing(),
        icon: a
            .manifest
            .icon
            .as_ref()
            .map(|i| a.dir.join(i))
            .filter(|p| p.is_file()),
        dir: a.dir.clone(),
        enabled: a.enabled,
    }
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest).map_err(io_err(dest))?;
    for entry in std::fs::read_dir(src).map_err(io_err(src))? {
        let entry = entry.map_err(io_err(src))?;
        let to = dest.join(entry.file_name());
        let kind = entry.file_type().map_err(io_err(entry.path()))?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &to).map_err(io_err(&to))?;
        }
    }
    Ok(())
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}
