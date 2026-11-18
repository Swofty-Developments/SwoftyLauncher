//! The edits the enabled DB Editor addons make to the game database, in load order, and turning them all off.

use std::collections::{HashMap, HashSet};

use swofty_formats::gvdb::{Gvdb, Record};

use super::{GVDB_NAME, Package, is_package};
use crate::dbedit::{Catalog, Edits, Origin};
use crate::store::Store;
use crate::{Error, Result};

/// The edits that turn the stock database into the modded one in `bytes`, and
/// notes on what they change.
pub(super) fn database_edits(catalog: &Catalog, bytes: &[u8], label: &str) -> Result<(Edits, Vec<String>)> {
    let not_db = || Error::Other(format!("{label} is not an F1 25 game database ({GVDB_NAME})"));
    let modded = Gvdb::parse(bytes).map_err(|_| not_db())?;
    let mut by_key: HashMap<u64, &Record> = HashMap::with_capacity(modded.records.len());
    for r in &modded.records {
        by_key.entry(r.key).or_insert(r);
    }
    let stock: Vec<_> = (0..catalog.records() as u32)
        .map(|i| catalog.record(i))
        .filter(|r| {
            catalog
                .record_by_key(r.key)
                .is_some_and(|first| first.index == r.index)
        })
        .collect();
    if stock.iter().filter(|r| by_key.contains_key(&r.key)).count() * 2 < stock.len() {
        return Err(not_db());
    }
    let origin = Origin::Imported(label.to_owned());
    let text = |key: u64, value: &[u8], notes: &mut Vec<String>| {
        let v = String::from_utf8(value.to_vec()).ok();
        if v.is_none() {
            notes.push(format!("0x{key:016x}: skipped, the value is not text"));
        }
        v
    };
    let (mut edits, mut count_edits, mut notes) = (Vec::new(), Vec::new(), Vec::new());
    let (mut grown, mut resized, mut names) = (Vec::new(), Vec::new(), HashMap::new());
    let (mut changed, mut removed) = (0, 0);
    for s in &stock {
        let edit = match by_key.get(&s.key) {
            None => {
                removed += 1;
                Edits::imported(catalog, s.key, s.tag, None, None, &origin)
            }
            Some(r) if r.tag == s.tag && String::from_utf8_lossy(&r.value) == s.value => continue,
            Some(r) => {
                let Some(value) = text(s.key, &r.value, &mut notes) else {
                    continue;
                };
                changed += 1;
                let Some((container, word)) = catalog.instance_count(s.key) else {
                    edits.push(Edits::imported(catalog, s.key, r.tag, Some(value), None, &origin));
                    continue;
                };
                let table = catalog.table_path(container, word);
                match (s.value.trim().parse::<u32>(), value.trim().parse::<u32>()) {
                    (Ok(from), Ok(to)) if to > from => {
                        names.extend(catalog.row_keys(container, word, from..to));
                        grown.push(table);
                    }
                    _ => resized.push(table),
                }
                count_edits.push(Edits::imported(catalog, s.key, r.tag, Some(value), None, &origin));
                continue;
            }
        };
        edits.push(edit);
    }
    let mut seen = HashSet::new();
    for r in &modded.records {
        if catalog.record_by_key(r.key).is_some() || !seen.insert(r.key) {
            continue;
        }
        if let Some(value) = text(r.key, &r.value, &mut notes) {
            edits.push(Edits::imported(
                catalog,
                r.key,
                r.tag,
                Some(value),
                names.remove(&r.key),
                &origin,
            ));
        }
    }
    let added = edits.len() + count_edits.len() - changed - removed;
    // Row counts go last, so selectors resolve against the stock rows.
    edits.extend(count_edits);
    if edits.is_empty() {
        return Err(Error::Other(format!(
            "{label} is the same as this game's stock database"
        )));
    }
    let mut out = vec![format!(
        "{changed} changed, {added} added and {removed} removed key(s)"
    )];
    if !grown.is_empty() {
        out.push(format!(
            "adds rows to {}; another addon that adds rows to the same tables won't stack with this one",
            grown.join(", ")
        ));
    }
    if !resized.is_empty() {
        out.push(format!("changes the row count of {}", resized.join(", ")));
    }
    out.extend(notes);
    Ok((
        Edits {
            schema_version: 1,
            edits,
            rows: Vec::new(),
        },
        out,
    ))
}

/// Ids of the DB Editor addons enabled in `profile`, in load order.
pub fn enabled_packages(store: &Store, profile: &str) -> Result<Vec<String>> {
    let p = store.profile(profile)?;
    let order = match crate::profile::resolver::resolve(&p, &store.addons()?) {
        Ok(plan) => plan.order,
        Err(_) => p.enabled().map(str::to_owned).collect(),
    };
    Ok(order.into_iter().filter(|id| is_package(store, id)).collect())
}

/// key → (addon id, value) for each DB Editor addon enabled in `profile`, in
/// load order; a removed key has an empty value. Addons that don't read are left out.
pub fn enabled_changes(store: &Store, profile: &str) -> Result<HashMap<u64, Vec<(String, String)>>> {
    let mut out: HashMap<u64, Vec<(String, String)>> = HashMap::new();
    for id in enabled_packages(store, profile)? {
        let Ok(package) = Package::open(store, &id) else {
            continue;
        };
        for e in package.edits.edits {
            out.entry(e.key)
                .or_default()
                .push((id.clone(), e.value.unwrap_or_default()));
        }
    }
    Ok(out)
}

/// Turns off every DB Editor addon enabled in `profile`; returns how many.
pub fn turn_off_all(store: &Store, profile: &str) -> Result<usize> {
    if profile == crate::profile::VANILLA {
        return Err(Error::Other(format!(
            "{profile} stays stock and has no addons on"
        )));
    }
    let on = enabled_packages(store, profile)?;
    store.update_profile(profile, |p| {
        for id in &on {
            p.set_enabled(id, false)?;
        }
        Ok(on.len())
    })
}
