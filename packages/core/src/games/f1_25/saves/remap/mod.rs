//! Rewrites the stat ids a career save holds for rows whose ids moved (`swofty/id_aliases.json`), in the
//! decoded bag tree, so a career saved under earlier ids loads under the ids the rows have now.
//!
//! The game's entrant vector is the one holder the core's native hook maps at load. Every other holder
//! (slot info, standings, contracts, R&D, engine managers, wallets, pit crew, upgrades) is a plain typed
//! field the game reads back verbatim, so it is mapped here, by field name, in career containers only.

use std::collections::BTreeMap;

use super::bag::{Bag, Value};
use crate::games::f1_25::row_ids::IdAliases;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IdMaps {
    pub teams: BTreeMap<u16, u16>,
    pub vehicles: BTreeMap<u16, u16>,
    pub drivers: BTreeMap<u16, u16>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Remapped {
    pub teams: usize,
    pub vehicles: usize,
    pub drivers: usize,
}

impl Remapped {
    pub fn total(self) -> usize {
        self.teams + self.vehicles + self.drivers
    }

    fn add(&mut self, other: Remapped) {
        self.teams += other.teams;
        self.vehicles += other.vehicles;
        self.drivers += other.drivers;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Team,
    Vehicle,
    Driver,
}

fn narrow(table: Option<&BTreeMap<i64, i64>>) -> BTreeMap<u16, u16> {
    table
        .into_iter()
        .flatten()
        .filter_map(|(&from, &to)| Some((u16::try_from(from).ok()?, u16::try_from(to).ok()?)))
        .filter(|(from, to)| from != to)
        .collect()
}

impl IdMaps {
    pub fn from_aliases(aliases: &IdAliases) -> Self {
        IdMaps {
            teams: narrow(aliases.tables.get("teams")),
            vehicles: narrow(aliases.tables.get("vehicles")),
            drivers: narrow(aliases.tables.get("drivers")),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.teams.is_empty() && self.vehicles.is_empty() && self.drivers.is_empty()
    }

    fn map(&self, kind: Kind) -> &BTreeMap<u16, u16> {
        match kind {
            Kind::Team => &self.teams,
            Kind::Vehicle => &self.vehicles,
            Kind::Driver => &self.drivers,
        }
    }
}

const TEAM_KEYS: &[&str] = &[
    "info_team",
    "teamStatId",
    "team_stat_id",
    "teamId",
    "team_id",
    "packed_team_stat_id",
    "packedTeamStatId",
    "linked_team_stat_id",
];

const VEHICLE_KEYS: &[&str] = &["info_vehicle", "vehicle_stat_id"];

const DRIVER_KEYS: &[&str] = &[
    "info_driver",
    "driverStatId",
    "driver_stat_id",
    "driver_stat_Id",
    "driver_statId",
    "driverId",
    "driver_id",
    "packed_driver_stat_id",
    "packedDriverStatId",
    "previous_player",
];

const DRIVER_STAT_LISTS: &[&str] = &[
    "press_interviews_harsh_answers_given",
    "race_penalty_history",
    "laps_driven_until_end_weekend backup",
    "laps_driven_until_end_career backup",
];

fn numbered<'a>(key: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = key.strip_prefix(prefix)?;
    (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())).then_some(rest)
}

fn kind_of(key: &str, list: Option<&str>) -> Option<Kind> {
    if TEAM_KEYS.contains(&key) || numbered(key, "info_standings_team_").is_some() {
        Some(Kind::Team)
    } else if VEHICLE_KEYS.contains(&key) {
        Some(Kind::Vehicle)
    } else if DRIVER_KEYS.contains(&key)
        || numbered(key, "info_standings_driver_").is_some()
        || (key == "statId" && list.is_some_and(|l| DRIVER_STAT_LISTS.contains(&l)))
    {
        Some(Kind::Driver)
    } else {
        None
    }
}

fn mapped(value: &mut Value, map: &BTreeMap<u16, u16>) -> bool {
    match value {
        Value::U16(v) => match map.get(v) {
            Some(&to) => {
                *v = to;
                true
            }
            None => false,
        },
        Value::I32(v) => match u16::try_from(*v).ok().and_then(|id| map.get(&id)) {
            Some(&to) => {
                *v = i32::from(to);
                true
            }
            None => false,
        },
        _ => false,
    }
}

fn walk(value: &mut Value, key: Option<&str>, list: Option<&str>, maps: &IdMaps, out: &mut Remapped) {
    match value {
        Value::Bag(bag) => {
            for entry in &mut bag.entries {
                let name = String::from_utf8_lossy(&entry.key.0).into_owned();
                walk(&mut entry.value, Some(&name), list, maps, out);
            }
        }
        Value::List(items) => {
            let name = key.map(str::to_owned);
            for item in &mut items.items {
                walk(item, None, name.as_deref(), maps, out);
            }
        }
        leaf => {
            let Some(kind) = key.and_then(|k| kind_of(k, list)) else {
                return;
            };
            if mapped(leaf, maps.map(kind)) {
                match kind {
                    Kind::Team => out.teams += 1,
                    Kind::Vehicle => out.vehicles += 1,
                    Kind::Driver => out.drivers += 1,
                }
            }
        }
    }
}

pub fn is_career_container(name: &str) -> bool {
    name.starts_with("gm_driver_career")
        || (name.starts_with("driver_career") && name.ends_with("_SaveSlotGameModeComponent"))
}

pub fn remap_container(container: &mut Bag, maps: &IdMaps) -> Remapped {
    let mut out = Remapped::default();
    for entry in &mut container.entries {
        let name = String::from_utf8_lossy(&entry.key.0).into_owned();
        walk(&mut entry.value, Some(&name), None, maps, &mut out);
    }
    out
}

pub fn remap_root(root: &mut Bag, maps: &IdMaps) -> Remapped {
    let mut out = Remapped::default();
    if maps.is_empty() {
        return out;
    }
    for save in &mut root.entries {
        let Value::Bag(containers) = &mut save.value else {
            continue;
        };
        for child in &mut containers.entries {
            let name = String::from_utf8_lossy(&child.key.0).into_owned();
            if !is_career_container(&name) {
                continue;
            }
            if let Value::Bag(bag) = &mut child.value {
                out.add(remap_container(bag, maps));
            }
        }
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum RemapError {
    #[error("the save stream does not decode: {0}")]
    Decode(#[from] super::bag::DecodeError),
    #[error("the remapped save stream does not encode: {0}")]
    Encode(#[from] super::bag::EncodeError),
}

pub fn remap_stream(stream: &[u8], maps: &IdMaps) -> Result<Option<(Vec<u8>, Remapped)>, RemapError> {
    if maps.is_empty() {
        return Ok(None);
    }
    let mut root = super::bag::decode(stream)?;
    let done = remap_root(&mut root, maps);
    if done.total() == 0 {
        return Ok(None);
    }
    Ok(Some((super::bag::encode(&root)?, done)))
}

#[cfg(test)]
mod tests;
