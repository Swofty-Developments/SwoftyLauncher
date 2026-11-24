//! Matching a host's addons: what the Addon Browser installs, what stops a match, and the profile a game
//! started to play with the host uses. The player's own profiles stay as they are.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::types::{HostAddon, HostPrint, RoomAddons};
use crate::addons::builtin::CORE_ID;
use crate::catalog::requires::Event;
use crate::catalog::{self, Addon, Stage};
use crate::install::Game;
use crate::manifest::{AddonManifest, Multiplayer};
use crate::profile::Profile;
use crate::store::Store;

/// The profile a game started to play with a host builds; the profile picker leaves it out.
pub const ROOM_PROFILE: &str = "-room";

pub fn is_room_profile(name: &str) -> bool {
    name == ROOM_PROFILE
}

/// One installed addon: its version and the Addon Browser download it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Have {
    pub version: semver::Version,
    pub from: Option<String>,
}

/// What this player has to match a host with: the launcher, the game, the installed addons and the Addon
/// Browser's listing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Own {
    pub launcher: String,
    pub exe_sha256: Option<String>,
    pub game_version: Option<String>,
    pub dlc: Option<bool>,
    pub have: BTreeMap<String, Have>,
    pub catalog: Vec<Addon>,
}

impl Own {
    pub fn read(store: &Store, game: Option<&Game>) -> crate::Result<Own> {
        let have = store
            .addons()?
            .values()
            .map(|m| {
                let from = catalog::installed_from(store, m);
                (
                    m.id.clone(),
                    Have {
                        version: m.version.clone(),
                        from,
                    },
                )
            })
            .collect();
        Ok(Own {
            launcher: crate::updates::release::current_version().to_string(),
            exe_sha256: game.map(|g| g.identity.exe_sha256.clone()),
            game_version: game.and_then(Game::version),
            dlc: None,
            have,
            catalog: catalog::cached(&store.root).unwrap_or_default(),
        })
    }

    /// Whether the host's `game` is another game than this player's: another FileVersion when both sides
    /// know theirs, else another exe. An unknown side never differs.
    fn plays_another_game(&self, game: HostGame<'_>) -> bool {
        match (game.version, self.game_version.as_deref()) {
            (Some(host), Some(mine)) => host != mine,
            _ => matches!(
                (game.exe_sha256, self.exe_sha256.as_deref()),
                (Some(host), Some(mine)) if !host.eq_ignore_ascii_case(mine)
            ),
        }
    }
}

/// The host's game as its fingerprint or its room listing names it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostGame<'a> {
    pub exe_sha256: Option<&'a str>,
    pub version: Option<&'a str>,
}

/// Why the Addon Browser can't give the host's copy of an addon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlisted {
    /// The host's copy didn't come from the Addon Browser, or the Addon Browser doesn't list that version.
    NotListed,
    /// The Addon Browser lists that version with other files than the host's copy.
    OtherCopy,
    /// That version needs another Swofty Launcher.
    NeedsLauncher,
}

/// What stops this player from matching the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    Launcher {
        host: String,
    },
    Game,
    Dlc,
    /// A built-in addon the host's launcher has and this one doesn't, at that version.
    Builtin {
        name: String,
        version: semver::Version,
    },
    Addon {
        name: String,
        version: semver::Version,
        why: Unlisted,
    },
}

/// One addon the Addon Browser installs to match the host: the listed version whose download is the host's
/// copy, and the version of the copy it replaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    pub addon: Addon,
    pub version: catalog::Version,
    pub replaces: Option<semver::Version>,
}

impl Install {
    pub fn name(&self) -> &str {
        &self.addon.name
    }
}

/// What matching the host takes: the installs, and what stops it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub installs: Vec<Install>,
    pub blockers: Vec<Blocker>,
}

impl Plan {
    pub fn bytes(&self) -> u64 {
        self.installs.iter().map(|i| i.version.size).sum()
    }
}

/// The host's addons from a room listing that names their versions; `None` from a server that doesn't.
pub fn listed_addons(addons: &RoomAddons) -> Option<Vec<HostAddon>> {
    if addons.list.is_empty() || addons.list.iter().any(|a| a.version.is_none()) {
        return None;
    }
    addons
        .list
        .iter()
        .map(|a| {
            Some(HostAddon {
                id: a.id.clone(),
                name: a.name.clone(),
                version: semver::Version::parse(a.version.as_deref()?).ok()?,
                registry: a.registry.clone(),
                order: a.order,
                builtin: a.builtin,
            })
        })
        .collect()
}

/// The listed version of `h` whose download is the host's copy.
pub fn listing<'a>(
    catalog: &'a [Addon],
    h: &HostAddon,
    launcher: &semver::Version,
) -> Result<(&'a Addon, &'a catalog::Version), Unlisted> {
    let sha = h.registry.as_deref().ok_or(Unlisted::NotListed)?;
    let addon = catalog.iter().find(|a| a.id == h.id).ok_or(Unlisted::NotListed)?;
    let v = addon
        .listed()
        .into_iter()
        .find(|v| v.version == h.version)
        .ok_or(Unlisted::NotListed)?;
    if !v.sha256.trim().eq_ignore_ascii_case(sha) {
        return Err(Unlisted::OtherCopy);
    }
    if !v.accepts(launcher) {
        return Err(Unlisted::NeedsLauncher);
    }
    Ok((addon, v))
}

/// What one of the host's addons needs here.
#[derive(Debug, Clone, PartialEq)]
pub enum Need {
    Have,
    Install(Box<Install>),
    Blocked(Blocker),
}

/// What each of the host's `addons` needs, in their order; `launcher` is the host's when known.
pub fn needs(addons: &[HostAddon], launcher: Option<&str>, own: &Own) -> Vec<Need> {
    let current = semver::Version::parse(&own.launcher).unwrap_or_else(|_| semver::Version::new(0, 0, 0));
    let same_launcher = launcher.is_none_or(|l| l == own.launcher);
    addons
        .iter()
        .map(|h| {
            let mine = own.have.get(&h.id);
            if h.builtin {
                let differs = mine.is_none_or(|m| m.version != h.version);
                if differs && same_launcher {
                    return Need::Blocked(Blocker::Builtin {
                        name: h.name.clone(),
                        version: h.version.clone(),
                    });
                }
                return Need::Have;
            }
            let other_copy = |m: &Have| match (&m.from, &h.registry) {
                (Some(from), Some(host)) => !from.eq_ignore_ascii_case(host),
                _ => false,
            };
            match (mine, listing(&own.catalog, h, &current)) {
                (Some(m), _) if m.version == h.version && !other_copy(m) => Need::Have,
                (Some(m), Err(_)) if m.version == h.version => Need::Have,
                (mine, Ok((addon, v))) => Need::Install(Box::new(Install {
                    addon: addon.clone(),
                    version: v.clone(),
                    replaces: mine.map(|m| m.version.clone()),
                })),
                (_, Err(why)) => Need::Blocked(Blocker::Addon {
                    name: h.name.clone(),
                    version: h.version.clone(),
                    why,
                }),
            }
        })
        .collect()
}

/// What matching the host's `addons` takes, with `launcher`, `game` and `dlc` from the host when known.
pub fn plan(
    addons: &[HostAddon],
    launcher: Option<&str>,
    game: HostGame<'_>,
    dlc: Option<bool>,
    own: &Own,
) -> Plan {
    let mut out = Plan::default();
    if let Some(host) = launcher.filter(|l| *l != own.launcher) {
        out.blockers.push(Blocker::Launcher {
            host: host.to_owned(),
        });
    }
    if own.plays_another_game(game) {
        out.blockers.push(Blocker::Game);
    }
    if let (Some(host), Some(mine)) = (dlc, own.dlc)
        && host != mine
    {
        out.blockers.push(Blocker::Dlc);
    }
    for need in needs(addons, launcher, own) {
        match need {
            Need::Have => {}
            Need::Install(install) => out.installs.push(*install),
            Need::Blocked(blocker) => out.blockers.push(blocker),
        }
    }
    out
}

/// The plan for a host's full fingerprint, as the session reports it once it arrived.
pub fn plan_for(host: &HostPrint, own: &Own) -> Plan {
    plan(
        &host.addons,
        Some(host.launcher.as_str()).filter(|l| !l.is_empty()),
        HostGame {
            exe_sha256: Some(host.exe_sha256.as_str()).filter(|g| !g.is_empty()),
            version: host.game_version.as_deref(),
        },
        host.dlc,
        own,
    )
}

/// How far [`install_all`] has got: `done` of `total` addons, `bytes` of `total_bytes`, and the addon it is
/// at, with the version of the player's copy it replaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub name: String,
    pub version: String,
    pub replacing: Option<String>,
}

/// Installs one listed version, reporting the bytes downloaded so far.
pub type Installer<'a> = dyn Fn(&Addon, &catalog::Version, &dyn Fn(u64)) -> Result<(), String> + 'a;

/// Installs `plan`'s addons one by one with `install`, each only while `latest` (the Addon Browser's listing
/// now) still lists the host's copy. The error names the addon and what went wrong.
pub fn install_all(
    plan: &Plan,
    latest: &[Addon],
    launcher: &semver::Version,
    install: &Installer<'_>,
    progress: &dyn Fn(&Progress),
) -> Result<(), String> {
    let mut now = Progress {
        total: plan.installs.len(),
        total_bytes: plan.bytes(),
        ..Progress::default()
    };
    for one in &plan.installs {
        now.name = one.addon.name.clone();
        now.version = one.version.version.to_string();
        now.replacing = one.replaces.as_ref().map(ToString::to_string);
        progress(&now);
        let wanted = HostAddon {
            id: one.addon.id.clone(),
            name: one.addon.name.clone(),
            version: one.version.version.clone(),
            registry: Some(one.version.sha256.clone()),
            order: None,
            builtin: false,
        };
        let listed = listing(latest, &wanted, launcher).map_err(|why| match why {
            Unlisted::OtherCopy => format!(
                "the Addon Browser's {} {} is no longer the host's copy",
                one.addon.name, one.version.version
            ),
            Unlisted::NotListed | Unlisted::NeedsLauncher => format!(
                "the Addon Browser no longer lists {} {}",
                one.addon.name, one.version.version
            ),
        })?;
        let (addon, version) = listed;
        let before = now.bytes;
        let report = |n: u64| {
            let mut at = now.clone();
            at.bytes = before + n.min(version.size);
            progress(&at);
        };
        install(addon, version, &report)
            .map_err(|e| format!("{} {}: {e}", one.addon.name, one.version.version))?;
        now.bytes = before + version.size;
        now.done += 1;
        progress(&now);
    }
    Ok(())
}

/// Installs a listed version from the Addon Browser the way its page does, with what it requires, into no
/// profile in particular: a new addon goes into every profile, off. The download is checked against the
/// listing's SHA-256.
pub fn install_listed(
    store: &Store,
    catalog: &[Addon],
    addon: &Addon,
    v: &catalog::Version,
    bytes: &dyn Fn(u64),
) -> Result<(), String> {
    catalog::requires::install(store, catalog, addon, v, &|e| {
        if let Event::Stage(Stage::Downloading(n)) = e {
            bytes(n);
        }
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// The room profile for the host's addons: each of `addons` that `installed` has at the host's version on,
/// in the host's load order, with the host's option values; `cosmetic` (this player's own cosmetic addons)
/// on after them; anything else off. An addon whose requirements aren't on stays off.
pub fn room_profile(
    addons: &[HostAddon],
    options: &BTreeMap<String, BTreeMap<String, Value>>,
    cosmetic: &[String],
    installed: &BTreeMap<String, AddonManifest>,
    kept: Option<&Profile>,
) -> Profile {
    let mut ranked: Vec<&HostAddon> = addons
        .iter()
        .filter(|h| h.id != CORE_ID)
        .filter(|h| installed.get(&h.id).is_some_and(|m| m.version == h.version))
        .collect();
    ranked.sort_by(|a, b| match (a.order, b.order) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
    let mut on: Vec<String> = ranked.iter().map(|h| h.id.clone()).collect();
    for id in cosmetic {
        let shown = installed
            .get(id)
            .is_some_and(|m| m.multiplayer() == Multiplayer::Cosmetic);
        if shown && !on.contains(id) && id != CORE_ID {
            on.push(id.clone());
        }
    }
    loop {
        let set: BTreeSet<&str> = on.iter().map(String::as_str).chain([CORE_ID]).collect();
        let lacking: Vec<String> = on
            .iter()
            .filter(|id| {
                installed[id.as_str()]
                    .requires
                    .iter()
                    .any(|r| !set.contains(r.id.as_str()))
            })
            .cloned()
            .collect();
        if lacking.is_empty() {
            break;
        }
        on.retain(|id| !lacking.contains(id));
    }
    let mut p = Profile::new(ROOM_PROFILE);
    if let Some(kept) = kept {
        p.applied = kept.applied.clone();
        p.state = kept.state.clone();
        p.shader = kept.shader.clone();
    }
    for id in &on {
        p.set_enabled(id, true).ok();
    }
    for m in installed
        .values()
        .filter(|m| m.id != CORE_ID && !m.is_builtin() && m.id != crate::updates::managed::ID)
    {
        p.add_addon(&m.id).ok();
    }
    p.options = options
        .iter()
        .filter(|(id, _)| on.contains(id))
        .map(|(id, values)| (id.clone(), values.clone()))
        .collect();
    p
}

/// Writes the room profile for the host's addons ([`room_profile`]) with this player's cosmetic addons from
/// `own_profile` when that profile reads, and returns it as the store reads it back.
pub fn write_room_profile(
    store: &Store,
    addons: &[HostAddon],
    options: &BTreeMap<String, BTreeMap<String, Value>>,
    own_profile: &str,
) -> crate::Result<Profile> {
    let installed = store.addons()?;
    let cosmetic: Vec<String> = store
        .profile(own_profile)
        .map(|p| p.enabled().map(str::to_owned).collect())
        .unwrap_or_default();
    let kept = store.profile(ROOM_PROFILE).ok();
    let p = room_profile(addons, options, &cosmetic, &installed, kept.as_ref());
    store.save_profile(&p)?;
    store.profile(ROOM_PROFILE)
}

#[cfg(test)]
mod tests;
