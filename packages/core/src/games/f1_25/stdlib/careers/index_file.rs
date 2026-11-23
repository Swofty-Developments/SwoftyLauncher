//! The career index file in the data folder: reading it, syncing it with the core's copy, moving careers
//! between profiles, and keeping it when the game folder is restored.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Career, DATA_PATH, GAME_PATH, launcher_wrote};

/// The careers a career index holds: `{ "careers": [...] }`, a list, or an object of records by
/// save key.
pub fn parse(text: &str) -> Vec<Career> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let one = |key: Option<String>, v: Value| -> Option<Career> {
        let mut c: Career = serde_json::from_value(v).ok()?;
        if c.key.is_none() {
            c.key = key;
        }
        (!c.career.is_empty()).then_some(c)
    };
    match v {
        Value::Array(list) => list.into_iter().filter_map(|c| one(None, c)).collect(),
        Value::Object(mut map) => match map.remove("careers") {
            Some(Value::Array(list)) => list.into_iter().filter_map(|c| one(None, c)).collect(),
            Some(Value::Object(by_key)) => by_key.into_iter().filter_map(|(k, c)| one(Some(k), c)).collect(),
            _ => map
                .into_iter()
                .filter(|(_, c)| c.is_object())
                .filter_map(|(k, c)| one(Some(k), c))
                .collect(),
        },
        _ => Vec::new(),
    }
}

pub fn read(path: &Path) -> Vec<Career> {
    std::fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn data_path(data: &Path) -> PathBuf {
    data.join(DATA_PATH)
}

pub(super) fn retarget(v: &mut Value, from: &str, to: &str) -> bool {
    match v {
        Value::Object(map) => {
            let mut changed = false;
            if map.get("career").is_some() && map.get("profile").and_then(Value::as_str) == Some(from) {
                map.insert("profile".into(), Value::String(to.into()));
                changed = true;
            }
            for child in map.values_mut() {
                changed |= retarget(child, from, to);
            }
            changed
        }
        Value::Array(list) => list.iter_mut().fold(false, |c, i| retarget(i, from, to) | c),
        _ => false,
    }
}

/// Point every career of profile `from` in the data folder's index at profile `to`.
pub fn rename_profile(data: &Path, from: &str, to: &str) -> crate::Result<()> {
    let path = data_path(data);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut index) = serde_json::from_str::<Value>(&text) else {
        return Ok(());
    };
    if retarget(&mut index, from, to) {
        crate::write_json_shared(&path, &index)?;
    }
    Ok(())
}

/// Merge the game folder's career index, written before the core used the data folder, into the
/// launcher's copy, newest record per career id first, and return the merged list. A career the
/// game no longer lists stays in the copy. A career the core wrote without a profile gets `stamp`,
/// the profile of the session that just ran.
pub fn sync(data: &Path, game: Option<&Path>, stamp: Option<&str>) -> crate::Result<Vec<Career>> {
    let mut kept = read(&data_path(data));
    let mut changed = false;
    let fresh = game.map(|g| read(&g.join(GAME_PATH))).unwrap_or_default();
    for mut c in fresh {
        changed = true;
        match kept.iter_mut().find(|k| k.career == c.career) {
            Some(k) => {
                c.profile = c.profile.or_else(|| k.profile.clone());
                for a in k.addons.iter().filter(|a| launcher_wrote(a)) {
                    if !c.addons.iter().any(|o| o.id == a.id) {
                        c.addons.push(a.clone());
                    }
                }
                *k = c;
            }
            None => kept.push(c),
        }
    }
    if let Some(stamp) = stamp {
        for c in kept.iter_mut().filter(|c| c.profile.is_none()) {
            c.profile = Some(stamp.to_owned());
            changed = true;
        }
    }
    if changed {
        crate::write_json_shared(
            &data_path(data),
            &serde_json::json!({ "version": 1, "careers": kept }),
        )?;
    }
    Ok(kept)
}

/// A restore puts a snapshot's careers back as the profile's list: the careers of `profile` (and the
/// ones with no profile yet) in the index are replaced by those of `restored`, with their own details
/// and last-played times, until the core lists the profile's careers again at the next Play. Other
/// profiles' careers stay. A restored career keeps the id the core gave it since ([`restored_ids`]).
pub fn replace_profile(data: &Path, profile: &str, restored: &[Career]) -> crate::Result<()> {
    let mine = |c: &Career| c.profile.as_deref().is_none_or(|p| p == profile);
    let (now, mut list): (Vec<Career>, Vec<Career>) = read(&data_path(data)).into_iter().partition(mine);
    let back: Vec<Career> = restored.iter().filter(|c| mine(c)).cloned().collect();
    list.extend(restored_ids(&now, back));
    crate::write_json_shared(
        &data_path(data),
        &serde_json::json!({ "version": 1, "careers": list }),
    )
}

/// A restored record the core listed from the save slots before it loaded the career takes the id the
/// core gave the career at its load since (`now` holds it under that id), so the next Play does not
/// rename it again.
pub(super) fn restored_ids(now: &[Career], mut back: Vec<Career>) -> Vec<Career> {
    let ids: Vec<String> = back.iter().map(|c| c.career.clone()).collect();
    for c in back.iter_mut().filter(|c| c.from_slots()) {
        let Some(n) = now
            .iter()
            .find(|n| !n.from_slots() && !ids.contains(&n.career) && n.same_career(c))
        else {
            continue;
        };
        let mut was: Vec<String> = n.renamed_from().map(str::to_owned).collect();
        if !was.contains(&c.career) {
            was.push(c.career.clone());
        }
        c.extra.remove("source");
        c.extra.insert("renamed_from".into(), Value::from(was));
        c.career = n.career.clone();
    }
    back
}

/// Take a career that is gone from the save (missing or deleted in game) off the data folder's index. Only
/// its record goes: the save, the career's addon data and the vault's versions stay, and the core lists the
/// career again when the game's save slots hold it again. Returns whether a record went.
pub fn forget(data: &Path, career: &str) -> crate::Result<bool> {
    let path = data_path(data);
    let list = read(&path);
    let gone = |c: &Career| c.career == career && c.is_gone();
    if !list.iter().any(gone) {
        return Ok(false);
    }
    let kept: Vec<Career> = list.into_iter().filter(|c| !gone(c)).collect();
    crate::write_json_shared(&path, &serde_json::json!({ "version": 1, "careers": kept }))?;
    Ok(true)
}

/// Where the launcher keeps the core's stat-id store: the one store of the install.
pub fn kept_ids(data: &Path) -> PathBuf {
    crate::stdlib::reports::path(data, crate::stdlib::reports::IDS)
}

/// Move what the core wrote in the game folder before it used the data folder: the career index
/// (merged), the stat-id store (the newer of the two wins) and the reports (when the data folder
/// has none). Returns one line for each thing moved.
pub fn migrate(data: &Path, game: &Path) -> crate::Result<Vec<String>> {
    use crate::stdlib::reports;
    let mut out = Vec::new();
    let old_index = game.join(GAME_PATH);
    if old_index.is_file() {
        sync(data, Some(game), None)?;
        std::fs::remove_file(&old_index).ok();
        out.push("career index".to_owned());
    }
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    for name in reports::SENT.iter().copied().chain([reports::IDS]) {
        let old = reports::legacy_path(game, name);
        if !old.is_file() {
            continue;
        }
        let new = reports::path(data, name);
        let take = !new.is_file() || (name == reports::IDS && modified(&old) > modified(&new));
        if take {
            let dir = new.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
            std::fs::copy(&old, &new).map_err(crate::io_err(&new))?;
            out.push(name.to_owned());
        }
        std::fs::remove_file(&old).ok();
    }
    Ok(out)
}

/// Before the game folder's `swofty/` goes (Restore game files): move anything the core left there
/// into the data folder, so nothing is lost. The data folder is not touched by the restore.
pub fn keep(data: &Path, game: &Path) -> crate::Result<()> {
    migrate(data, game).map(|_| ())
}
