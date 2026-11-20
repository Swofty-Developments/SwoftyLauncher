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

    /// Why these maps are not safe to apply, if they are not: an id that is both an earlier id and a
    /// target (a chain would map a row twice), or a target equal to an earlier id of another entry.
    pub fn refusal(&self) -> Option<String> {
        for (table, map) in [
            ("teams", &self.teams),
            ("vehicles", &self.vehicles),
            ("drivers", &self.drivers),
        ] {
            if let Some((from, to)) = map.iter().find(|(_, to)| map.contains_key(to)) {
                return Some(format!(
                    "the {table} aliases chain: {from} maps to {to}, which is itself an earlier id"
                ));
            }
        }
        None
    }

    /// A refusal for earlier ids that are rows in the build (`live` holds the build's ids per table), which
    /// would map a live row away.
    pub fn refusal_with_live(
        &self,
        live: &BTreeMap<&'static str, std::collections::BTreeSet<u16>>,
    ) -> Option<String> {
        if let Some(r) = self.refusal() {
            return Some(r);
        }
        for (table, map) in [
            ("teams", &self.teams),
            ("vehicles", &self.vehicles),
            ("drivers", &self.drivers),
        ] {
            let Some(rows) = live.get(table) else { continue };
            if let Some(id) = map.keys().find(|id| rows.contains(id)) {
                return Some(format!(
                    "the {table} alias for earlier id {id} names an id that is a row in this build"
                ));
            }
        }
        None
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

const TEAM_BUFFERS: &[&str] = &["packed_other_team_stat_ids"];

const DRIVER_BUFFERS: &[&str] = &["packed_driver_stat_ids", "packed_other_driver_stat_ids"];

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

fn mapped_buffer(value: &mut Value, map: &BTreeMap<u16, u16>) -> usize {
    let Value::Bffr(bytes) = value else {
        return 0;
    };
    if bytes.0.len() % 2 != 0 {
        return 0;
    }
    let mut n = 0;
    for lane in bytes.0.chunks_exact_mut(2) {
        let id = u16::from_le_bytes([lane[0], lane[1]]);
        if let Some(&to) = map.get(&id) {
            lane.copy_from_slice(&to.to_le_bytes());
            n += 1;
        }
    }
    n
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
            if let Some(k) = key {
                if TEAM_BUFFERS.contains(&k) {
                    out.teams += mapped_buffer(leaf, &maps.teams);
                } else if DRIVER_BUFFERS.contains(&k) {
                    out.drivers += mapped_buffer(leaf, &maps.drivers);
                }
            }
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

/// The slot id a `<mode>_SaveSlotGameModeComponent` names as the active career, whose body is the one
/// `gm_<mode>` container the profile save holds.
pub fn active_slot(slot_component: &Bag) -> Option<u64> {
    match slot_component.get("data")? {
        Value::Bag(data) => match data.get("active_slot_id")? {
            Value::U64(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// What mapping the slot component's slots would change, per slot id, and the slots with nothing to change
/// left out. The slots are the careers' own records: a slot that holds no earlier team id is in the current
/// numbering and untouched, since its other earlier ids are live rows of this build.
pub fn remap_slots(slot_component: &mut Bag, maps: &IdMaps) -> Vec<(u64, Remapped)> {
    remap_slots_gated(slot_component, maps).0
}

/// [`remap_slots`], with the ids of the slots that hold earlier driver or vehicle ids but no earlier team id.
pub fn remap_slots_gated(slot_component: &mut Bag, maps: &IdMaps) -> (Vec<(u64, Remapped)>, Vec<u64>) {
    let mut out = Vec::new();
    let mut held = Vec::new();
    let Some(Value::Bag(data)) = slot_component
        .entries
        .iter_mut()
        .find(|e| e.key.0 == b"data")
        .map(|e| &mut e.value)
    else {
        return (out, held);
    };
    let Some(Value::List(slots)) = data
        .entries
        .iter_mut()
        .find(|e| e.key.0 == b"slots")
        .map(|e| &mut e.value)
    else {
        return (out, held);
    };
    for slot in &mut slots.items {
        let Value::Bag(bag) = slot else { continue };
        let id = match bag.get("slot_id") {
            Some(Value::U64(id)) => *id,
            _ => continue,
        };
        let mut mapped = bag.clone();
        let done = remap_container(&mut mapped, maps);
        if done.teams > 0 {
            *bag = mapped;
            out.push((id, done));
        } else if done.total() > 0 {
            held.push(id);
        }
    }
    (out, held)
}

pub fn remap_root(root: &mut Bag, maps: &IdMaps) -> Remapped {
    let mut out = Remapped::default();
    if maps.is_empty() {
        return out;
    }
    for (name, bag) in save_containers_mut(root) {
        if is_career_container(&name) {
            out.add(remap_container(bag, maps));
        }
    }
    out
}

fn containers_of(save: &Bag) -> Option<&Bag> {
    match save.get("containers")? {
        Value::Bag(c) => Some(c),
        _ => None,
    }
}

/// The containers of a decoded profile save stream, by name: `save_game.containers.<name>`.
pub fn save_containers(root: &Bag) -> impl Iterator<Item = (String, &Bag)> {
    root.entries
        .iter()
        .filter_map(|save| match &save.value {
            Value::Bag(b) => containers_of(b),
            _ => None,
        })
        .flat_map(|c| c.entries.iter())
        .filter_map(|e| match &e.value {
            Value::Bag(b) => Some((String::from_utf8_lossy(&e.key.0).into_owned(), b)),
            _ => None,
        })
}

/// [`save_containers`], mutable.
pub fn save_containers_mut(root: &mut Bag) -> impl Iterator<Item = (String, &mut Bag)> {
    root.entries
        .iter_mut()
        .filter_map(|save| match &mut save.value {
            Value::Bag(b) => b.entries.iter_mut().find(|e| e.key.0 == b"containers"),
            _ => None,
        })
        .filter_map(|e| match &mut e.value {
            Value::Bag(c) => Some(c),
            _ => None,
        })
        .flat_map(|c| c.entries.iter_mut())
        .filter_map(|e| match &mut e.value {
            Value::Bag(b) => Some((String::from_utf8_lossy(&e.key.0).into_owned(), b)),
            _ => None,
        })
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
