//! The entries the pages list, in their folders' order, with the entries in no folder under Addons, Tracks
//! and Mods.

use super::{Entry, Kind, Opt};
use crate::addons::settings::{SessionAddon, SessionMap, SessionMod};

pub fn entries<'a>(
    addons: &'a [SessionAddon],
    maps: &'a [SessionMap],
    mods: &'a [SessionMod],
) -> Vec<Entry<'a>> {
    let mut k = 0;
    let mut out = Vec::new();
    for (i, (full, m)) in addons.iter().enumerate().filter(|(_, m)| m.enabled).enumerate() {
        let all: Vec<Opt> = m
            .options
            .iter()
            .enumerate()
            .map(|(local, o)| {
                k += 1;
                Opt { k: k - 1, local, o }
            })
            .collect();
        let options = all
            .iter()
            .filter(|o| !o.o.hidden)
            .map(|o| Opt {
                k: o.k,
                local: o.local,
                o: o.o,
            })
            .collect();
        out.push(Entry {
            kind: Kind::Addon,
            index: i,
            screen: format!("swofty_addon_{full}"),
            name: &m.name,
            author: &m.author,
            version: &m.version,
            description: &m.description,
            id: &m.id,
            track: m.track,
            maps: maps.iter().filter(|mp| mp.addon == m.id).collect(),
            folder: m.folder.as_deref().unwrap_or(LOOSE_ADDONS),
            options,
            all,
            setup: m.career_setup.as_ref(),
        });
    }
    // A track package an enabled addon brings is on that addon's tile; the rest get their own.
    let brought = |mp: &SessionMap| addons.iter().any(|a| a.enabled && a.id == mp.addon);
    for (j, mp) in maps.iter().enumerate().filter(|(_, mp)| !brought(mp)) {
        out.push(Entry {
            kind: Kind::Map,
            index: j,
            screen: format!("swofty_map_{j}"),
            name: &mp.name,
            author: &mp.author,
            version: &mp.version,
            description: &mp.description,
            id: &mp.code,
            track: true,
            maps: Vec::new(),
            folder: LOOSE_TRACKS,
            options: Vec::new(),
            all: Vec::new(),
            setup: None,
        });
    }
    for (j, md) in mods.iter().enumerate() {
        out.push(Entry {
            kind: Kind::Mod,
            index: j,
            screen: format!("swofty_mod_{j}"),
            name: &md.name,
            author: &md.author,
            version: &md.version,
            description: &md.description,
            id: &md.id,
            track: false,
            maps: Vec::new(),
            folder: LOOSE_MODS,
            options: Vec::new(),
            all: Vec::new(),
            setup: None,
        });
    }
    by_folder(out)
}

/// The rows of entries that are in no folder: addons, track packages and mods. A launcher folder of
/// the same name, as the Tracks category's, takes them in.
pub(super) const LOOSE_ADDONS: &str = "Addons";

pub(super) const LOOSE_TRACKS: &str = "Tracks";

pub(super) const LOOSE_MODS: &str = "Mods";

/// Entries grouped by folder, the folders in the order their first entry comes, each folder's
/// entries in the order they come.
pub(super) fn by_folder(entries: Vec<Entry>) -> Vec<Entry> {
    let mut folders: Vec<&str> = Vec::new();
    for e in &entries {
        if !folders.contains(&e.folder) {
            folders.push(e.folder);
        }
    }
    let mut out = entries;
    out.sort_by_key(|e| folders.iter().position(|f| *f == e.folder));
    out
}
