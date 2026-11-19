//! The records of an addon's hud file as composer inputs, with the unique prefix of each record's names.

use super::model::HudElement;
use super::{Input, err};
use crate::{Error, Result};
use std::path::Path;

pub(super) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    out.trim_matches('_').to_owned()
}

/// The prefix of a record's element and template node names.
pub fn prefix_for(addon: &str, key: &str, explicit: Option<&str>) -> Result<String> {
    let p = match explicit {
        Some(p) => p.to_owned(),
        None => {
            let tail = addon.rsplit('.').next().unwrap_or(addon);
            let a = slug(addon);
            if slug(tail) == slug(key) {
                a
            } else {
                format!("{a}_{}", slug(key))
            }
        }
    };
    let ok = !p.is_empty()
        && p.len() <= 24
        && p.starts_with(|c: char| c.is_ascii_lowercase())
        && p.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(p)
    } else {
        Err(err(format!(
            "{addon}/{key}: the prefix {p:?} must be 1 to 24 characters [a-z0-9_] starting with a letter; set `prefix` in the record"
        )))
    }
}

/// Reads the records of an addon's hud file into inputs.
pub fn inputs_of(addon: &str, dir: &Path, file: &str) -> Result<Vec<Input>> {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).map_err(|e| Error::Io {
        path: path.clone(),
        source: e,
    })?;
    let doc = swofty_addon::ops::HudFile::parse(&text)?;
    let mut out = Vec::new();
    for (id, value) in doc.elements {
        let key = id.rsplit('/').next().unwrap_or(&id).to_owned();
        let record = HudElement::parse(&value).map_err(|e| err(format!("{path:?}: {id}: {e}")))?;
        out.push(Input {
            addon: addon.to_owned(),
            key,
            dir: dir.to_path_buf(),
            record,
        });
    }
    Ok(out)
}
