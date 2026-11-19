use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::IdFrom;
use crate::{Loaded, Result};

use super::{Store, addon_of, pool, row_name, table_of};

/// What a room's host lends a joiner for one session's builds, so both copies' fingerprints match: the
/// ids the host gave the addons' rows (`rows`, by addon and then `<table>.<row>/<label>`, in place of the
/// ids the joiner's own `row-ids.json` keeps), the host's load order of the addons both have (`order`) and
/// the host's values of their options (`options`). None of it is written to `row-ids.json` (see
/// [`Store::lend`]) or the profile (see [`crate::profile::Profile::lend`]), and all of it is part of the
/// build's cache key.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    pub rows: BTreeMap<String, BTreeMap<String, i64>>,
    /// The shared addons that change game data, in the host's load order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// The host's value of each option that differs from the joiner's, by addon and option id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// The host's stat ids for the core's registry records of each shared addon, as
    /// `<registry>:<record>` to id. The core reads them from `swofty/room_stat_ids.json` for the
    /// session and keeps its own `ids.json` as it is. An addon with no ids here means the host's
    /// registry holds none for it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registry: BTreeMap<String, BTreeMap<String, i64>>,
}

/// The file in the game's `swofty` folder that carries a room's [`Overrides::registry`] to the core.
pub const ROOM_STAT_IDS_FILE: &str = "room_stat_ids.json";

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.rows.values().all(BTreeMap::is_empty)
            && self.order.is_empty()
            && self.options.is_empty()
            && self.registry.is_empty()
    }

    /// The registry ids of the addons in `order`, as `swofty/room_stat_ids.json` holds them: registry to
    /// record to id. `None` when there are none to lend.
    pub fn room_stat_ids(&self, order: &[String]) -> Option<serde_json::Value> {
        let mut ids: BTreeMap<&str, BTreeMap<&str, i64>> = BTreeMap::new();
        for (_, entries) in self.registry.iter().filter(|(a, _)| order.contains(a)) {
            for (key, id) in entries {
                if let Some((registry, record)) = key.split_once(':') {
                    ids.entry(registry).or_default().insert(record, *id);
                }
            }
        }
        (!ids.is_empty()).then(|| serde_json::json!({ "version": 1, "ids": ids }))
    }

    /// These overrides with `newer` laid over them: a row, option or order `newer` has wins.
    pub fn merged(mut self, newer: Overrides) -> Overrides {
        for (addon, rows) in newer.rows {
            self.rows.entry(addon).or_default().extend(rows);
        }
        for (addon, options) in newer.options {
            self.options.entry(addon).or_default().extend(options);
        }
        self.registry.extend(newer.registry);
        if !newer.order.is_empty() {
            self.order = newer.order;
        }
        self
    }

    /// How many option values `options` carries.
    pub fn option_values(&self) -> usize {
        self.options.values().map(BTreeMap::len).sum()
    }

    /// `current` with the addons of `order` that it holds put in the host's order, each in a place an
    /// addon of `order` held; the rest stay where they are.
    pub fn arranged(&self, current: &[String]) -> Vec<String> {
        let mut host = self.order.iter().filter(|id| current.contains(id));
        current
            .iter()
            .map(|id| match self.order.contains(id) {
                true => host.next().cloned().unwrap_or_else(|| id.clone()),
                false => id.clone(),
            })
            .collect()
    }

    /// The overrides for the addons in `order`, as `<addon>/<table>.<row>/<label>` keys.
    pub(super) fn keys(&self, order: &[String]) -> BTreeMap<String, i64> {
        order
            .iter()
            .filter_map(|addon| Some((addon, self.rows.get(addon)?)))
            .flat_map(|(addon, rows)| rows.iter().map(move |(row, id)| (format!("{addon}/{row}"), *id)))
            .collect()
    }

    /// What goes into the build's cache key for the addons in `order`; empty when none apply.
    pub fn stamp(&self, order: &[String]) -> Option<String> {
        let keys = self.keys(order);
        let ranked: Vec<&String> = self.order.iter().filter(|id| order.contains(id)).collect();
        let registry: BTreeMap<&String, &BTreeMap<String, i64>> = self
            .registry
            .iter()
            .filter(|(id, _)| order.contains(id))
            .collect();
        let options: BTreeMap<&String, &BTreeMap<String, serde_json::Value>> =
            self.options.iter().filter(|(id, _)| order.contains(id)).collect();
        let mut out = String::new();
        if !keys.is_empty() {
            out += &format!(
                "room row ids {};",
                serde_json::to_string(&keys).expect("serialisable")
            );
        }
        if !ranked.is_empty() || !options.is_empty() {
            out += &format!(
                "room settings {} {};",
                serde_json::to_string(&ranked).expect("serialisable"),
                serde_json::to_string(&options).expect("serialisable")
            );
        }
        if !registry.is_empty() {
            out += &format!(
                "room registry ids {};",
                serde_json::to_string(&registry).expect("serialisable")
            );
        }
        (!out.is_empty()).then_some(out)
    }

    pub fn read(path: &Path) -> Result<Self> {
        match crate::load_json::<Overrides>(path)? {
            Loaded::Found(o) => Ok(o),
            Loaded::Missing | Loaded::Damaged(_) => Ok(Self::default()),
        }
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        crate::write_json(path, self)
    }
}

#[cfg(not(test))]
static LENT: std::sync::Mutex<Option<Overrides>> = std::sync::Mutex::new(None);

// Tests run side by side in one process, so a test's lend stays on its own thread: a build in another
// test must never see it.
#[cfg(test)]
thread_local! {
    static LENT: std::cell::RefCell<Option<Overrides>> = const { std::cell::RefCell::new(None) };
}

/// Makes this process's builds use `overrides` (a room launch's), or none. The `swofty run` that starts
/// the game for a room calls it once; no other process does.
pub fn lend(overrides: Option<Overrides>) {
    let overrides = overrides.filter(|o| !o.is_empty());
    #[cfg(not(test))]
    {
        *LENT.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = overrides;
    }
    #[cfg(test)]
    LENT.with(|lent| *lent.borrow_mut() = overrides);
}

pub fn lent() -> Option<Overrides> {
    #[cfg(not(test))]
    {
        LENT.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    #[cfg(test)]
    LENT.with(|lent| lent.borrow().clone())
}

impl Store {
    /// Gives the rows of `overrides` for the addons in `order` the host's ids for this build only: the store is
    /// not written afterwards, and the aliases leave these rows out. An id another row of the build keeps
    /// (one in `order`, not itself overridden) or a registry record of an addon in `order` holds is
    /// refused, naming both rows. A registry record of an addon outside the build doesn't register this
    /// session, so its id is lent.
    pub fn lend(&mut self, overrides: &Overrides, order: &[String]) -> std::result::Result<usize, String> {
        let wanted = overrides.keys(order);
        self.lent_build = order.iter().cloned().collect();
        for key in wanted.keys() {
            if let Some(old) = self.rows.remove(key)
                && let Some(ids) = self.by_id.get_mut(table_of(key))
                && ids.get(&old).is_some_and(|k| k == key)
            {
                ids.remove(&old);
            }
        }
        let mut given: HashMap<(&str, i64), &str> = HashMap::new();
        for (key, &id) in &wanted {
            let table = table_of(key);
            if let Some(&other) = given.get(&(table, id)) {
                return Err(format!(
                    "the host's stat id {id} is given to both {} and {}",
                    row_name(other),
                    row_name(key)
                ));
            }
            given.insert((table, id), key);
            if let Some(other) = self.by_id.get(table).and_then(|ids| ids.get(&id))
                && order.iter().any(|a| a == addon_of(other))
            {
                return Err(format!(
                    "the host's stat id {id} for {} is the one your {} uses",
                    row_name(key),
                    row_name(other)
                ));
            }
            if let Some(record) = self.pinned.get(pool(table)).and_then(|ids| ids.get(&id))
                && self.pin_holds(record)
            {
                return Err(format!(
                    "the host's stat id {id} for {} is held by the game's registry record {record} here",
                    row_name(key)
                ));
            }
        }
        for (key, &id) in &wanted {
            self.insert(key.clone(), id);
            self.from.insert(key.clone(), IdFrom::Seeded);
        }
        self.lent = wanted;
        self.read_only = true;
        Ok(self.lent.len())
    }

    /// Whether the row `key` holds `id` of `table` without this build giving it that id: its addon is
    /// left out of the build, or is in it but adds no rows to the table, and no career uses `id`.
    pub(super) fn spare_row(&self, table: &str, id: i64, key: &str) -> bool {
        let addon = addon_of(key);
        let adds = self.adds_to.contains(&(addon.to_owned(), table.to_owned()));
        (!self.building.contains(addon) || (!adds && !self.claimed.contains(key)))
            && !self
                .career_held
                .get(pool(table))
                .is_some_and(|ids| ids.contains(&id))
    }

    /// Whether `record`, which `ids.json` holds `id` for, belongs to an addon this build leaves out and no career
    /// uses `id`: a full table may give the id to a row, and the core's registry moves the record when its addon
    /// is turned on.
    pub(super) fn spare_pin(&self, table: &str, id: i64, record: &str) -> bool {
        let addon = addon_of(record);
        record.contains('/')
            && addon != "game"
            && !self.building.contains(addon)
            && !self.lent_build.contains(addon)
            && !self
                .career_held
                .get(pool(table))
                .is_some_and(|ids| ids.contains(&id))
    }

    /// Whether a registry record keeps its id from this build. While a host's ids are lent, a record of an
    /// addon outside the lent build doesn't register this session, so its id is free.
    pub(super) fn pin_holds(&self, record: &str) -> bool {
        self.lent_build.is_empty() || !record.contains('/') || self.lent_build.contains(addon_of(record))
    }

    /// The first lent row the build gave another id than the host's, as the message that refuses the build.
    pub fn lent_missed(&self) -> Option<String> {
        self.lent.iter().find_map(|(key, &id)| {
            let got = self.rows.get(key).copied();
            (got != Some(id)).then(|| {
                format!(
                    "the host's stat id {id} for {} can't be used here: the build gave it {}",
                    row_name(key),
                    got.map_or("none".to_owned(), |g| g.to_string())
                )
            })
        })
    }
}
