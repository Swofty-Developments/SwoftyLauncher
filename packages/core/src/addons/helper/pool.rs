//! The helper addons the launcher has started, one per addon.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use swofty_addon::manifest::AddonManifest;

use super::HelperAddon;

#[derive(Default)]
pub struct Helpers {
    all: Mutex<Vec<Arc<HelperAddon>>>,
    paused: Arc<AtomicUsize>,
}

/// Keeps every helper program stopped, and unstartable, until it drops.
#[must_use = "helpers start again when this drops"]
pub struct Pause(Arc<AtomicUsize>);

impl Drop for Pause {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for Helpers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Helpers")
    }
}

impl Helpers {
    pub fn get(&self, manifest: &AddonManifest, dir: &Path) -> Option<Arc<HelperAddon>> {
        let mut all = self.all.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = all.iter().find(|h| h.manifest == *manifest && h.dir == dir) {
            return Some(h.clone());
        }
        let mut helper = HelperAddon::new(manifest.clone(), dir)?;
        helper.paused = self.paused.clone();
        let helper = Arc::new(helper);
        all.retain(|h| h.manifest.id != manifest.id);
        all.push(helper.clone());
        Some(helper)
    }

    /// Stops every helper program and keeps them from starting until the
    /// returned guard drops: a helper holds folders the restore must remove.
    pub fn pause(&self) -> Pause {
        self.paused.fetch_add(1, Ordering::SeqCst);
        let pause = Pause(self.paused.clone());
        let all: Vec<_> = self.all.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for h in all {
            h.stop();
        }
        pause
    }

    pub fn forget(&self, id: &str) {
        let mut all = self.all.lock().unwrap_or_else(|e| e.into_inner());
        for h in all.iter().filter(|h| h.manifest.id == id) {
            h.stop();
        }
        all.retain(|h| h.manifest.id != id);
    }
}
