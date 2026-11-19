//! What a session records in the career index: the addons that own each career's team, car or driver, and its
//! My Team car.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use super::index_file::{data_path, read};
use super::play::in_save_of;
use super::rows::{settle_rows, stock_max_for};
use super::{CAR_WHY, Career, CareerAddon, is_guess};
use crate::manifest::AddonManifest;

/// The save slot's vehicle id of a My Team career is this flag and the id of its custom vehicle.
pub(super) const CUSTOM_VEHICLE: i64 = 0x4000;

pub(super) const VEHICLES: &str = "entrants.vehicles.vehicle";

/// The My Team cars the core found without a vehicle row when it loaded the save, from the
/// `[myteam]` lines of its loader.log, as `(custom vehicle id, vehicle label)`.
pub fn lost_cars(loader_log: &str) -> BTreeSet<(i64, String)> {
    let between = |l: &str, from: &str, to: &str| -> Option<String> {
        let rest = l.split_once(from)?.1;
        Some(rest.split_once(to)?.0.to_owned())
    };
    loader_log
        .lines()
        .filter_map(|l| {
            let (id, label) = if l.contains("[myteam] keep: My Team car (custom vehicle ") {
                (between(l, "(custom vehicle ", ")")?, between(l, " on '", "'")?)
            } else if l.contains("[myteam] save: My Team vehicle base ")
                && l.contains("is not in the game data")
            {
                (between(l, "(stat id ", ")")?, between(l, "uses vehicle '", "'")?)
            } else {
                return None;
            };
            Some((id.trim().parse().ok()?, label))
        })
        .collect()
}

/// Records on each My Team career of `in_save` whose car is one of `lost` ([`lost_cars`]) the addons
/// that own the car's vehicle row (`owners`, by label: `row_ids::label_owners`). Returns whether any
/// career changed.
pub fn record_cars(
    list: &mut [Career],
    in_save: &dyn Fn(&Career) -> bool,
    lost: &BTreeSet<(i64, String)>,
    owners: &BTreeMap<String, BTreeSet<String>>,
    installed: &BTreeMap<String, AddonManifest>,
) -> bool {
    let mut changed = false;
    for c in list.iter_mut().filter(|c| !c.is_gone() && c.in_slots()) {
        if !in_save(c) {
            continue;
        }
        let Some(vehicle) = c.extra.get("vehicle_raw").and_then(Value::as_i64) else {
            continue;
        };
        for (id, label) in lost {
            if vehicle != CUSTOM_VEHICLE | *id {
                continue;
            }
            for addon in owners.get(label).into_iter().flatten() {
                changed |= prove(c, addon, CAR_WHY, installed);
            }
        }
    }
    changed
}

pub(super) fn prove(
    c: &mut Career,
    addon: &str,
    why: &str,
    installed: &BTreeMap<String, AddonManifest>,
) -> bool {
    match c.addons.iter_mut().find(|a| a.id == addon) {
        Some(a) if is_guess(a) => {
            a.why = Some(why.to_owned());
            true
        }
        Some(_) => false,
        None => {
            c.addons.push(CareerAddon {
                id: addon.to_owned(),
                version: installed.get(addon).map(|m| m.version.to_string()),
                need: None,
                why: Some(why.to_owned()),
            });
            true
        }
    }
}

/// After a session of `profile`: records in the data folder's career index the addons that own the
/// team, car or driver row of each career ([`settle_rows`], with the stock maximum ids `stock_max`
/// gives) and the addons whose My Team cars the session's loader.log names as lost ([`record_cars`]).
pub fn record_session(
    data: &Path,
    profile: &str,
    loader_log: &str,
    stock_max: impl FnOnce() -> BTreeMap<String, i64>,
) -> crate::Result<()> {
    let mut list = read(&data_path(data));
    if list.is_empty() {
        return Ok(());
    }
    let installed = crate::store::Store::new(data).addons().unwrap_or_default();
    let ids: BTreeSet<String> = in_save_of(&list, data, profile)
        .into_iter()
        .map(|c| c.career.clone())
        .collect();
    let in_save = |c: &Career| ids.contains(&c.career);
    let stock_max = stock_max_for(&list, stock_max);
    let store = crate::store::Store::new(data);
    let on = store
        .profile(profile)
        .ok()
        .and_then(|p| crate::profile::resolver::resolve(&p, &installed).ok())
        .map(|plan| plan.order);
    let mut changed = settle_rows(
        &mut list,
        &crate::row_ids::id_owners(data),
        &stock_max,
        &installed,
        on.as_deref().map(|on| (&ids, on)),
    );
    let lost = lost_cars(loader_log);
    if !lost.is_empty() {
        changed |= record_cars(
            &mut list,
            &in_save,
            &lost,
            &crate::row_ids::label_owners(data, VEHICLES),
            &installed,
        );
    }
    if changed {
        crate::write_json_shared(
            &data_path(data),
            &serde_json::json!({ "version": 1, "careers": list }),
        )?;
    }
    Ok(())
}
