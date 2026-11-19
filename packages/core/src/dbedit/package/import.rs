//! Importing a DB Editor addon from a zip, an exported json, or a modded game database, and installing one
//! from the Addons page.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::game_edits::database_edits;
use super::naming::{slug, taken, unique_name};
use super::{FORMAT, GVDB_NAME, ID_PREFIX, Imported, MARKER, MAX_GVDB, Package, PackageFile};
use crate::dbedit::{Catalog, Edits, ImportReport};
use crate::manifest::{Category, MANIFEST_FILE};
use crate::store::Store;
use crate::{Error, Result, io_err};

pub(super) fn read_file(path: &Path) -> Result<PackageFile> {
    let file: PackageFile = crate::read_json(path)?;
    if file.format != FORMAT {
        return Err(Error::Other(format!(
            "{}: format {:?} is not {FORMAT}",
            path.display(),
            file.format
        )));
    }
    Ok(file)
}

pub(super) fn scratch_dir(store: &Store) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    store
        .root
        .join("cache")
        .join(format!("dbedit-import-{}-{n}", std::process::id()))
}

pub(super) fn file_label(file: &Path) -> String {
    file.file_name().map_or_else(
        || file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

pub(super) fn file_stem(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Imported".into())
}

/// What [`read`] found in a file.
#[derive(Debug)]
pub enum Incoming {
    /// A DB Editor addon `.zip`, installed as a new addon.
    Addon(Imported),
    /// Changes without addon details, named after the file.
    Changes {
        name: String,
        edits: Edits,
        report: ImportReport,
    },
}

/// `.zip` with `dbedit.json` installs as a new addon (a taken id gets a new one);
/// a `.json` DB Editor or EGO export, a modded `gamedatabases.2025.bin`, or a
/// `.zip` holding one and no `swofty.json` becomes a new addon named after the file.
pub fn import(store: &Store, catalog: &Catalog, file: &Path) -> Result<Imported> {
    match read(store, catalog, file)? {
        Incoming::Addon(done) => Ok(done),
        Incoming::Changes { name, edits, report } => {
            let mut package = Package::create(store, &unique_name(store, &name, false)?)?;
            package.edits = edits;
            package.save(store, catalog)?;
            Ok(Imported {
                report,
                package: package.info(),
                renamed_from: None,
            })
        }
    }
}

/// Like [`import`], but returns the changes of a `.json`, `.bin` or database
/// `.zip` instead of making an addon of them.
pub fn read(store: &Store, catalog: &Catalog, file: &Path) -> Result<Incoming> {
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" => import_any_zip(store, catalog, file),
        "json" => import_json(catalog, file),
        "bin" => {
            let bytes = std::fs::read(file).map_err(io_err(file))?;
            import_bin(catalog, &bytes, &file_label(file), &file_stem(file))
        }
        _ => Err(Error::Other(format!(
            "{} is not a DB Editor addon .zip, a .json export or a .bin game database",
            file_label(file)
        ))),
    }
}

pub(super) fn import_any_zip(store: &Store, catalog: &Catalog, file: &Path) -> Result<Incoming> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(file).map_err(io_err(file))?)?;
    let leaf = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_owned();
    let names = z
        .file_names()
        .map(|n| n.map(|n| n.into_owned()))
        .collect::<std::result::Result<Vec<String>, _>>()?;
    if names.iter().any(|n| leaf(n) == MANIFEST_FILE) {
        let scratch = scratch_dir(store);
        let result = import_zip(store, file, &scratch);
        std::fs::remove_dir_all(&scratch).ok();
        return result.map(Incoming::Addon);
    }
    let bins: Vec<&String> = names
        .iter()
        .filter(|n| leaf(n).eq_ignore_ascii_case(GVDB_NAME))
        .collect();
    let label = file_label(file);
    let name = match bins.as_slice() {
        [one] => one.as_str(),
        [] => {
            return Err(Error::Other(format!(
                "{label} is not a DB Editor addon and holds no {GVDB_NAME}"
            )));
        }
        many => {
            return Err(Error::Other(format!(
                "{label} holds {} {GVDB_NAME} files; extract the one you want and import it",
                many.len()
            )));
        }
    };
    let entry = z.by_name(name)?;
    if entry.size() > MAX_GVDB {
        return Err(Error::Other(format!("{label}: {name} is too large")));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(MAX_GVDB)
        .read_to_end(&mut bytes)
        .map_err(io_err(file))?;
    import_bin(catalog, &bytes, &label, &file_stem(file))
}

pub(super) fn import_zip(store: &Store, file: &Path, scratch: &Path) -> Result<Imported> {
    crate::content::track_package::unzip(file, scratch, Error::Other)?;
    let root = crate::addons::import::addon_root(scratch).unwrap_or_else(|| scratch.to_path_buf());
    if !root.join(MARKER).is_file() || !root.join(MANIFEST_FILE).is_file() {
        return Err(Error::Other(format!(
            "{} is not a DB Editor addon; install it on the Addons page",
            file_label(file)
        )));
    }
    let (package, renamed_from) = install(store, &root)?;
    Ok(Imported {
        report: ImportReport {
            format: "package",
            imported: package.edits.len(),
            ..ImportReport::default()
        },
        package: package.info(),
        renamed_from,
    })
}

#[derive(Deserialize)]
pub(super) struct Details {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) version: String,
    #[serde(default)]
    pub(super) author: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) category: Option<serde_json::Value>,
}

/// Installs the addon in `root` as a new DB Editor addon: its name, version,
/// author, description and category from `swofty.json`, its edits from
/// `dbedit.json`, and nothing else. A taken id gets a new id and a numbered
/// name, returning the id it asked for.
pub fn install(store: &Store, root: &Path) -> Result<(Package, Option<String>)> {
    let source = read_file(&root.join(MARKER))?;
    let d: Details = crate::read_json(&root.join(MANIFEST_FILE))?;
    let version = semver::Version::parse(d.version.trim())
        .map_err(|_| Error::Other(format!("{:?} is not a version like 1.0.0", d.version)))?;
    let base = d.name.trim();
    if base.is_empty() {
        return Err(Error::Other("an addon needs a name".into()));
    }
    let wanted = format!("{ID_PREFIX}{}", slug(base));
    let renamed_from = taken(store, &wanted)?.then_some(wanted);
    let name = unique_name(store, base, renamed_from.is_some())?;
    let mut package = Package::create(store, &name)?;
    let filled = package
        .set_details(
            &name,
            &version.to_string(),
            &d.author,
            d.description.as_deref().unwrap_or_default(),
        )
        .and_then(|()| {
            package.set_category(d.category.and_then(|v| Category::read(v).ok().flatten()));
            package.edits = Edits {
                schema_version: 1,
                edits: source.edits,
                rows: source.rows,
            };
            package.game_build = source.game_build;
            package.write(store)
        });
    if let Err(e) = filled {
        Package::delete(store, &package.id).ok();
        return Err(e);
    }
    Ok((package, renamed_from))
}

pub(super) fn import_json(catalog: &Catalog, file: &Path) -> Result<Incoming> {
    let bytes = std::fs::read(file).map_err(io_err(file))?;
    let mut edits = Edits {
        schema_version: 1,
        ..Edits::default()
    };
    let report = edits
        .import(catalog, &bytes, &file_label(file))
        .map_err(Error::Other)?;
    Ok(Incoming::Changes {
        name: file_stem(file),
        edits,
        report,
    })
}

pub(super) fn import_bin(catalog: &Catalog, bytes: &[u8], label: &str, name: &str) -> Result<Incoming> {
    let (edits, notes) = database_edits(catalog, bytes, label)?;
    Ok(Incoming::Changes {
        name: name.to_owned(),
        report: ImportReport {
            format: "bin",
            imported: edits.len(),
            unchanged: 0,
            skipped: notes,
        },
        edits,
    })
}
