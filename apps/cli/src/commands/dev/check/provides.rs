use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use swofty_core::store::Store;

use crate::commands::dev::check::read_json;
use crate::commands::dev::lock::LOCK_FILE;

pub(super) fn provided(lock: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (registry, list) in lock["provides"].as_object().into_iter().flatten() {
        for p in list.as_array().into_iter().flatten() {
            if let Some(id) = p.as_str().or_else(|| p["id"].as_str()) {
                out.push(format!("{registry}:{id}"));
            }
        }
    }
    out
}

pub(super) fn data_files(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(folder.join("registry"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub(super) fn data_ids(addon: &str, files: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for f in files {
        let Some(Ok(doc)) = read_json(f) else { continue };
        for (registry, records) in doc.as_object().into_iter().flatten() {
            for key in records.as_object().into_iter().flatten().map(|(k, _)| k) {
                if key.starts_with('$') {
                    continue;
                }
                let id = if key.contains('/') {
                    key.clone()
                } else {
                    format!("{addon}/{key}")
                };
                if id.starts_with(&format!("{addon}/")) {
                    out.push(format!("{registry}:{id}"));
                }
            }
        }
    }
    out
}

pub(super) fn installed_ids(store: &Store, except: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(addons) = store.addons() else { return out };
    for id in addons.keys().filter(|id| *id != except) {
        let dir = store.addons_dir().join(id);
        if let Some(Ok(lock)) = read_json(&dir.join(LOCK_FILE)) {
            out.extend(provided(&lock));
        }
        out.extend(data_ids(id, &data_files(&dir)));
    }
    out
}
