use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::buildhash::BuildHash;
use super::loader::loader_line;
use super::run::Run;
use super::{Built, source_dir};
use crate::Result;
use crate::addons::builtin::{CORE_ID, loader_dir};
use crate::asset::Features;
use crate::asset::{Origin, Source};
use crate::planner::{MipmapNames, OverlayNames, Planner, load_features};
use crate::session::Config;
use crate::store::Store;

/// What a build reads before any addon's ops: the in-game loader and its features, the launcher's
/// asset routes and the DB Editor packages on. The planner borrows it for the rest of the build.
pub(super) struct Setup<'a> {
    pub loader: Option<PathBuf>,
    /// The folder of the running launcher, which a missing loader is named by.
    pub exe_dir: Option<PathBuf>,
    pub features: Features,
    pub config: Config,
    /// The DB Editor packages on, by id, with their names.
    pub packages: BTreeMap<String, String>,
    /// The folder of compressed archive data the formats crate reuses between builds.
    pub packed: PathBuf,
    overlay_names: Box<OverlayNames<'a>>,
    mipmap_names: Box<MipmapNames<'a>>,
}

/// The folder of compressed frames the build's packer reuses between builds.
pub(super) fn packed_dir(store: &Store) -> PathBuf {
    store.root.join("cache").join(crate::cache::PACKED_DIR)
}

/// The features the in-game loader the build uses turns on, none without one.
pub(super) fn loader_features() -> Result<Features> {
    features_of(loader_dir().as_deref())
}

fn features_of(loader: Option<&Path>) -> Result<Features> {
    Ok(loader.map(load_features).transpose()?.unwrap_or_default())
}

impl<'a> Setup<'a> {
    pub fn read(run: Run<'a>, built: &mut Built) -> Result<Self> {
        let store = run.store;
        let packed = packed_dir(store);
        let loader = loader_dir();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf));
        built
            .lines
            .push(loader_line(exe_dir.as_deref(), loader.as_deref()));
        let features = features_of(loader.as_deref())?;
        let packages = run
            .order
            .iter()
            .filter(|id| crate::dbedit::package::is_package(store, id))
            .map(|id| (id.clone(), run.addons[id].name.clone()))
            .collect();
        Ok(Self {
            loader,
            exe_dir,
            features,
            config: store.config(),
            packages,
            packed,
            overlay_names: Box::new(move |origin: &Origin, src: &Source| -> Vec<String> {
                src.resolve(&source_dir(store, &origin.addon, src.root))
                    .ok()
                    .and_then(|p| swofty_formats::erp::ErpFile::open(&p).ok())
                    .map(|f| f.erp.resources.into_iter().map(|r| r.name).collect())
                    .unwrap_or_default()
            }),
            mipmap_names: Box::new(move |origin: &Origin, src: &Source| -> Vec<String> {
                src.resolve(&source_dir(store, &origin.addon, src.root))
                    .map(|p| crate::erp::mipmap_names(&p))
                    .unwrap_or_default()
            }),
        })
    }

    /// The planner of this build: DB Editor packages and the core apply leniently.
    pub fn planner(&self, run: Run) -> Planner<'_> {
        let mut lenient = self.packages.clone();
        if let Some(core) = run.addons.get(CORE_ID) {
            lenient.insert(CORE_ID.to_owned(), core.name.clone());
        }
        let names = run
            .order
            .iter()
            .map(|id| (id.clone(), run.addons[id].name.clone()))
            .collect();
        Planner::new(&self.features, &self.config.runtime_assets)
            .build_only(&self.config.build_assets)
            .lenient(lenient)
            .names(names)
            .overlays(&*self.overlay_names)
            .mipmaps(&*self.mipmap_names)
    }

    pub fn hash(&self, hash: &mut BuildHash) {
        if self.features != Features::default() {
            hash.update(format!(
                "runtime {:?} {:?};",
                self.features, self.config.runtime_assets
            ));
            if !self.config.build_assets.is_empty() {
                hash.update(format!("build {:?};", self.config.build_assets));
            }
        }
    }
}
