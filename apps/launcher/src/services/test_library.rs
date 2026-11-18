//! A data folder in the temp directory with addons and an active profile, read into a [`Library`].

use std::path::PathBuf;

use swofty_core::manifest::MANIFEST_FILE;
use swofty_core::profile::{Profile, ProfileAddon};
use swofty_core::store::Store;

use super::library::Library;

pub(crate) struct TestLibrary {
    pub(crate) store: Store,
    root: PathBuf,
}

impl TestLibrary {
    pub(crate) fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-launcher-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        Self {
            store: Store::new(&root),
            root,
        }
    }

    pub(crate) fn addon(self, id: &str, version: &str) -> Self {
        let dir = self.store.addons_dir().join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"{version}","author":"t","compat":{{"game":"f1_25"}}}}"#
        );
        std::fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
        self
    }

    pub(crate) fn profile(self, name: &str, on: &[&str]) -> Self {
        let mut profile = Profile::new(name);
        profile.addons = on
            .iter()
            .map(|id| ProfileAddon {
                id: (*id).to_owned(),
                enabled: true,
                unknown: Default::default(),
            })
            .collect();
        self.store.save_profile(&profile).unwrap();
        self.store.set_active_profile(name).unwrap();
        self
    }

    pub(crate) fn read(&self) -> Library {
        Library::read(&self.store)
    }
}

impl Drop for TestLibrary {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}
