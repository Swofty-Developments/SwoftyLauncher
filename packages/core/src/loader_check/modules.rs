use std::path::Path;

use crate::install::PROXY;

use super::{MAX_OTHERS, Modules, file_name, under};

/// Windows' compatibility shim DLLs. apphelp.dll is left out: most processes
/// load it whether a shim applies or not.
const SHIMS: &[&str] = &[
    "AcLayers.dll",
    "AcGenral.dll",
    "AcSpecfc.dll",
    "AcXtrnal.dll",
    "AcWinRT.dll",
];

/// Programs that hook into games, by the start of their DLL's name.
const HOOKS: &[(&str, &str)] = &[
    ("medal-hook", "Medal"),
    ("rtsshooks", "RivaTuner Statistics Server"),
    ("discordhook", "Discord's overlay"),
    ("graphics-hook", "OBS Studio"),
    ("windhawk", "Windhawk"),
];

/// In-process modules of security products, by the start of the DLL's name.
pub(super) const SECURITY_MODULES: &[&str] = &["mc-sec", "mfehidk", "aswhook", "snxhk", "sf2", "avghook"];

/// The version.dll files and outside modules among a process's module paths.
pub fn modules(paths: &[String], game: &Path, windows: Option<&str>) -> Modules {
    let game = game.to_string_lossy();
    let mut others: Vec<String> = paths
        .iter()
        .filter(|p| !under(p, &game) && !windows.is_some_and(|w| under(p, w)))
        .map(|p| file_name(p).to_owned())
        .collect();
    others.sort_by_key(|n| n.to_lowercase());
    others.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    others.truncate(MAX_OTHERS);
    let mut shims: Vec<String> = Vec::new();
    for p in paths {
        let name = file_name(p);
        if SHIMS.iter().any(|s| s.eq_ignore_ascii_case(name))
            && !shims.iter().any(|s| s.eq_ignore_ascii_case(name))
        {
            shims.push(name.to_owned());
        }
    }
    Modules {
        shims,
        count: paths.len(),
        version: paths
            .iter()
            .filter(|p| file_name(p).eq_ignore_ascii_case(PROXY))
            .cloned()
            .collect(),
        others,
    }
}

/// The programs that hook into games among a process's outside modules.
pub(super) fn hooks(others: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in others {
        let m = m.to_ascii_lowercase();
        if let Some((_, product)) = HOOKS.iter().find(|(prefix, _)| m.starts_with(prefix))
            && !out.iter().any(|p| p == product)
        {
            out.push((*product).to_owned());
        }
    }
    out
}

/// The DLLs a Wine process has mapped, from `/proc/<pid>/maps`.
pub fn mapped_modules(maps: &str, game: &Path) -> Modules {
    let mut paths: Vec<String> = Vec::new();
    for line in maps.lines() {
        let Some(at) = line.find('/') else { continue };
        let path = line[at..].trim_end_matches(" (deleted)");
        if path.to_ascii_lowercase().ends_with(".dll") && !paths.iter().any(|p| p == path) {
            paths.push(path.to_owned());
        }
    }
    Modules {
        others: Vec::new(),
        shims: Vec::new(),
        ..modules(&paths, game, None)
    }
}

/// The registered antivirus products as `name (on)`/`name (off)`, from a
/// product state (WSC_SECURITY_PRODUCT_STATE in bits 12-15).
pub fn antivirus_name(name: &str, state: Option<u32>) -> String {
    match state {
        Some(s) if s & 0x1000 != 0 => format!("{name} (on)"),
        Some(_) => format!("{name} (off)"),
        None => name.to_owned(),
    }
}
