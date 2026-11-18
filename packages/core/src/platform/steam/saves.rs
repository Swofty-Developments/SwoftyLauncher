//! The game's Steam Cloud save folder, for the signed-in or most recent account.

use std::path::{Path, PathBuf};

use super::vdf::Vdf;
use crate::STEAM_APP_ID;

use super::library::{drive_roots, drives, find_game, isolated_root, root_of_game_in, steam_roots};
use super::read_vdf;

pub(super) const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

pub(super) fn has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|d| d.flatten().any(|e| e.file_type().is_ok_and(|t| t.is_file())))
}

/// The game's Steam Cloud save folder, `userdata/<steamid3>/<app>/remote`: for the
/// most recent user in loginusers.vdf, else the user whose folder changed last.
pub fn save_dir(steam_root: &Path) -> Option<PathBuf> {
    save_dir_with(steam_root, None, &mut Vec::new())
}

/// [`save_dir`] that prefers `active_user` (Steam's `ActiveUser`, 0 for nobody) and then the
/// `MostRecent` account in loginusers.vdf, then the other accounts by sign-in time and folder
/// change time, and takes the first whose folder holds files, else the first that exists. `tried`
/// gets one line per folder looked at and why it was rejected.
pub fn save_dir_with(
    steam_root: &Path,
    active_user: Option<u32>,
    tried: &mut Vec<String>,
) -> Option<PathBuf> {
    let userdata = steam_root.join("userdata");
    let mut order: Vec<(String, &'static str)> = Vec::new();
    let mut push = |id3: String, why: &'static str| {
        if !order.iter().any(|(i, _)| *i == id3) {
            order.push((id3, why));
        }
    };
    if let Some(user) = active_user.filter(|u| *u != 0) {
        push(user.to_string(), "ActiveUser");
    }
    if let Some(v) = read_vdf(&steam_root.join("config/loginusers.vdf"))
        && let Some(users) = v.get("users")
    {
        let stamp = |e: &Vdf| {
            e.get("Timestamp")
                .and_then(Vdf::str)
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0)
        };
        let mut all: Vec<_> = users
            .entries()
            .filter_map(|(id, e)| {
                let id3 = id.parse::<u64>().ok()?.checked_sub(STEAMID64_BASE)?;
                let most_recent = e.get("MostRecent").and_then(Vdf::str) == Some("1");
                Some((most_recent, stamp(e), id3.to_string()))
            })
            .collect();
        all.sort_by_key(|u| std::cmp::Reverse((u.0, u.1)));
        for (most_recent, _, id3) in all {
            push(
                id3,
                match most_recent {
                    true => "loginusers.vdf MostRecent",
                    false => "loginusers.vdf",
                },
            );
        }
    } else {
        tried.push(format!(
            "{}: no readable loginusers.vdf",
            steam_root.join("config/loginusers.vdf").display()
        ));
    }
    let mut folders: Vec<(Option<std::time::SystemTime>, String)> = std::fs::read_dir(&userdata)
        .map(|d| {
            d.flatten()
                .filter_map(|u| {
                    let name = u.file_name().to_str()?.to_owned();
                    let remote = u.path().join(STEAM_APP_ID.to_string()).join("remote");
                    Some((std::fs::metadata(remote).and_then(|m| m.modified()).ok(), name))
                })
                .collect()
        })
        .unwrap_or_default();
    if folders.is_empty() {
        tried.push(format!("{}: no userdata accounts", userdata.display()));
    }
    folders.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, id3) in folders {
        push(id3, "newest userdata folder");
    }
    let mut first_existing = None;
    for (id3, why) in order {
        let dir = userdata.join(&id3).join(STEAM_APP_ID.to_string()).join("remote");
        if !dir.is_dir() {
            tried.push(format!("{} ({why}): rejected, no such folder", dir.display()));
        } else if has_files(&dir) {
            tried.push(format!("{} ({why}): chosen, it holds save files", dir.display()));
            return Some(dir);
        } else {
            tried.push(format!(
                "{} ({why}): rejected, the folder is empty",
                dir.display()
            ));
            first_existing.get_or_insert(dir);
        }
    }
    first_existing
}

/// save_dir for the Steam install that holds the game, then for every other one.
pub fn find_save_dir() -> Option<PathBuf> {
    search_save(find_game().map(|g| g.dir).as_deref()).dir
}

/// The Steam Cloud save folder found, and what was looked at on the way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveSearch {
    pub dir: Option<PathBuf>,
    pub tried: Vec<String>,
}

/// The save folder for the game in `game`. The Steam root comes from the library's own Steam
/// install, never from the game's path: `userdata` is always under the root, and a library on
/// another drive has none. Roots come from the library's `libraryfolders.vdf` owner, then the
/// default folders, the registry and a running steam.exe, and last from the usual folders on every
/// drive.
pub fn search_save(game: Option<&Path>) -> SaveSearch {
    if isolated_root().is_some() {
        return search_save_in(game, &steam_roots(), &[], None);
    }
    search_save_in(game, &steam_roots(), &drive_roots(&drives()), active_user())
}

pub fn search_save_in(
    game: Option<&Path>,
    roots: &[PathBuf],
    scanned: &[PathBuf],
    active_user: Option<u32>,
) -> SaveSearch {
    let mut tried = Vec::new();
    let real = |x: &Path| x.canonicalize().unwrap_or_else(|_| x.to_path_buf());
    let mut order: Vec<(PathBuf, &str)> = Vec::new();
    let add = |order: &mut Vec<(PathBuf, &str)>, root: PathBuf, why: &'static str| {
        if !order.iter().any(|(r, _)| real(r) == real(&root)) {
            order.push((root, why));
        }
    };
    match game {
        Some(game) => match root_of_game_in(game, roots) {
            Some(root) => add(
                &mut order,
                root,
                "the Steam install that lists the game's library",
            ),
            None => tried.push(format!(
                "{}: no known Steam root lists this library in libraryfolders.vdf",
                game.display()
            )),
        },
        None => tried.push("no game folder to find the library of".to_owned()),
    }
    for root in roots {
        add(&mut order, root.clone(), "a known Steam install");
    }
    let mut empty: Option<PathBuf> = None;
    let mut look = |order: &[(PathBuf, &str)], tried: &mut Vec<String>| {
        for (root, why) in order {
            tried.push(format!("Steam root {} ({why})", root.display()));
            if let Some(dir) = save_dir_with(root, active_user, tried) {
                if has_files(&dir) {
                    return Some(dir);
                }
                empty.get_or_insert(dir);
            }
        }
        None
    };
    let mut found = look(&order, &mut tried);
    if found.is_none() {
        let mut more = Vec::new();
        for root in scanned
            .iter()
            .filter(|r| !order.iter().any(|(o, _)| real(o) == real(r)))
        {
            add(&mut more, root.clone(), "found by looking on every drive");
        }
        if more.is_empty() && order.is_empty() {
            tried.push("no Steam install found in the registry, the default folders or on any drive".into());
        }
        found = look(&more, &mut tried);
    }
    SaveSearch {
        dir: found.or(empty),
        tried,
    }
}

#[cfg(windows)]
pub fn active_user() -> Option<u32> {
    use super::client::{self, Registry};
    let out = client::LocalRegistry.query(client::ACTIVE_PROCESS_KEY)?;
    crate::platform::run_as_admin::reg_dword(&out, "ActiveUser").filter(|u| *u != 0)
}

#[cfg(not(windows))]
pub fn active_user() -> Option<u32> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    [".steam/registry.vdf", ".local/share/Steam/registry.vdf"]
        .iter()
        .find_map(|f| {
            read_vdf(&home.join(f))?
                .path(&[
                    "Registry",
                    "HKCU",
                    "Software",
                    "Valve",
                    "Steam",
                    "ActiveProcess",
                    "ActiveUser",
                ])?
                .str()?
                .trim()
                .parse::<u32>()
                .ok()
        })
        .filter(|u| *u != 0)
}
