//! The addons in the addons folder, an addon this build cannot read set aside and named, and the built-in
//! addons taken in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::Store;
use super::config::{warn, warn_once};
use crate::addons::builtin::{CORE_ID, core_manifest};
use crate::manifest::{AddonManifest, MANIFEST_FILE};
use crate::profile::Profile;
use crate::{Error, Result, io_err};

/// A saved profile. One that is damaged (not JSON at all) was set aside by
/// [`crate::load_json`] and starts over empty, as a missing one does, so it
/// cannot block a launch; the set-aside copy is kept for the player.
/// An installed addon whose `swofty.json` this launcher can't read.
#[derive(Debug, Clone, PartialEq)]
pub struct Unreadable {
    /// The manifest's id when it has one, else the folder name.
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub dir: PathBuf,
    /// Why, after "it": "needs a newer Swofty Launcher (unknown field `widgets`)".
    pub reason: String,
    /// Made for a newer launcher rather than damaged.
    pub newer: bool,
}

impl Unreadable {
    pub fn of(dir: &Path, err: &swofty_addon::Error, launcher: &semver::Version) -> Self {
        let doc: Option<serde_json::Value> = std::fs::read(dir.join(MANIFEST_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let text = |k: &str| {
            doc.as_ref()
                .and_then(|d| d.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .filter(|s| !s.trim().is_empty())
        };
        let folder = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let id = text("id")
            .filter(|i| crate::manifest::valid_id(i))
            .unwrap_or(folder);
        let name = text("name").unwrap_or_else(|| id.clone());
        let req = doc
            .as_ref()
            .and_then(|d| d.pointer("/compat/launcher"))
            .and_then(|v| v.as_str())
            .and_then(|r| semver::VersionReq::parse(r).ok())
            .filter(|r| !r.matches(launcher));
        let schema = doc
            .as_ref()
            .and_then(|d| d.get("schema_version"))
            .and_then(|v| v.as_u64())
            .filter(|v| *v > u64::from(crate::manifest::SCHEMA_VERSION));
        let message = match err {
            swofty_addon::Error::Json { source, .. } => source.to_string(),
            other => other.to_string(),
        };
        let unknown = unknown_part(&message);
        let reason = match (&req, &unknown) {
            (Some(req), Some(u)) => format!("needs Swofty Launcher {req}; this is {launcher} ({u})"),
            (Some(req), None) => format!("needs Swofty Launcher {req}; this is {launcher}"),
            (None, Some(u)) => format!("needs a newer Swofty Launcher ({u})"),
            (None, None) => match schema {
                Some(v) => format!("needs a newer Swofty Launcher (schema_version {v})"),
                None => format!("can't be read: {message}"),
            },
        };
        Self {
            newer: req.is_some() || unknown.is_some() || schema.is_some(),
            version: text("version"),
            dir: dir.to_path_buf(),
            id,
            name,
            reason,
        }
    }

    /// "Race HUD (w3.hud) needs a newer Swofty Launcher (unknown field `widgets`)".
    pub fn line(&self) -> String {
        if self.name == self.id {
            format!("{} {}", self.id, self.reason)
        } else {
            format!("{} ({}) {}", self.name, self.id, self.reason)
        }
    }
}

/// "unknown field `hud`" or "unknown value `postfx`" from a serde message.
pub(super) fn unknown_part(message: &str) -> Option<String> {
    for (marker, word) in [("unknown field `", "field"), ("unknown variant `", "value")] {
        if let Some(at) = message.find(marker) {
            let rest = &message[at + marker.len()..];
            let name = &rest[..rest.find('`')?];
            return Some(format!("unknown {word} `{name}`"));
        }
    }
    None
}

/// Before a save replaces the settings file at `path`, move one this build can't read aside to
/// `<name>.unreadable-<unix time>`, so the defaults [`load_settings`] fell back to don't overwrite
/// the only copy of the player's settings.
pub(super) fn set_aside_unreadable<T: serde::de::DeserializeOwned>(path: &Path) -> Result<()> {
    if let Err(e @ Error::Json { .. }) = crate::load_json::<T>(path) {
        let to = crate::set_aside(path, crate::UNREADABLE_SUFFIX)?;
        warn(format!(
            "{e}; moved it to {} and saved new settings in its place",
            to.display()
        ));
    }
    Ok(())
}

pub(super) fn manifest_id(dir: &Path) -> Option<String> {
    let doc: serde_json::Value = crate::read_json(&dir.join(MANIFEST_FILE)).ok()?;
    Some(doc.get("id")?.as_str()?.to_owned())
}

pub(super) fn subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(dir)(e)),
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    out.sort();
    Ok(out)
}

impl Store {
    /// The installed addons this launcher can read. One it can't is left out with a warning;
    /// [`unreadable_addons`](Self::unreadable_addons) lists them.
    pub fn addons(&self) -> Result<BTreeMap<String, AddonManifest>> {
        Ok(self.scan_addons()?.0)
    }

    /// Installed addons this launcher can't read: made for a newer launcher, or damaged.
    pub fn unreadable_addons(&self) -> Vec<Unreadable> {
        self.scan_addons().map(|(_, bad)| bad).unwrap_or_default()
    }

    pub(super) fn scan_addons(&self) -> Result<(BTreeMap<String, AddonManifest>, Vec<Unreadable>)> {
        let mut out = BTreeMap::from([(CORE_ID.to_owned(), core_manifest())]);
        for b in self.builtins.iter() {
            let m = b.addon.manifest();
            out.entry(m.id.clone()).or_insert_with(|| m.clone());
        }
        let mut bad = Vec::new();
        for dir in subdirs(&self.addons_dir())? {
            if !dir.join(MANIFEST_FILE).is_file() {
                continue;
            }
            let m = match AddonManifest::load(&dir) {
                Ok(m) => m,
                Err(e) => {
                    let u = Unreadable::of(&dir, &e, &crate::updates::release::current_version());
                    warn_once(format!("skipped addon {}: it {}", u.id, u.reason));
                    bad.push(u);
                    continue;
                }
            };
            if self.is_builtin(&m.id) {
                continue;
            }
            out.insert(m.id.clone(), m);
        }
        Ok((out, bad))
    }

    pub fn is_builtin(&self, id: &str) -> bool {
        id == CORE_ID || self.builtins.iter().any(|b| b.addon.manifest().id == id)
    }

    pub(super) fn adopt_builtins(&self, p: &mut Profile) {
        if p.is_vanilla() {
            return;
        }
        for b in self.builtins.iter() {
            let id = &b.addon.manifest().id;
            let listed = p.addons.iter().any(|a| &a.id == id);
            if b.locked || (b.default_on && !listed) {
                p.set_enabled(id, true).expect("not vanilla");
            }
        }
        if self
            .addons_dir()
            .join(crate::updates::managed::ID)
            .join(MANIFEST_FILE)
            .is_file()
        {
            p.set_enabled(crate::updates::managed::ID, true)
                .expect("not vanilla");
        }
    }
}
