//! The compatibility tool Steam runs the game with, and the Proton folder it names.

use std::path::{Path, PathBuf};

use super::vdf::Vdf;
use crate::STEAM_APP_ID;

use super::library::libraries;
use super::read_vdf;

pub fn compat_tool(steam_root: &Path) -> Option<String> {
    let cfg = read_vdf(&steam_root.join("config/config.vdf"))?;
    let map = cfg.path(&[
        "InstallConfigStore",
        "Software",
        "Valve",
        "Steam",
        "CompatToolMapping",
    ])?;
    let name = |key: &str| {
        map.get(key)?
            .get("name")?
            .str()
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
    };
    name(&STEAM_APP_ID.to_string()).or_else(|| name("0"))
}

pub(super) fn valve_folder_name(tool: &str) -> Option<String> {
    let rest = tool.strip_prefix("proton_")?;
    match rest {
        "experimental" => Some("Proton - Experimental".to_owned()),
        "hotfix" => Some("Proton Hotfix".to_owned()),
        v if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) => Some(format!("Proton {v}.0")),
        _ => None,
    }
}

pub(super) fn manifest_names(dir: &Path) -> Vec<String> {
    let Some(v) = read_vdf(&dir.join("compatibilitytool.vdf")) else {
        return Vec::new();
    };
    let Some(tools) = v.path(&["compatibilitytools", "compat_tools"]) else {
        return Vec::new();
    };
    tools
        .entries()
        .flat_map(|(k, e)| {
            [
                Some(k.clone()),
                e.get("display_name").and_then(Vdf::str).map(str::to_owned),
            ]
        })
        .flatten()
        .collect()
}

pub struct ProtonSearch {
    pub found: Option<PathBuf>,
    pub searched: Vec<PathBuf>,
}

/// The Proton directory (the one holding `proton`) for the compat tool name
/// Steam records: a folder of that name, a tool whose compatibilitytool.vdf
/// declares the name, or Valve's own folder for an internal name such as
/// `proton_9`. Searches compatibilitytools.d and every library's
/// steamapps/common, with `first` searched ahead of the rest.
pub fn find_proton(steam_root: &Path, first: Option<&Path>, tool: &str) -> ProtonSearch {
    find_proton_in(steam_root, first, tool, &system_compat_dirs())
}

pub fn system_compat_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("STEAM_EXTRA_COMPAT_TOOLS_PATHS")
        .map(|v| {
            std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default();
    for fixed in [
        "/usr/share/steam/compatibilitytools.d",
        "/usr/local/share/steam/compatibilitytools.d",
    ] {
        dirs.push(PathBuf::from(fixed));
    }
    dirs
}

pub fn find_proton_in(
    steam_root: &Path,
    first: Option<&Path>,
    tool: &str,
    system: &[PathBuf],
) -> ProtonSearch {
    let mut homes = vec![steam_root.join("compatibilitytools.d")];
    for dir in system {
        if !homes.contains(dir) {
            homes.push(dir.clone());
        }
    }
    let mut libs: Vec<PathBuf> = first.into_iter().map(Path::to_path_buf).collect();
    libs.extend(libraries(steam_root));
    for lib in libs {
        let common = lib.join("steamapps/common");
        if !homes.contains(&common) {
            homes.push(common);
        }
    }
    let valve = valve_folder_name(tool);
    let matches = |dir: &Path| {
        let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned());
        let named = |want: &str| folder.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(want));
        named(tool) || valve.as_deref().is_some_and(named) || manifest_names(dir).iter().any(|n| n == tool)
    };
    let mut found = None;
    for home in &homes {
        let Ok(rd) = std::fs::read_dir(home) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        if let Some(hit) = dirs
            .into_iter()
            .find(|d| d.join("proton").is_file() && matches(d))
        {
            found = Some(hit);
            break;
        }
    }
    ProtonSearch {
        found,
        searched: homes,
    }
}
