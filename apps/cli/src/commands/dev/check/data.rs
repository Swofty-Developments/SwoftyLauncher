use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::commands::dev::check::{Report, read_json};

pub fn sdk_schema(sdk: &Path, registry: &str) -> Option<Value> {
    let dir = sdk.join("packages/sdk/std/registries").join(registry);
    let mut best: Option<((u32, u32), PathBuf)> = None;
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some((maj, min)) = name.strip_suffix(".json").and_then(|v| v.split_once('.')) else {
            continue;
        };
        let (Ok(maj), Ok(min)) = (maj.parse::<u32>(), min.parse::<u32>()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(v, _)| (maj, min) > *v) {
            best = Some(((maj, min), e.path()));
        }
    }
    read_json(&best?.1)?.ok()
}

pub(super) fn missing_paths(sdk: &Path, folder: &Path, files: &[PathBuf], r: &mut Report) {
    for f in files {
        let Some(Ok(doc)) = read_json(f) else { continue };
        let shown = format!("registry/{}", f.file_name().unwrap_or_default().to_string_lossy());
        for (registry, records) in doc.as_object().into_iter().flatten() {
            let Some(schema) = sdk_schema(sdk, registry) else {
                continue;
            };
            for (id, value) in records.as_object().into_iter().flatten() {
                if id.starts_with('$') {
                    continue;
                }
                let mut paths = Vec::new();
                crate::commands::dev::data_paths::schema_paths(&schema, &schema, value, &mut paths, 0);
                for p in paths {
                    if !crate::commands::dev::data_paths::inside(folder, &p) {
                        r.error(format!(
                            "{shown}: {registry} {id} names {p}, which isn't in the addon"
                        ));
                    }
                }
            }
        }
    }
}

pub(super) fn validate_data(sdk: &Option<PathBuf>, folder: &Path, files: &[PathBuf], r: &mut Report) {
    let (Some(sdk), Ok(py)) = (sdk, crate::commands::dev::python()) else {
        r.warn("the data files weren't checked: dev check needs python3 and the SDK's tools/std/validate.py");
        return;
    };
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_name())
        .map(|n| format!("registry/{}", n.to_string_lossy()))
        .collect();
    let out = Command::new(py)
        .arg(sdk.join("tools/std/validate.py"))
        .arg("--repo")
        .arg(sdk)
        .arg("--data")
        .args(&names)
        .current_dir(folder)
        .output();
    let Ok(out) = out else {
        r.warn("the data files weren't checked: validate.py didn't start");
        return;
    };
    let before = r.errors;
    validate_report(&String::from_utf8_lossy(&out.stdout), &names, r);
    if !out.status.success() && r.errors == before {
        r.error(format!(
            "validate.py failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
}

pub(super) fn validate_report(text: &str, names: &[String], r: &mut Report) {
    let mut current_failed = false;
    for line in text.lines() {
        let (ok, rest) = if let Some(rest) = line.strip_prefix("ok  ") {
            (true, rest)
        } else if let Some(rest) = line.strip_prefix("FAIL") {
            (false, rest)
        } else {
            if current_failed && !line.trim().is_empty() {
                r.lines.push(format!("    {}", line.trim()));
            }
            continue;
        };
        let rest = rest.trim();
        let (path, checked) = match rest.rfind(" (") {
            Some(at) if rest.ends_with(')') => rest.split_at(at),
            _ => (rest, ""),
        };
        let shown = format!("{}{checked}", shown_path(path, names));
        current_failed = !ok;
        if ok {
            r.ok(shown);
        } else {
            r.error(shown);
        }
    }
}

pub(super) fn shown_path(reported: &str, names: &[String]) -> String {
    let norm = reported.trim().replace('\\', "/").to_lowercase();
    names
        .iter()
        .find(|n| {
            let n = n.to_lowercase();
            norm == n || norm.ends_with(&format!("/{n}"))
        })
        .cloned()
        .unwrap_or_else(|| reported.trim().replace('\\', "/"))
}
