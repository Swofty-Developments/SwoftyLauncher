//! DB Editor addons: named sets of edits installed as data addons
//! (`<data>/addons/<id>/` with `swofty.json`, `database.json` and `dbedit.json`).
//! The module keeps its old name, `package`.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Catalog, Edit, Edits, ImportReport, RowCopy};
use crate::manifest::{AddonManifest, Category, MANIFEST_FILE, valid_id};
use crate::store::Store;
use crate::{Error, Result, io_err};

mod addon_manifest;
mod export;
mod game_edits;
mod import;
mod naming;

use addon_manifest::{launcher_floor, manifest, origin, raise_launcher};
pub use export::{export_game_files, export_packages};
pub use game_edits::{enabled_changes, enabled_packages, turn_off_all};
use import::read_file;
pub use import::{Incoming, import, install, read};
pub use naming::modified;
use naming::unique_id;

/// The DB Editor's source file; its presence marks a DB Editor addon.
pub const MARKER: &str = "dbedit.json";
/// The database operation file generated from the edits.
pub const OPS_FILE: &str = "database.json";
pub const FORMAT: &str = "swofty-db-package";
pub const ID_PREFIX: &str = "dbedit.";
pub const DEFAULT_AUTHOR: &str = "Unknown";
/// `origin.tool` of every DB Editor addon.
pub const ORIGIN_TOOL: &str = "db-editor";
const GAME_BUILDS: &str = ">=25251167";
/// The first launcher that reads copied rows (`rows` in `dbedit.json`, format version 2).
const ROWS_LAUNCHER: semver::Version = semver::Version::new(1, 3, 4);
/// The game database's file name, which a modded-database import looks for.
pub const GVDB_NAME: &str = "gamedatabases.2025.bin";
pub const README: &str = "README.txt";
const MAX_GVDB: u64 = 1 << 30;
const MAX_TRIES: u32 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub changes: usize,
    /// The catalog build the edits were made against.
    pub game_build: String,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub id: String,
    pub manifest: AddonManifest,
    pub edits: Edits,
    pub game_build: String,
}

#[derive(Debug, Clone)]
pub struct Imported {
    pub package: PackageInfo,
    pub report: ImportReport,
    /// The id the file asked for when it was taken and a new one was given.
    pub renamed_from: Option<String>,
}

/// What [`export_game_files`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameExport {
    pub changes: usize,
    /// Changes that no longer apply, each naming its addon.
    pub skipped: Vec<String>,
    /// Values the database has no rows for, such as Driver Manager drivers, each naming its addon.
    pub warnings: Vec<String>,
    /// Size of the written zip.
    pub bytes: u64,
}

/// `dbedit.json`. Format version 2 adds `rows`; a file without copied rows is written as version 1.
#[derive(Serialize, Deserialize)]
struct PackageFile {
    format: String,
    format_version: u32,
    #[serde(default)]
    game_build: String,
    edits: Vec<Edit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<RowCopy>,
}

fn dir(store: &Store, id: &str) -> PathBuf {
    store.addons_dir().join(id)
}

fn not_a_package(id: &str) -> Error {
    Error::Other(format!("{id} is not a DB Editor addon"))
}

pub fn is_package(store: &Store, id: &str) -> bool {
    id.starts_with(ID_PREFIX) && valid_id(id) && dir(store, id).join(MARKER).is_file()
}

/// Every installed DB Editor addon that reads, sorted by name; see [`scan`]
/// for the ones that don't.
pub fn list(store: &Store) -> Result<Vec<PackageInfo>> {
    Ok(scan(store)?.packages)
}

#[derive(Debug, Default)]
pub struct Scan {
    /// The addons that read, sorted by name.
    pub packages: Vec<PackageInfo>,
    /// (id, error) for each addon whose files are damaged.
    pub damaged: Vec<(String, String)>,
}

pub fn scan(store: &Store) -> Result<Scan> {
    let rd = match std::fs::read_dir(store.addons_dir()) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(io_err(store.addons_dir())(e)),
    };
    let (mut out, mut damaged) = (Vec::new(), Vec::new());
    for entry in rd.flatten() {
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if is_package(store, &id) {
            match Package::open(store, &id) {
                Ok(p) => out.push(p.info()),
                Err(e) => damaged.push((id, e.to_string())),
            }
        }
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    damaged.sort();
    Ok(Scan {
        packages: out,
        damaged,
    })
}

impl Package {
    pub fn create(store: &Store, name: &str) -> Result<Package> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Other("an addon needs a name".into()));
        }
        let id = unique_id(store, name)?;
        let mut package = Package {
            manifest: manifest(&id, name)?,
            id,
            edits: Edits {
                schema_version: 1,
                ..Edits::default()
            },
            game_build: String::new(),
        };
        package.write(store)?;
        Ok(package)
    }

    /// Edits that are not installed, for [`export_packages`].
    pub fn unsaved(name: &str, edits: Edits) -> Result<Package> {
        Ok(Package {
            id: format!("{ID_PREFIX}draft"),
            manifest: manifest(&format!("{ID_PREFIX}draft"), name)?,
            edits,
            game_build: String::new(),
        })
    }

    pub fn open(store: &Store, id: &str) -> Result<Package> {
        if !is_package(store, id) {
            return Err(not_a_package(id));
        }
        let dir = dir(store, id);
        let file = read_file(&dir.join(MARKER))?;
        let manifest = AddonManifest::load(&dir)?;
        Ok(Package {
            id: id.to_owned(),
            manifest,
            edits: Edits {
                schema_version: 1,
                edits: file.edits,
                rows: file.rows,
            },
            game_build: file.game_build,
        })
    }

    pub fn info(&self) -> PackageInfo {
        PackageInfo {
            id: self.id.clone(),
            name: self.manifest.name.clone(),
            version: self.manifest.version.to_string(),
            author: self.manifest.author.clone(),
            description: self.manifest.description.clone().unwrap_or_default(),
            changes: self.edits.len(),
            game_build: self.game_build.clone(),
        }
    }

    /// Writes `dbedit.json`, `database.json` and `swofty.json`.
    pub fn save(&mut self, store: &Store, catalog: &Catalog) -> Result<()> {
        self.game_build = catalog.build_id().to_owned();
        self.write(store)
    }

    /// Writes the three files as they are, keeping `game_build`; an addon
    /// from before 1.3.0 gains its origin.
    pub fn write(&mut self, store: &Store) -> Result<()> {
        let m = &mut self.manifest;
        if m.origin.is_none() {
            m.origin = Some(origin());
            let floor = launcher_floor(&crate::updates::release::current_version());
            m.compat.launcher = semver::VersionReq::parse(&format!(">={floor}")).ok();
        }
        if !self.edits.rows.is_empty() {
            raise_launcher(
                &mut m.compat.launcher,
                &crate::updates::release::current_version(),
            );
        }
        let dir = dir(store, &self.id);
        crate::write_json_shared(&dir.join(OPS_FILE), &self.edits.to_ops())?;
        crate::write_json_shared(&dir.join(MANIFEST_FILE), &self.manifest)?;
        crate::write_json_shared(
            &dir.join(MARKER),
            &PackageFile {
                format: FORMAT.into(),
                format_version: if self.edits.rows.is_empty() { 1 } else { 2 },
                game_build: self.game_build.clone(),
                edits: self.edits.edits.clone(),
                rows: self.edits.rows.clone(),
            },
        )
    }

    pub fn set_details(&mut self, name: &str, version: &str, author: &str, description: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Other("an addon needs a name".into()));
        }
        let version = semver::Version::parse(version.trim())
            .map_err(|_| Error::Other(format!("{version:?} is not a version like 1.0.0")))?;
        let (author, description) = (author.trim(), description.trim());
        self.manifest.name = name.into();
        self.manifest.version = version;
        self.manifest.author = match author {
            "" => DEFAULT_AUTHOR.into(),
            a => a.into(),
        };
        self.manifest.description = (!description.is_empty()).then(|| description.into());
        Ok(())
    }

    pub fn set_category(&mut self, category: Option<Category>) {
        self.manifest.category = category;
    }

    /// Removes every change and returns how many; [`Package::save`] writes it.
    pub fn clear_edits(&mut self) -> usize {
        let n = self.edits.len();
        self.edits.clear();
        n
    }

    pub fn duplicate(store: &Store, id: &str, name: &str) -> Result<Package> {
        let from = Package::open(store, id)?;
        let mut copy = Package::create(store, name)?;
        let m = &from.manifest;
        let description = m.description.clone().unwrap_or_default();
        copy.set_details(name, &m.version.to_string(), &m.author, &description)?;
        copy.edits = from.edits;
        copy.game_build = from.game_build;
        copy.write(store)?;
        Ok(copy)
    }

    pub fn delete(store: &Store, id: &str) -> Result<()> {
        if !is_package(store, id) {
            return Err(not_a_package(id));
        }
        store.remove_addon(id)
    }

    pub fn export_zip(store: &Store, id: &str, out: &Path) -> Result<()> {
        if !is_package(store, id) {
            return Err(not_a_package(id));
        }
        let dir = dir(store, id);
        let file = std::fs::File::create(out).map_err(io_err(out))?;
        let mut z = zip::ZipWriter::new(file);
        let opts =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for name in [MANIFEST_FILE, OPS_FILE, MARKER] {
            let path = dir.join(name);
            let data = std::fs::read(&path).map_err(io_err(&path))?;
            z.start_file(name, opts)?;
            z.write_all(&data).map_err(io_err(out))?;
        }
        z.finish()?.sync_all().map_err(io_err(out))
    }
}

#[cfg(test)]
mod tests;
