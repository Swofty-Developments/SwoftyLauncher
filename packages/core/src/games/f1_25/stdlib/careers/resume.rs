//! What each career needs, as the in-game Resume Career list reads it from the session settings.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::rows::{PLACEHOLDER_IDS, SAVE_IDS};
use super::{Career, OWN_WHY, is_guess};
use crate::manifest::AddonManifest;
use crate::stdlib::range::Range;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumeNeed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub on: bool,
    pub at_version: bool,
}

/// The team, car and driver rows the database a career will load into holds, by stat id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowSet {
    pub teams: BTreeSet<i64>,
    pub vehicles: BTreeSet<i64>,
    pub drivers: BTreeSet<i64>,
    /// The build's id aliases (`swofty/id_aliases.json`): the ids rows had before, by `teams`, `vehicles`
    /// and `drivers`, mapped to the ids they have now.
    pub aliases: crate::row_ids::IdAliases,
}

impl RowSet {
    fn of(&self, field: &str) -> (&BTreeSet<i64>, &'static str) {
        match field {
            "team_raw" => (&self.teams, "teams"),
            "vehicle_raw" => (&self.vehicles, "vehicles"),
            _ => (&self.drivers, "drivers"),
        }
    }

    /// The id a save's `id` of `field` (`team_raw`, `vehicle_raw`, `driver_raw`) names in the build: itself
    /// when a row has it, else the id its alias gives when a row has that. `None` when no row does.
    pub fn resolve(&self, field: &str, id: i64) -> Option<i64> {
        let (rows, table) = self.of(field);
        if rows.contains(&id) {
            return Some(id);
        }
        let now = *self.aliases.tables.get(table)?.get(&id)?;
        rows.contains(&now).then_some(now)
    }
}

/// Rows a career's save slot names that the database it would load into lacks: the addon that provided them
/// when the career was made (its name, empty when none is recorded) and which of team, car and driver.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MissingRows {
    pub addon: String,
    pub tables: Vec<String>,
}

impl MissingRows {
    pub fn text(&self) -> String {
        match self.addon.is_empty() {
            false => format!("Missing rows from {}", self.addon),
            true => format!("Missing rows: {}", self.tables.join(", ")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CareerNeeds {
    pub key: String,
    pub career: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode_label: String,
    pub addons: Vec<ResumeNeed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_rows: Option<MissingRows>,
    /// The name of the addon that gives the career's mode when that addon is off or not installed: the game has
    /// no such mode to load the career into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_mode: Option<String>,
}

const STOCK_MODES: [&str; 3] = ["", "driver_career", "my_team"];

fn addon_name(id: &str, installed: &BTreeMap<String, AddonManifest>) -> String {
    installed.get(id).map_or(id.to_owned(), |m| m.name.clone())
}

/// The addon that owns the career's mode: the one recorded with a `career content` or `its career mode`
/// reason, else the first addon the career needs.
fn mode_owner(c: &Career) -> Option<&super::CareerAddon> {
    let owns = |a: &&super::CareerAddon| {
        a.why
            .as_deref()
            .is_some_and(|w| w.starts_with("career content") || w.starts_with("its career mode"))
    };
    c.addons
        .iter()
        .filter(|a| !is_guess(a))
        .find(owns)
        .or_else(|| c.addons.iter().find(|a| !is_guess(a)))
}

/// The mode suffix an addon's career content gives (`progressive` for `driver_career_progressive` and
/// `my_team_career_progressive`), by the addon that owns it, from the careers of every profile in the index.
pub fn mode_owners(list: &[Career]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for c in list {
        let Some(owner) = mode_owner(c).filter(|a| a.why.is_some()) else {
            continue;
        };
        let why = owner.why.as_deref().unwrap_or("");
        let suffix = match why.strip_prefix("career content f1_career_") {
            Some(group) => Some(group.to_owned()),
            None if why.starts_with("its career mode") => c
                .career
                .split(':')
                .next()
                .and_then(mode_suffix)
                .map(str::to_owned),
            None => None,
        };
        if let Some(suffix) = suffix.filter(|s| !s.is_empty() && !is_stock_suffix(s)) {
            out.entry(suffix).or_insert_with(|| owner.id.clone());
        }
    }
    out
}

/// What follows `driver_career_` or `my_team_career_` in a career mode's name.
pub fn mode_suffix(mode: &str) -> Option<&str> {
    mode.strip_prefix("driver_career_")
        .or_else(|| mode.strip_prefix("my_team_career_"))
}

fn is_stock_suffix(s: &str) -> bool {
    matches!(s, "25" | "26" | "2025" | "2026")
}

/// Marks each career of `saved` (by save slot key and mode) whose mode an addon of `owners` gives while that addon is
/// not in the load order `on`, adding an entry to `needs` for a career the index has none for.
pub fn mark_missing_modes(
    needs: &mut Vec<CareerNeeds>,
    saved: &[(String, String)],
    owners: &BTreeMap<String, String>,
    on: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) {
    for (key, mode) in saved {
        let Some(owner) = mode_suffix(mode).and_then(|s| owners.get(s)) else {
            continue;
        };
        if on.contains(owner) {
            continue;
        }
        let name = addon_name(owner, installed);
        match needs.iter_mut().find(|n| &n.key == key) {
            Some(n) => n.missing_mode = Some(name),
            None => needs.push(CareerNeeds {
                key: key.clone(),
                mode_label: name.clone(),
                missing_mode: Some(name),
                ..CareerNeeds::default()
            }),
        }
    }
}

/// Whether the career is one of an addon's own modes: its mode names one, or it carries addon content.
fn addon_mode(c: &Career) -> bool {
    !STOCK_MODES.contains(&c.field("mode").unwrap_or("")) || c.field("content").is_some()
}

/// What the in game list calls a career's mode: nothing for the two stock modes (the core names them from the
/// save), else the name of the addon that owns the mode.
fn mode_label(c: &Career, installed: &BTreeMap<String, AddonManifest>) -> String {
    if !addon_mode(c) {
        return String::new();
    }
    mode_owner(c)
        .map(|a| addon_name(&a.id, installed))
        .unwrap_or_default()
}

/// The rows of the career's save slot that `rows` lacks. Ids from the placeholder up are the game's own
/// (no team, my team) and are never checked.
fn lacked(c: &Career, rows: &RowSet) -> Vec<String> {
    SAVE_IDS
        .iter()
        .filter_map(|(field, table)| {
            let id = c.extra.get(*field).and_then(serde_json::Value::as_i64)?;
            let named = (0..PLACEHOLDER_IDS).contains(&id) && rows.resolve(field, id).is_none();
            named.then(|| table.rsplit('.').next().unwrap_or(table).to_owned())
        })
        .collect()
}

fn missing_rows(
    c: &Career,
    rows: Option<&RowSet>,
    installed: &BTreeMap<String, AddonManifest>,
) -> Option<MissingRows> {
    let tables = lacked(c, rows?);
    if tables.is_empty() {
        return None;
    }
    let owner = c
        .addons
        .iter()
        .find(|a| a.why.as_deref() == Some(OWN_WHY))
        .or_else(|| mode_owner(c).filter(|_| addon_mode(c)))
        .map(|a| addon_name(&a.id, installed))
        .unwrap_or_default();
    Some(MissingRows { addon: owner, tables })
}

fn at_version(a: &super::CareerAddon, installed: &AddonManifest) -> bool {
    match a.need.as_deref().and_then(|n| Range::parse(n).ok()) {
        Some(range) => range.matches(&installed.version),
        None => a
            .version
            .as_deref()
            .is_none_or(|made_with| made_with == installed.version.to_string()),
    }
}

/// One entry per career the save slots back, keyed by the save slot key the core sees; the addons are the
/// ones the career needs, each with whether profile `order` has it on and at the version the career was
/// made with. An addon recorded only because it was on is not a need. With `rows`, the database the career
/// would load into, a career whose team, car or driver row it lacks says so in `missing_rows`.
pub fn resume_needs(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
    rows: Option<&RowSet>,
) -> Vec<CareerNeeds> {
    list.iter()
        .filter(|c| !c.is_gone() && c.in_slots())
        .filter_map(|c| {
            let key = c.field("slot")?.to_owned();
            let addons = c
                .addons
                .iter()
                .filter(|a| !is_guess(a))
                .map(|a| match installed.get(&a.id) {
                    None => ResumeNeed {
                        id: a.id.clone(),
                        name: a.id.clone(),
                        version: a.version.clone().unwrap_or_default(),
                        on: false,
                        at_version: false,
                    },
                    Some(m) => ResumeNeed {
                        id: a.id.clone(),
                        name: m.name.clone(),
                        version: m.version.to_string(),
                        on: order.contains(&a.id),
                        at_version: at_version(a, m),
                    },
                })
                .collect();
            Some(CareerNeeds {
                key,
                career: c.career.clone(),
                mode_label: mode_label(c, installed),
                addons,
                missing_rows: missing_rows(c, rows, installed),
                missing_mode: None,
            })
        })
        .collect()
}

/// The careers of `list` the save of `profile` holds: the ones made under it, and a career with no profile
/// only where no career of `profile` has its slot key. Profiles sharing a source save share slot keys, so
/// a slot key alone does not name one profile's career.
pub fn of_profile(list: &[Career], profile: &str) -> Vec<Career> {
    let own = |c: &Career| c.profile.as_deref() == Some(profile);
    let taken: BTreeSet<&str> = list
        .iter()
        .filter(|c| own(c))
        .filter_map(|c| c.field("slot"))
        .collect();
    list.iter()
        .filter(|c| own(c) || (c.profile.is_none() && c.field("slot").is_none_or(|k| !taken.contains(k))))
        .cloned()
        .collect()
}

pub fn session_needs(
    data: &std::path::Path,
    profile: &str,
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Vec<CareerNeeds> {
    let list = of_profile(&super::read(&super::data_path(data)), profile);
    resume_needs(&list, order, installed, None)
}
