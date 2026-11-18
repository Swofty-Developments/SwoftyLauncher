//! A DB Editor addon's name and id: a name made unique among the installed ones, its slug, and an id no other
//! addon has.

use std::collections::HashSet;
use std::time::SystemTime;

use super::{ID_PREFIX, MARKER, MAX_TRIES, dir, list};
use crate::manifest::valid_id;
use crate::store::Store;
use crate::{Error, Result};

/// `name`, or `name (2)`, `name (3)`… when another addon already has it or
/// `renumber` is set.
pub(super) fn unique_name(store: &Store, name: &str, renumber: bool) -> Result<String> {
    let names: HashSet<String> = list(store)?.into_iter().map(|p| p.name.to_lowercase()).collect();
    if !renumber && !names.contains(&name.to_lowercase()) {
        return Ok(name.to_owned());
    }
    (2..MAX_TRIES)
        .map(|n| format!("{name} ({n})"))
        .find(|n| !names.contains(&n.to_lowercase()))
        .ok_or_else(|| too_many(name))
}

pub(super) fn too_many(name: &str) -> Error {
    Error::Other(format!("too many addons are named like {name}; rename one"))
}

/// The modified time of an addon's `dbedit.json`.
pub fn modified(store: &Store, id: &str) -> Option<SystemTime> {
    if !valid_id(id) {
        return None;
    }
    std::fs::metadata(dir(store, id).join(MARKER))
        .and_then(|m| m.modified())
        .ok()
}

/// Lowercase `[a-z0-9-]`, words joined by single dashes.
pub(super) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(40);
    match out.trim_end_matches('-') {
        "" => "addon".into(),
        s => s.into(),
    }
}

pub(super) fn taken(store: &Store, id: &str) -> Result<bool> {
    Ok(store.addons()?.contains_key(id) || dir(store, id).exists())
}

pub(super) fn unique_id(store: &Store, name: &str) -> Result<String> {
    let base = format!("{ID_PREFIX}{}", slug(name));
    let installed = store.addons()?;
    std::iter::once(base.clone())
        .chain((2..MAX_TRIES).map(|n| format!("{base}-{n}")))
        .find(|id| !installed.contains_key(id) && !dir(store, id).exists())
        .ok_or_else(|| too_many(name))
}
