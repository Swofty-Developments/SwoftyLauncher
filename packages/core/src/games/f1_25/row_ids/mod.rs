//! The install's ids for the rows `add-row` gives an id of its own, in `$DATA/registry/row-ids.json`
//! next to the core's `ids.json` (docs/std/registry.md "Stat ids"). A key names an addon's row,
//! `<addon>/<table>.<row>/<label>`, and keeps its id at every build of every profile, whatever else
//! the build adds, since careers in the shared Steam save hold the ids.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ops::{IdFrom, is_unlock_ids, stat_id_field};
use crate::stdlib::reports;
use crate::{Result, io_err};

mod aliases;
mod overrides;
mod owners;
mod recover;
mod registry;
mod scoped;
mod seed;
mod store;
mod tracks;

#[cfg(test)]
mod tests;

pub use self::aliases::IdAliases;
pub use self::overrides::{Overrides, ROOM_STAT_IDS_FILE, lend, lent};
pub use self::owners::{
    IdOwners, Kept, id_owners, kept_rows, label_owners, registry_rows, save_owners, stock_max_ids,
};
pub use self::recover::Recovery;
use self::registry::{career_ids, registry_ids};
pub use self::scoped::Scoped;
use self::seed::Seed;
pub use self::seed::{LastBuild, last_build};
pub use self::tracks::{TRACK_STAT_MAX, track_row};

pub const FILE: &str = "row-ids.json";
const LOCK: &str = "row-ids.lock";
const VERSION: u32 = 2;
const ALIASES_VERSION: u32 = 1;

/// The core's registries whose `ids.json` ids are ids of these tables' rows.
const REGISTRY_TABLES: &[(&str, &str)] = &[
    ("vehicle", "entrants.vehicles.vehicle"),
    (
        "vehicle.package",
        "entrants.vehicles.vehicle_packages.vehicle_package",
    ),
    ("content.package", "entrants.entrant_lists.entrant_list"),
    ("content.package.scenario", "game_setup.scenarios.scenario"),
    ("team.identity", "entrants.teams.team"),
    ("driver.identity", "entrants.drivers.driver"),
    ("driver.head", "entrants.animation.driver_heads.driver_head"),
    // The core's livery unlock pool spans every unlock list ([`pool`]).
    ("livery.unlock", UNLOCK_POOL),
];

/// The name the core's livery unlock ids go under: the pool is one for all unlock lists.
const UNLOCK_POOL: &str = "unlockable_content.unlockable_items";

/// The name `ids.json`'s ids and careers of `table`'s rows go under: every unlock list's id space
/// (`<list>.unlockable_item`) shares the core's one livery unlock pool.
fn pool(table: &str) -> &str {
    if is_unlock_ids(table) { UNLOCK_POOL } else { table }
}

/// The core registries whose careers never hold ids: saves name vehicle packages by label, not by `stat_id`.
const UNHELD_REGISTRIES: &[&str] = &["vehicle.package"];

/// The tables saves hold ids of, and the name `swofty/id_aliases.json` gives each.
const ALIAS_TABLES: &[(&str, &str)] = &[
    ("entrants.teams.team", "teams"),
    ("entrants.vehicles.vehicle", "vehicles"),
    ("entrants.drivers.driver", "drivers"),
];

pub const ALIASES_FILE: &str = "id_aliases.json";

/// The environment variable the launcher sets for `swofty run` to the file of the host's ids for a room
/// launch ([`Overrides`]).
pub const ENV: &str = "SWOFTY_ROOM_ROW_IDS";

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    rows: BTreeMap<String, Entry>,
    /// The profiles whose last build before the store seeded it.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    seeded: BTreeSet<String>,
    /// The ids 1.6.10 and 1.6.11 gave from the rows' keys were put back once (see [`Store::recover`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    recovered: bool,
    /// Every id a key's row had before the one it keeps, so saves made under those ids still load.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    aliases: BTreeMap<String, BTreeSet<i64>>,
    /// Per profile, the ids and aliases that differ from the install-wide ones above: a row of the profile
    /// that a clash there moved. Another profile never reads them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    profiles: BTreeMap<String, ProfileIds>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ProfileIds {
    #[serde(default)]
    rows: BTreeMap<String, Entry>,
    #[serde(default)]
    aliases: BTreeMap<String, BTreeSet<i64>>,
}

impl ProfileIds {
    fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.aliases.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    id: i64,
    from: IdFrom,
}

pub fn path(data: &Path) -> PathBuf {
    reports::path(data, FILE)
}

/// Held while a build reads and writes `row-ids.json`; another build waits until it is dropped.
pub fn lock(data: &Path) -> Result<std::fs::File> {
    let dir = reports::dir(data);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(LOCK);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    file.lock().map_err(io_err(&path))?;
    Ok(file)
}

/// The kept ids, the ids the core's registry holds, and what this build claimed and changed.
#[derive(Default)]
pub struct Store {
    rows: BTreeMap<String, i64>,
    from: HashMap<String, IdFrom>,
    seeded: BTreeSet<String>,
    recovered: bool,
    aliases: BTreeMap<String, BTreeSet<i64>>,
    profile: String,
    base: BTreeMap<String, Entry>,
    base_ids: HashMap<String, HashMap<i64, String>>,
    base_aliases: BTreeMap<String, BTreeSet<i64>>,
    local: BTreeSet<String>,
    local_aliases: BTreeMap<String, BTreeSet<i64>>,
    others: BTreeMap<String, ProfileIds>,
    existed: bool,
    by_id: HashMap<String, BTreeMap<i64, String>>,
    pinned: HashMap<String, HashMap<i64, String>>,
    career_held: HashMap<String, HashSet<i64>>,
    seed: Seed,
    claimed: HashSet<String>,
    building: HashSet<String>,
    adds_to: HashSet<(String, String)>,
    added: usize,
    moved: usize,
    reused: usize,
    changed: bool,
    read_only: bool,
    lent: BTreeMap<String, i64>,
    lent_build: HashSet<String>,
}

fn addon_of(key: &str) -> &str {
    key.split('/').next().unwrap_or(key)
}

fn table_of(key: &str) -> &str {
    key.split('/').nth(1).unwrap_or_default()
}

fn row_name(key: &str) -> String {
    let (addon, rest) = key.split_once('/').unwrap_or(("", key));
    format!("{rest} of {addon}")
}
