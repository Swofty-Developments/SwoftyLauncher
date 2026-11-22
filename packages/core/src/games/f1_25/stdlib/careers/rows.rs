//! The database rows a career's save names: the ids above the stock rows and the addons that own them now.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::record::prove;
use super::{Career, OWN_WHY, is_guess};
use crate::manifest::AddonManifest;
use crate::row_ids::IdOwners;

pub(super) const SAVE_IDS: [(&str, &str); 3] = [
    ("team_raw", "entrants.teams.team"),
    ("vehicle_raw", "entrants.vehicles.vehicle"),
    ("driver_raw", "entrants.drivers.driver"),
];

/// The ids from this value up are the game's placeholders (no team, my team), never rows.
pub(super) const PLACEHOLDER_IDS: i64 = 16383;

/// The ids a career's save entry holds above the stock database's highest (`stock_max`, by table,
/// see `row_ids::stock_max_ids`), as `(table, id)`: rows an addon added. A table with no stock
/// maximum yields none.
pub(super) fn added_ids(c: &Career, stock_max: &BTreeMap<String, i64>) -> Vec<(&'static str, i64)> {
    SAVE_IDS
        .iter()
        .filter_map(|(field, table)| {
            let id = c.extra.get(*field).and_then(Value::as_i64)?;
            let top = stock_max.get(*table)?;
            (id > *top && id < PLACEHOLDER_IDS).then_some((*table, id))
        })
        .collect()
}

pub(super) fn records_rows(c: &Career) -> bool {
    c.addons.iter().any(|a| a.why.as_deref() == Some(OWN_WHY))
}

/// The stock maximum ids `read` gives, read only when a career of `list` needs them: one that is not
/// deleted and holds a team, vehicle or driver id that could be an addon's. Empty otherwise.
pub fn stock_max_for(list: &[Career], read: impl FnOnce() -> BTreeMap<String, i64>) -> BTreeMap<String, i64> {
    let could_be_added = |c: &Career| {
        SAVE_IDS
            .iter()
            .filter_map(|(field, _)| c.extra.get(*field).and_then(Value::as_i64))
            .any(|id| id <= PLACEHOLDER_IDS)
    };
    if list.iter().any(|c| !c.is_gone() && could_be_added(c)) {
        read()
    } else {
        BTreeMap::new()
    }
}

/// The addons that own `id` of `table` for career `c` now (`owners`, see `row_ids::id_owners`). An
/// addon that is not installed is no owner when an installed one is, and with the load order `on` an
/// addon that is off is no owner when one that is on is: the id belongs to the rows in the game.
pub(super) fn owners_now<'a>(
    c: &Career,
    (table, id): (&str, i64),
    owners: &'a IdOwners,
    installed: &BTreeMap<String, AddonManifest>,
    on: Option<&[String]>,
) -> Vec<&'a String> {
    let mut found: Vec<&String> = owners.of(&c.career, table, id).into_iter().flatten().collect();
    if found.iter().any(|a| installed.contains_key(*a)) {
        found.retain(|a| installed.contains_key(*a));
    }
    if let Some(on) = on
        && found.iter().any(|a| on.contains(a))
    {
        found.retain(|a| on.contains(a));
    }
    found
}

/// The careers of the save being played and its load order, for [`settle_rows`].
pub type Played<'a> = (&'a BTreeSet<String>, &'a [String]);

/// Gives each career the addons that own the team, vehicle or driver row its save slot names: an id
/// above the stock rows (`stock_max`, see `row_ids::stock_max_ids`) that the install's ids give an
/// addon ([`owners_now`], with the load order of `played` for its careers). An addon recorded that way
/// before and no longer the owner of one of the career's rows is taken off. With no `stock_max`
/// nothing can be told apart from a stock row, so nothing changes. Returns whether any career changed.
pub fn settle_rows(
    list: &mut [Career],
    owners: &IdOwners,
    stock_max: &BTreeMap<String, i64>,
    installed: &BTreeMap<String, AddonManifest>,
    played: Option<Played>,
) -> bool {
    if stock_max.is_empty() {
        return false;
    }
    let mut changed = false;
    for c in list.iter_mut().filter(|c| !c.is_gone()) {
        let on = played
            .filter(|(careers, _)| careers.contains(&c.career))
            .map(|(_, on)| on);
        let found: BTreeSet<String> = added_ids(c, stock_max)
            .into_iter()
            .flat_map(|at| owners_now(c, at, owners, installed, on))
            .cloned()
            .collect();
        let before = c.addons.len();
        c.addons
            .retain(|a| a.why.as_deref() != Some(OWN_WHY) || found.contains(&a.id));
        changed |= c.addons.len() != before;
        for addon in &found {
            match c.addons.iter_mut().find(|a| &a.id == addon) {
                Some(a) if a.why.as_deref() == Some(OWN_WHY) => {}
                Some(a) if is_guess(a) => {
                    a.why = Some(OWN_WHY.to_owned());
                    changed = true;
                }
                Some(_) => {}
                None => changed |= prove(c, addon, OWN_WHY, installed),
            }
        }
    }
    changed
}
