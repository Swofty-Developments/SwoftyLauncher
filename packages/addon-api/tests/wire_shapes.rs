use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use swofty_addon::asset::{FileKind, OpFile};
use swofty_addon::manifest::{AddonManifest, MANIFEST_FILE};
use swofty_addon::ops::{DbOps, HudFile, PkgPatch};

const SNAPSHOT: &str = "tests/fixtures/wire_shapes.json";
const UPDATE_ENV: &str = "SWOFTY_UPDATE_WIRE_SHAPES";
const CORPUS_DIRS: [&str; 5] = [
    "addons",
    "templates",
    "docs",
    "native/test/content",
    "packages/sdk/std",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "node_modules" || n == "target")
            {
                continue;
            }
            json_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

fn fnv(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}:{}", text.len())
}

fn shape<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, path: &str) -> String {
    let text = serde_json::to_string(&value).unwrap();
    let again: T =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: re-reading its own output: {e}"));
    assert_eq!(
        again, value,
        "{path}: serialize then deserialize changed the value"
    );
    let canonical: Value = serde_json::from_str(&text).unwrap();
    fnv(&sorted(&canonical).to_string())
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let keys: BTreeMap<&String, &Value> = map.iter().collect();
            Value::Object(keys.into_iter().map(|(k, v)| (k.clone(), sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

fn parse<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    doc: &Value,
    path: &str,
) -> Option<String> {
    serde_json::from_value::<T>(doc.clone())
        .ok()
        .map(|v| shape(v, path))
}

fn without_conditions(doc: &Value) -> Value {
    let mut doc = doc.clone();
    if let Some(top) = doc.as_object_mut() {
        top.remove("when");
        for op in top
            .get_mut("ops")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(op) = op.as_object_mut() {
                op.remove("when");
            }
        }
    }
    doc
}

fn op_file(doc: &Value, kind: FileKind, path: &str) -> Option<String> {
    OpFile::from_value(without_conditions(doc), kind)
        .ok()
        .map(|v| shape(v, path))
}

fn shapes() -> BTreeMap<String, BTreeMap<&'static str, String>> {
    let root = repo();
    let mut files = Vec::new();
    for dir in CORPUS_DIRS {
        json_files(&root.join(dir), &mut files);
    }
    let mut out = BTreeMap::new();
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let mut found = BTreeMap::new();
        if file.file_name().is_some_and(|n| n == MANIFEST_FILE) {
            found.extend(parse::<AddonManifest>(&doc, &rel).map(|s| ("manifest", s)));
        }
        if doc.get("ops").is_some() {
            found.extend(parse::<DbOps>(&doc, &rel).map(|s| ("db_ops", s)));
            found.extend(op_file(&doc, FileKind::Database, &rel).map(|s| ("database_file", s)));
            found.extend(op_file(&doc, FileKind::Erp, &rel).map(|s| ("erp_file", s)));
            found.extend(op_file(&doc, FileKind::Assets, &rel).map(|s| ("assets_file", s)));
        }
        if doc.get("edits").is_some() {
            found.extend(parse::<PkgPatch>(&doc, &rel).map(|s| ("pkg_patch", s)));
        }
        if doc.get("hud.element").is_some() {
            found.extend(parse::<HudFile>(&doc, &rel).map(|s| ("hud_file", s)));
        }
        if !found.is_empty() {
            out.insert(rel, found);
        }
    }
    out
}

#[test]
fn the_real_manifests_and_operation_files_keep_their_serde_shape() {
    let now = shapes();
    let snapshot = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        std::fs::write(&snapshot, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
        return;
    }
    let text = std::fs::read_to_string(&snapshot)
        .unwrap_or_else(|e| panic!("{}: {e}; run with {UPDATE_ENV}=1 to write it", snapshot.display()));
    let then: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(&text).unwrap();
    let now: BTreeMap<String, BTreeMap<String, String>> = now
        .into_iter()
        .map(|(path, kinds)| (path, kinds.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
        .collect();
    assert!(now.len() >= 20, "the corpus shrank to {} files", now.len());
    assert_eq!(
        now, then,
        "a manifest or operation file now serializes differently; if the corpus itself changed, run with {UPDATE_ENV}=1"
    );
}
