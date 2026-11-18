mod provides;
mod schemas;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;
use swofty_core::manifest::{AddonManifest, MANIFEST_FILE};
use swofty_core::stdlib::manifest::{RegistryManifest, Target};
use swofty_core::store::Store;

use self::provides::{data_files, data_ids, installed_ids, provided};
pub use self::schemas::sdk_schema;
use self::schemas::{missing_paths, validate_data};
use super::lock::LOCK_FILE;

pub const REGISTRY_FILE: &str = "registry.json";

#[derive(Debug, Default)]
pub struct Report {
    pub lines: Vec<String>,
    pub errors: usize,
    pub warnings: usize,
}

impl Report {
    fn error(&mut self, s: impl Into<String>) {
        self.lines.push(format!("error: {}", s.into()));
        self.errors += 1;
    }

    fn warn(&mut self, s: impl Into<String>) {
        self.lines.push(format!("warning: {}", s.into()));
        self.warnings += 1;
    }

    fn ok(&mut self, s: impl Into<String>) {
        self.lines.push(format!("ok: {}", s.into()));
    }
}

pub fn check(store: &Store, sdk: &Option<PathBuf>, folder: &Path) -> Result<Report> {
    let mut r = Report::default();
    let id = match AddonManifest::load(folder) {
        Ok(m) => {
            r.ok(format!("{MANIFEST_FILE}: {} {}", m.id, m.version));
            m.id
        }
        Err(e) => {
            r.error(format!("{MANIFEST_FILE}: {e}"));
            return Ok(r);
        }
    };
    let manifest = super::manifest_json(folder)?;
    let mut needs: Vec<(Vec<String>, bool, String)> = Vec::new();
    match RegistryManifest::load(folder) {
        Ok(None) => {}
        Err(e) => r.error(e),
        Ok(Some(m)) => registry_json(&m, &mut r, &mut needs),
    }
    let has_register = match super::register_dlls(folder, &manifest) {
        Ok(dlls) => !dlls.is_empty(),
        Err(e) => {
            r.warn(format!("{e}"));
            false
        }
    };
    let mut own: BTreeSet<String> = BTreeSet::new();
    match read_json(&folder.join(LOCK_FILE)) {
        None if has_register => r.warn(format!(
            "the DLL has a SWOFTY_REGISTER but there's no {LOCK_FILE}: swofty dev pack writes it"
        )),
        None => {}
        Some(Err(e)) => r.error(format!("{LOCK_FILE}: {e}")),
        Some(Ok(lock)) => {
            if !has_register {
                r.warn(format!(
                    "{LOCK_FILE} is there but no DLL has a SWOFTY_REGISTER: swofty dev pack removes it"
                ));
            }
            own.extend(provided(&lock));
            for n in lock["needs"].as_array().into_iter().flatten() {
                if let Some(rec) = n["record"].as_str() {
                    let optional = n["policy"].as_str() == Some("optional");
                    let mut alts = vec![rec.to_owned()];
                    alts.extend(
                        n["or"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|o| o.as_str().map(str::to_owned)),
                    );
                    needs.push((alts, !optional, LOCK_FILE.to_owned()));
                }
            }
        }
    }
    let data = data_files(folder);
    own.extend(data_ids(&id, &data));
    if !data.is_empty() {
        validate_data(sdk, folder, &data, &mut r);
        if let Some(sdk) = sdk {
            missing_paths(sdk, folder, &data, &mut r);
        }
    }
    let installed = installed_ids(store, &id);
    for (alts, required, from) in needs {
        if let Some(bad) = alts.iter().find(|a| !a.contains(':')) {
            r.error(format!("{from}: {bad:?} isn't <registry>:<id>"));
            continue;
        }
        let met = alts.iter().any(|a| {
            let rid = a.split_once(':').map_or("", |(_, id)| id);
            rid.starts_with("game/") || own.contains(a) || installed.contains(a)
        });
        if met {
            continue;
        }
        let msg = format!("{from}: no installed addon provides {}", alts.join(" or "));
        if required { r.error(msg) } else { r.warn(msg) }
    }
    Ok(r)
}

fn read_json(path: &Path) -> Option<Result<Value, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).map_err(|e| e.to_string()))
}

fn registry_json(m: &RegistryManifest, r: &mut Report, needs: &mut Vec<(Vec<String>, bool, String)>) {
    for why in &m.skipped {
        r.warn(format!("{REGISTRY_FILE}: {why}; the launcher ignores it"));
    }
    for key in m.extra.keys() {
        r.warn(format!(
            "{REGISTRY_FILE}: the launcher doesn't know the field {key:?} and ignores it"
        ));
    }
    for (list, required) in [(&m.requires, true), (&m.recommends, false)] {
        for (i, n) in list.iter().enumerate() {
            if !required && n.for_.is_none() {
                r.warn(format!(
                    "{REGISTRY_FILE}: recommends[{i}]: add a \"for\" for players"
                ));
            }
            if let Target::Record { registry, id } = &n.target {
                needs.push((
                    vec![format!("{registry}:{id}")],
                    required,
                    REGISTRY_FILE.to_owned(),
                ));
            }
        }
    }
    r.ok(REGISTRY_FILE);
}

#[cfg(test)]
mod tests;
