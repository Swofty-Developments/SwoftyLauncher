use super::*;

/// The game folder's file names among the crash report's loaded modules.
fn game_folder_modules(report: &str) -> Vec<String> {
    let exe = report
        .lines()
        .find_map(|l| l.strip_prefix("exe")?.trim().split(", ").next())
        .unwrap_or_default();
    let Some((folder, _)) = exe.rsplit_once('\\') else {
        return Vec::new();
    };
    report
        .lines()
        .filter_map(|l| l.trim().split_once("  ").and(l.find(folder)).map(|i| &l[i..]))
        .filter_map(|p| p.strip_prefix(folder)?.strip_prefix('\\'))
        .filter(|rest| !rest.contains('\\'))
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore]
fn diagnoses_real_bundles() {
    let Some(root) = std::env::var_os("SWOFTY_KP_CASES") else {
        return;
    };
    let dirs = |p: &Path| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        v.sort();
        v
    };
    let json = |p: &Path| -> serde_json::Value {
        read_capped(p)
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    };
    for case in dirs(Path::new(&root)) {
        let logs = case.join("logs");
        let addons = json(&logs.join("addons.json"));
        let installed = addons["installed"].as_array().cloned().unwrap_or_default();
        let names = installed
            .iter()
            .filter_map(|a| Some((a["id"].as_str()?.to_owned(), a["name"].as_str()?.to_owned())))
            .collect();
        let versions: BTreeMap<String, String> = installed
            .iter()
            .filter_map(|a| Some((a["id"].as_str()?.to_owned(), a["version"].as_str()?.to_owned())))
            .collect();
        let conflicts: crate::profile::conflicts::Report =
            serde_json::from_value(json(&logs.join("conflicts.json"))).unwrap_or_default();
        let tracks = conflicts
            .order
            .iter()
            .filter(|id| {
                installed
                    .iter()
                    .any(|a| a["id"].as_str() == Some(id.as_str()) && a["category"] == "tracks")
            })
            .cloned()
            .collect();
        let off: Vec<String> = addons["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["active"] == true)
            .flat_map(|p| p["addons"].as_array().cloned().unwrap_or_default())
            .filter(|a| a["enabled"] == false)
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .filter(|id| {
                installed.iter().any(|a| {
                    a["id"].as_str() == Some(id.as_str())
                        && (a["category"] == "tracks" || a["category"] == "seasons")
                })
            })
            .collect();
        for session in dirs(&logs.join("sessions")) {
            let record = SessionRecord::read(&session);
            let loader = ["loader.log", "game-logs/loader.log"]
                .iter()
                .find_map(|rel| read_capped(&session.join(rel)))
                .unwrap_or_default();
            let crashed = record.crashed()
                || loader.contains("reached the top-level filter")
                || loader.contains("[crash] crash report ")
                || loader.contains(LOW_DISK);
            let report = report_name(&loader).and_then(|n| read_capped(&logs.join("crash").join(n)));
            let game_files = report.as_deref().map(game_folder_modules).unwrap_or_default();
            let mut e = Evidence::read(&record, None, BTreeMap::new(), None);
            let game_version = read_capped(&logs.join("summary.txt")).and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("F1 25 "))
                    .map(|v| v.trim().to_owned())
            });
            e = Evidence {
                game_version,
                loader,
                report,
                game_files,
                names: BTreeMap::clone(&names),
                versions: versions.clone(),
                enabled: conflicts.order.clone(),
                tracks: Vec::clone(&tracks),
                off: off.clone(),
                conflicts: conflicts.pairs.clone(),
                kept_lists: conflicts.kept_lists.clone(),
                ..e
            };
            let found = match crashed {
                true => diagnose(&e),
                false => stalled(&e),
            };
            if !crashed && found.is_none() {
                continue;
            }
            let found = found.map_or_else(
                || "-".to_owned(),
                |d| format!("{}\t{}\t{}", d.id, d.title, d.text()),
            );
            println!(
                "KP\t{}\t{}\t{found}",
                case.file_name().unwrap().to_string_lossy(),
                session.file_name().unwrap().to_string_lossy()
            );
        }
    }
}
