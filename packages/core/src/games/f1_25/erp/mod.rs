use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use swofty_formats::erp::{ErpFile, Packer, Resource};
use swofty_formats::pkg::{Node, Pkg};

use crate::manifest::relative_inside;
use crate::{Error, Result};

mod apply;
mod archive;
mod checks;
mod index;
mod ops;
pub(crate) mod pack;
mod place;
mod slices;
mod stats;
mod templates;
mod ui_merge;
mod view;
mod write;

#[cfg(test)]
mod tests;

pub use self::apply::Held;
use self::apply::SourceKey;
pub(crate) use self::archive::{Archive, Slot};
pub use self::checks::texture_findings_in;
pub use self::index::{
    IndexName, erp_files, group_dir, index_holds, index_of, mipmap_names, mipmap_path, overlay_names,
    served_at_runtime,
};
pub use self::ops::{addon_of, contested_ui, reads, unsupported, writes};
pub use self::slices::SliceMerge;
pub(crate) use self::slices::view_mismatch;
pub use self::stats::{
    ArchiveRepair, StatsRepair, repair_stats_in, repaired_note, stale_stats_in, unrepairable_message,
};
pub use self::ui_merge::{UiConflict, UiMerge};
pub use self::write::{BuiltIn, Resized};

const SCENE: &str = "UILayout";
const ASSET_GROUPS: &str = "2025_asset_groups";

pub struct ErpBuild<'a> {
    original: &'a dyn Fn(&str) -> PathBuf,
    erps: BTreeMap<String, Archive>,
    // The origin of the op that last wrote each resource, by archive and name,
    // and the origins in the order their ops came.
    writers: HashMap<(String, String), String>,
    origins: Vec<String>,
    appended: HashMap<(String, String), String>,
    sources: HashMap<SourceKey, Node>,
    archives: HashMap<PathBuf, ErpFile>,
    mipmaps: BTreeMap<String, PathBuf>,
    stock: HashMap<String, Option<Resource>>,
    merges: Vec<SliceMerge>,
    notes: Vec<String>,
    kept: HashSet<(String, String)>,
    held: Vec<Held>,
    baseline: Option<&'a dyn Fn(&str) -> PathBuf>,
    merge_ui: bool,
    whole: HashMap<(String, String), Vec<(String, Pkg)>>,
    ui_base: HashMap<(String, String), Option<Pkg>>,
    ui_merges: Vec<UiMerge>,
    added: HashMap<(String, String, String, String), String>,
    packer: Packer,
}

fn fail(msg: impl Into<String>) -> Error {
    Error::Resolve(msg.into())
}

fn checked(rel: &str) -> Result<&str> {
    if relative_inside(rel) {
        Ok(rel)
    } else {
        Err(fail(format!(
            "{rel} must be a relative path inside the game folder"
        )))
    }
}

impl<'a> ErpBuild<'a> {
    pub fn new(original: &'a dyn Fn(&str) -> PathBuf) -> Self {
        Self {
            original,
            erps: BTreeMap::new(),
            writers: HashMap::new(),
            origins: Vec::new(),
            appended: HashMap::new(),
            sources: HashMap::new(),
            archives: HashMap::new(),
            mipmaps: BTreeMap::new(),
            stock: HashMap::new(),
            merges: Vec::new(),
            notes: Vec::new(),
            kept: HashSet::new(),
            held: Vec::new(),
            baseline: None,
            merge_ui: true,
            whole: HashMap::new(),
            ui_base: HashMap::new(),
            ui_merges: Vec::new(),
            added: HashMap::new(),
            packer: Packer::default(),
        }
    }

    /// The packer every fragment the build changes is compressed by; a deferring one leaves them for the
    /// archive's write.
    pub fn with_packer(mut self, packer: Packer) -> Self {
        self.packer = packer;
        self
    }

    /// The game's own files, which two addons' whole replacements of a UI resource are merged against.
    /// Without it the later replacement wins as a whole.
    pub fn with_baseline(mut self, baseline: &'a dyn Fn(&str) -> PathBuf) -> Self {
        self.baseline = Some(baseline);
        self
    }

    /// Whether whole replacements of a UI resource by different addons are merged (on by default).
    pub fn set_merge_ui(&mut self, on: bool) {
        self.merge_ui = on;
    }

    pub fn take_ui_merges(&mut self) -> Vec<UiMerge> {
        std::mem::take(&mut self.ui_merges)
    }

    pub fn keep(&mut self, erp: &str, resource: &str) {
        self.kept.insert((erp.to_owned(), resource.to_owned()));
    }

    pub fn keeps(&self, erp: &str, resource: &str) -> bool {
        !self.kept.is_empty() && self.kept.contains(&(erp.to_owned(), resource.to_owned()))
    }

    pub fn take_held(&mut self) -> Vec<Held> {
        std::mem::take(&mut self.held)
    }

    fn erp(&mut self, rel: &str) -> Result<&mut Archive> {
        if !self.erps.contains_key(rel) {
            let path = (self.original)(checked(rel)?);
            self.erps.insert(rel.to_owned(), Archive::open(rel, &path)?);
        }
        Ok(self.erps.get_mut(rel).expect("inserted above"))
    }

    pub fn load(&mut self, rel: &str) -> Result<()> {
        self.erp(rel).map(|_| ())
    }

    pub fn origins(&self) -> &[String] {
        &self.origins
    }

    pub fn take_merges(&mut self) -> Vec<SliceMerge> {
        std::mem::take(&mut self.merges)
    }

    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub fn changed(&self) -> impl Iterator<Item = &str> {
        self.erps
            .iter()
            .filter(|(_, a)| !a.untouched())
            .map(|(rel, _)| rel.as_str())
    }

    pub fn take_mipmaps(&mut self) -> BTreeMap<String, PathBuf> {
        std::mem::take(&mut self.mipmaps)
    }
}
