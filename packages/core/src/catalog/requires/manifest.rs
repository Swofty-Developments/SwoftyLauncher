//! The swofty.json inside a downloaded addon zip, and the requirements it lists.

use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use super::MANIFEST_LIMIT;
use crate::catalog::{Addon, Version};
use crate::manifest::{MANIFEST_FILE, Requirement};
use crate::{Error, Result};

#[derive(Deserialize)]
pub(super) struct Head {
    pub(super) id: String,
    pub(super) version: semver::Version,
    #[serde(default)]
    pub(super) requires: Vec<Requirement>,
}

/// An archive entry's path as a filesystem resolves it, on any platform: `.` and
/// `..` applied, either slash, no case, and without what Windows ignores, an NTFS
/// stream or a trailing dot or space.
pub(super) fn entry_key(name: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in name.split(['/', '\\']) {
        if part == ".." {
            parts.pop();
            continue;
        }
        let part = part.split(':').next().unwrap_or_default();
        let part = part.trim_end_matches(['.', ' ']);
        if !part.is_empty() {
            parts.push(part.to_uppercase());
        }
    }
    parts.join("/")
}

pub(super) fn zip_manifest(path: &Path) -> Result<Head> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).map_err(crate::io_err(path))?)?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok())
        .map(|n| n.into_owned())
        .collect();
    let nested: Vec<&String> = names
        .iter()
        .filter(|n| {
            n.strip_suffix(MANIFEST_FILE)
                .and_then(|dir| dir.strip_suffix('/'))
                .is_some_and(|dir| !dir.is_empty() && !dir.contains('/'))
        })
        .collect();
    let name = match (names.iter().find(|n| *n == MANIFEST_FILE), nested.as_slice()) {
        (Some(root), _) => root.clone(),
        (None, [one]) => (*one).clone(),
        _ => {
            return Err(Error::Other(format!(
                "{} has no {MANIFEST_FILE} at its root",
                path.display()
            )));
        }
    };
    // Unpacking resolves these spellings to one file, and the last one wins, so
    // the manifest read here must be the only one that can end up installed.
    let (chosen, root) = (entry_key(&name), entry_key(MANIFEST_FILE));
    let manifests = names
        .iter()
        .map(|n| entry_key(n))
        .filter(|key| *key == chosen || *key == root)
        .count();
    if manifests > 1 {
        return Err(Error::Other(format!(
            "{} has more than one {MANIFEST_FILE}",
            path.display()
        )));
    }
    let mut text = String::new();
    zip.by_name(&name)?
        .take(MANIFEST_LIMIT)
        .read_to_string(&mut text)
        .map_err(crate::io_err(path))?;
    serde_json::from_str(&text).map_err(|e| Error::Manifest(format!("{name}: {e}")))
}

pub(super) fn requires_in(path: &Path, addon: &Addon, v: &Version) -> Result<Vec<Requirement>> {
    let head = zip_manifest(path)?;
    if head.id != addon.id || head.version != v.version {
        return Err(Error::Other(format!(
            "the registry lists {} {}, the download holds {} {}",
            addon.id, v.version, head.id, head.version
        )));
    }
    Ok(head.requires)
}
