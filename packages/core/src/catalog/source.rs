//! Where an installed addon came from, which decides whether the play bar updates it from the
//! Addon Browser. Only an addon installed from there is updated, so an import or a file that has
//! the id of an Addon Browser addon keeps its own files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::manifest::AddonManifest;
use crate::store::Store;

/// The skipped Addon Browser versions, by addon id.
pub const SKIPPED_FILE: &str = "state/skipped-updates.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Installed from the Addon Browser: [`super::record_install`] recorded its download.
    Registry,
    /// Made from a mod by a tool, such as the Simplified Importer or the DB Editor.
    Made,
    /// Installed from a file or a folder.
    Local,
    /// Installed by a launcher that recorded only Addon Browser installs, or none: an Addon Browser
    /// install from before then, or a file.
    Unrecorded,
}

impl Source {
    /// Whether the play bar offers it the Addon Browser's newer versions. An addon installed before
    /// the launcher recorded where addons came from keeps getting them, as it did.
    pub fn takes_updates(self) -> bool {
        matches!(self, Self::Registry | Self::Unrecorded)
    }
}

/// What the launcher records for an addon installed from anywhere but the Addon Browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalInstall {
    /// The addon whose helper installed it, such as the Simplified Importer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// The id it asked for, when the launcher installed it under another one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<String>,
}

pub fn source(store: &Store, m: &AddonManifest) -> Source {
    if store.registry_install_path(&m.id).is_file() {
        Source::Registry
    } else if m.origin.is_some() {
        Source::Made
    } else if store.local_install_path(&m.id).is_file() {
        Source::Local
    } else {
        Source::Unrecorded
    }
}

pub fn takes_updates(store: &Store, m: &AddonManifest) -> bool {
    source(store, m).takes_updates()
}

pub fn local_install(store: &Store, id: &str) -> Option<LocalInstall> {
    crate::read_json(&store.local_install_path(id)).ok()
}

pub(crate) fn record_local(store: &Store, id: &str, local: &LocalInstall) -> Result<()> {
    crate::write_json_shared(&store.local_install_path(id), local)
}

fn skipped_path(root: &Path) -> PathBuf {
    root.join(SKIPPED_FILE)
}

/// The Addon Browser version of each addon the play bar doesn't offer.
pub fn skipped(root: &Path) -> BTreeMap<String, semver::Version> {
    crate::read_json(&skipped_path(root)).unwrap_or_default()
}

/// Stops the play bar offering each addon the version given with it; a newer one is offered again.
pub fn skip(
    root: &Path,
    versions: impl IntoIterator<Item = (String, semver::Version)>,
) -> Result<BTreeMap<String, semver::Version>> {
    let mut all = skipped(root);
    all.extend(versions);
    crate::write_json_shared(&skipped_path(root), &all)?;
    Ok(all)
}
