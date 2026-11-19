//! Where Steam and the game are installed: the Steam roots from the usual folders, the registry and the drives, the libraries each root lists, and the root that holds the game.

use std::path::{Path, PathBuf};

use super::client;
use super::vdf::Vdf;
use crate::STEAM_APP_ID;

use super::{GameInstall, read_vdf};

/// An explicit isolation root suppresses registry, default-location and drive scans.
/// This is useful for sandboxed tools and tests that must never inspect host saves.
pub(crate) fn isolated_root() -> Option<PathBuf> {
    std::env::var_os("SWOFTY_STEAM_ROOT_ONLY").map(PathBuf::from)
}

pub fn steam_roots() -> Vec<PathBuf> {
    if let Some(root) = isolated_root() {
        return if root.join("steamapps").is_dir() {
            vec![root]
        } else {
            Vec::new()
        };
    }
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("SWOFTY_STEAM_ROOT") {
        out.push(dir.into());
    }
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                out.push(PathBuf::from(p).join("Steam"));
            }
        }
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        out.push(home.join(".steam/steam"));
        out.push(home.join(".local/share/Steam"));
        out.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        out.push(home.join(".var/app/com.valvesoftware.Steam/data/Steam"));
    }
    out.extend(registered_roots_cached());
    let mut seen = Vec::new();
    out.retain(|p| {
        let real = p.canonicalize().unwrap_or_else(|_| p.clone());
        let keep = p.join("steamapps").is_dir() && !seen.contains(&real);
        seen.push(real);
        keep
    });
    out
}

pub(super) const REGISTERED_KEYS: [(&str, &str); 4] = [
    (r"HKCU\Software\Valve\Steam", "SteamPath"),
    (r"HKCU\Software\Valve\Steam", "SteamExe"),
    (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
    (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
];

/// The Steam folders Windows records: the per-user `SteamPath` and `SteamExe`, and the machine-wide
/// `InstallPath`. Steam registers where it is installed, not where the game library is.
pub fn registered_roots(reg: &dyn client::Registry) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut last_key = "";
    let mut values: Vec<(String, String)> = Vec::new();
    for (key, name) in REGISTERED_KEYS {
        if key != last_key {
            values = reg
                .query(key)
                .map(|l| crate::platform::run_as_admin::parse_reg_query(&l))
                .unwrap_or_default();
            last_key = key;
        }
        let Some((_, data)) = values
            .iter()
            .find(|(n, d)| n.eq_ignore_ascii_case(name) && !d.trim().is_empty())
        else {
            continue;
        };
        let path = PathBuf::from(data.trim());
        let root = match name {
            "SteamExe" => path.parent().map(Path::to_path_buf),
            _ => Some(path),
        };
        out.extend(root.filter(|r| !out.contains(r)));
    }
    out
}

#[cfg(windows)]
pub(super) fn registered_roots_cached() -> Vec<PathBuf> {
    static KEPT: crate::platform::Kept<(), Vec<PathBuf>> = crate::platform::Kept::new();
    KEPT.get((), crate::platform::KEEP_RECORDS, || {
        let mut roots = registered_roots(&client::LocalRegistry);
        roots.extend(
            crate::platform::process::named_paths("steam.exe")
                .iter()
                .filter_map(|p| p.parent().map(Path::to_path_buf)),
        );
        roots
    })
}

#[cfg(not(windows))]
pub(super) fn registered_roots_cached() -> Vec<PathBuf> {
    Vec::new()
}

/// Where Steam is usually installed on each drive in `drives` (`C:\`, `D:\`, ...), for a Steam that
/// neither the registry nor a running steam.exe revealed.
pub fn drive_roots(drives: &[PathBuf]) -> Vec<PathBuf> {
    drives
        .iter()
        .flat_map(|d| {
            [
                d.join("Program Files (x86)").join("Steam"),
                d.join("Program Files").join("Steam"),
                d.join("Steam"),
            ]
        })
        .filter(|r| r.join("steamapps").is_dir() || r.join("userdata").is_dir())
        .collect()
}

#[cfg(windows)]
pub(super) fn drives() -> Vec<PathBuf> {
    ('C'..='Z').map(|c| PathBuf::from(format!("{c}:\\"))).collect()
}

#[cfg(not(windows))]
pub(super) fn drives() -> Vec<PathBuf> {
    Vec::new()
}

pub fn libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    if let Some(v) = read_vdf(&steam_root.join("steamapps/libraryfolders.vdf")) {
        let entries = v.get("libraryfolders").into_iter().flat_map(Vdf::entries);
        for (_, e) in entries {
            if let Some(p) = e.get("path").and_then(Vdf::str) {
                let p = PathBuf::from(p);
                let real = |x: &Path| x.canonicalize().unwrap_or_else(|_| x.to_path_buf());
                if !libs.iter().any(|l| real(l) == real(&p)) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

pub fn find_game() -> Option<GameInstall> {
    find_app(STEAM_APP_ID)
}

pub fn find_game_in(steam_root: &Path) -> Option<GameInstall> {
    find_app_in(steam_root, STEAM_APP_ID)
}

/// The first install of the Steam app `app_id` in any Steam root's libraries.
pub fn find_app(app_id: u32) -> Option<GameInstall> {
    steam_roots()
        .into_iter()
        .find_map(|root| find_app_in(&root, app_id))
}

/// The first library of `steam_root` whose `appmanifest_<app_id>.acf` names an install folder.
pub fn find_app_in(steam_root: &Path, app_id: u32) -> Option<GameInstall> {
    libraries(steam_root).into_iter().find_map(|lib| {
        let steamapps = lib.join("steamapps");
        let acf = read_vdf(&steamapps.join(format!("appmanifest_{app_id}.acf")))?;
        let state = acf.get("AppState")?;
        let dir = steamapps.join("common").join(state.get("installdir")?.str()?);
        let build_id = state
            .get("buildid")
            .and_then(Vdf::str)
            .and_then(|b| b.parse().ok());
        let compat = steamapps.join("compatdata").join(app_id.to_string());
        Some(GameInstall {
            steam_root: steam_root.to_path_buf(),
            library: lib.clone(),
            dir,
            build_id,
            compatdata: compat.is_dir().then_some(compat),
        })
    })
}

/// True when `dir` is a Steam copy of the game: it sits in a library's
/// `steamapps/common` next to the app's manifest.
pub fn in_steam_library(dir: &Path) -> bool {
    let shaped = |dir: &Path| {
        let named = |p: Option<&Path>, name: &str| {
            p.and_then(Path::file_name)
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        };
        let common = dir.parent();
        let steamapps = common.and_then(Path::parent);
        named(common, "common")
            && named(steamapps, "steamapps")
            && steamapps.is_some_and(|s| s.join(format!("appmanifest_{STEAM_APP_ID}.acf")).is_file())
    };
    shaped(dir) || dir.canonicalize().is_ok_and(|real| shaped(&real))
}

/// The Steam install whose `userdata` holds the saves of the game at `game`
/// (`<library>/steamapps/common/<game>`): the library itself when it is a Steam root,
/// else the known root whose `libraryfolders.vdf` lists the library.
pub fn root_of_game_in(game: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let common = game.parent()?;
    let steamapps = common.parent()?;
    let named = |p: &Path, name: &str| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
    };
    if !named(common, "common") || !named(steamapps, "steamapps") {
        return None;
    }
    let library = steamapps.parent()?;
    let real = |x: &Path| x.canonicalize().unwrap_or_else(|_| x.to_path_buf());
    let is_root = |r: &Path| r.join("userdata").is_dir() || r.join("config/loginusers.vdf").is_file();
    if is_root(library) {
        return Some(library.to_path_buf());
    }
    roots
        .iter()
        .find(|r| libraries(r).iter().any(|l| real(l) == real(library)))
        .cloned()
}

pub fn root_of_game(game: &Path) -> Option<PathBuf> {
    root_of_game_in(game, &steam_roots())
}
