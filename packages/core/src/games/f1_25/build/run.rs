use std::collections::BTreeMap;
use std::path::PathBuf;

use swofty_formats::erp::Packer;

use crate::Result;
use crate::addons::settings::OnAddons;
use crate::asset::{Origin, Root};
use crate::install::Game;
use crate::manifest::AddonManifest;
use crate::planner::Planner;
use crate::profile::Profile;
use crate::store::Store;

/// What one build of a profile reads, the same for every stage of it.
#[derive(Clone, Copy)]
pub(super) struct Run<'a> {
    pub store: &'a Store,
    pub game: &'a Game,
    pub profile: &'a str,
    /// A check plans the build without keeping anything a later build depends on.
    pub checking: bool,
    pub addons: &'a BTreeMap<String, AddonManifest>,
    /// The addons on, in load order.
    pub order: &'a [String],
    pub settings: &'a Profile,
    pub on: &'a OnAddons,
    /// The build's packer: it reuses the frames earlier builds compressed into the store's cache, and leaves
    /// the archive build's compression to each archive's write.
    pub packer: &'a Packer,
}

impl<'a> Run<'a> {
    /// The build's packer compressing as it goes, for the archives written straight to disk instead of
    /// through the archive build.
    pub fn eager_packer(&self) -> Packer {
        self.packer.clone().deferring(false)
    }

    /// Where `origin`'s addon comes in the load order; an addon outside it comes last.
    pub fn rank(&self, origin: &Origin) -> usize {
        self.order
            .iter()
            .position(|id| *id == origin.addon)
            .unwrap_or(usize::MAX)
    }

    /// The addon's display name, or its id when it is not installed.
    pub fn name(&self, id: &str) -> String {
        self.addons
            .get(id)
            .map_or_else(|| id.to_owned(), |m| m.name.clone())
    }

    /// The display name of an origin written as `id` or `id (item)`.
    pub fn origin_name(&self, origin: &str) -> String {
        match origin.split_once(" (") {
            Some((id, item)) => match self.addons.get(id) {
                Some(m) => format!("{} ({item}", m.name),
                None => origin.to_owned(),
            },
            None => self.name(origin),
        }
    }

    /// The addon whose database ops last wrote record `key`, by name, or the database itself.
    pub fn writer_name(&self, planner: &Planner, key: u64) -> String {
        planner
            .record_writer(key)
            .map_or_else(|| "database".to_owned(), |id| self.name(id))
    }

    pub fn manifests(&self) -> Vec<&'a AddonManifest> {
        self.order.iter().map(|id| &self.addons[id]).collect()
    }

    pub fn original(&self, rel: &str) -> PathBuf {
        super::original(&self.store.root, self.game, rel)
    }

    pub fn addon_dir(&self, id: &str) -> PathBuf {
        self.store.addons_dir().join(id)
    }

    pub fn source_dir(&self, addon: &str, root: Root) -> PathBuf {
        super::source_dir(self.store, addon, root)
    }

    /// Stops the build when the player cancelled it; a check never stops.
    pub fn cancel(&self) -> Result<()> {
        match self.checking {
            true => Ok(()),
            false => crate::game_folder::busy::check(&self.store.root),
        }
    }
}
