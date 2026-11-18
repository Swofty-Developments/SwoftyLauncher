use std::path::PathBuf;
use std::sync::Arc;

use swofty_addon::launcher::Builtin;

use crate::Result;
use crate::addons::helper::Helpers;

mod addon_install;
mod config;
mod migrate;
mod profiles;
mod scan;
mod swap;

pub use config::set_warning_sink;
pub use profiles::ProfileDeletion;
pub use scan::Unreadable;

pub const DEFAULT_PROFILE: &str = "default";
pub const DATA_VERSION: u32 = 4;
const LEGACY_ADDONS_DIR: &str = "mods";

#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    builtins: Arc<[Builtin]>,
    helpers: Arc<Helpers>,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            builtins: Arc::new([]),
            helpers: Arc::default(),
        }
    }

    pub fn with_builtins(mut self, builtins: Vec<Builtin>) -> Self {
        self.builtins = builtins.into();
        self
    }

    pub fn builtins(&self) -> &[Builtin] {
        &self.builtins
    }

    pub fn helpers(&self) -> &Helpers {
        &self.helpers
    }

    pub fn registry(&self) -> Result<crate::addons::registry::Registry> {
        crate::addons::registry::Registry::load(self)
    }

    pub fn open_default() -> Self {
        Self::new(crate::data::paths::data_dir())
    }

    pub fn addons_dir(&self) -> PathBuf {
        self.root.join("addons")
    }

    /// Personal shader configs and the shader settings.
    pub fn shaders_dir(&self) -> PathBuf {
        self.root.join("shaders")
    }

    /// Compiled effects and the extracted effect library.
    pub fn shaders_cache_dir(&self) -> PathBuf {
        self.root.join("cache/shaders")
    }

    pub fn addon_data_dir(&self, id: &str) -> PathBuf {
        self.root.join("addon-data").join(id)
    }

    pub fn addon_cache_dir(&self, id: &str) -> PathBuf {
        self.root.join("cache/addons").join(id)
    }

    /// Where an Addon Browser install records the download it installed `id` from; any other
    /// install or removal of `id` deletes it.
    pub fn registry_install_path(&self, id: &str) -> PathBuf {
        self.root
            .join("state/registry-installs")
            .join(format!("{id}.json"))
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn overlays_dir(&self, profile: &str) -> PathBuf {
        overlay_root(&self.root).join(profile)
    }

    pub fn overlay_dir(&self, profile: &str, buildhash: &str) -> PathBuf {
        self.overlays_dir(profile).join(buildhash)
    }

    pub fn saves_dir(&self, profile: &str) -> PathBuf {
        self.root.join("saves").join(profile)
    }

    pub fn baseline_dir(&self, build: &str) -> PathBuf {
        self.root.join("baseline").join(build)
    }

    pub fn journal_dir(&self) -> PathBuf {
        self.root.join("state/journal")
    }

    pub fn requests_dir(&self) -> PathBuf {
        self.root.join("state/requests")
    }

    pub fn launch_request_path(&self) -> PathBuf {
        self.root.join("state/launch.json")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    // A package is one addon; any other archive holds whatever addons it has.
}

pub fn overlay_root(root: &std::path::Path) -> PathBuf {
    root.join("cache").join("overlay")
}

#[cfg(test)]
mod tests;
