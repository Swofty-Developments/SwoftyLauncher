use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;

use super::{ALIAS_TABLES, ALIASES_VERSION, Store, addon_of, pool};

/// `swofty/id_aliases.json`: for each of `teams`, `vehicles` and `drivers`, the ids added rows had
/// before (1.6.10 and 1.6.11 hashed ids, ids before 1.6.10) mapped to the id they have now. The core
/// reads it at StateLoadSession to load a save made under the earlier ids.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdAliases {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub tables: BTreeMap<String, BTreeMap<i64, i64>>,
}

impl IdAliases {
    pub fn is_empty(&self) -> bool {
        self.tables.values().all(BTreeMap::is_empty)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let file = IdAliases {
            version: ALIASES_VERSION,
            tables: self.tables.clone(),
        };
        crate::write_json(path, &file)
    }
}

impl Store {
    fn pinned_in_build(&self, table: &str, id: i64) -> bool {
        self.pinned
            .get(pool(table))
            .and_then(|ids| ids.get(&id))
            .is_some_and(|record| self.building.is_empty() || self.building.contains(addon_of(record)))
    }

    /// The ids rows had before the ones they keep, by the tables saves hold them of; see [`IdAliases`].
    /// Only rows this build's database edits added give aliases: a kept row of an addon that now adds it
    /// through the core's registry has the id its registry record pins, not the one kept here. An old id
    /// a registry record of an addon in this build pins is a live row's id, so it maps nowhere; a record of an
    /// addon the build leaves out holds no row here and does not block the alias.
    pub fn id_aliases(&self) -> IdAliases {
        let mut out = IdAliases::default();
        let mut seen: HashMap<(&str, i64), Option<i64>> = HashMap::new();
        for (key, olds) in &self.aliases {
            if !self.building.is_empty() && !self.building.contains(addon_of(key)) {
                continue;
            }
            if !self.claimed.contains(key) {
                continue;
            }
            let Some(&now) = self.rows.get(key).filter(|_| !self.lent.contains_key(key)) else {
                continue;
            };
            let table = key.split('/').nth(1).unwrap_or_default();
            for &old in olds {
                if old == now
                    || self.by_id.get(table).is_some_and(|ids| ids.contains_key(&old))
                    || self.pinned_in_build(table, old)
                {
                    continue;
                }
                if self
                    .base_ids
                    .get(table)
                    .and_then(|ids| ids.get(&old))
                    .is_some_and(|holder| holder != key)
                {
                    continue;
                }
                match seen.entry((table, old)) {
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(Some(now));
                    }
                    std::collections::hash_map::Entry::Occupied(mut o) => {
                        if o.get() != &Some(now) {
                            o.insert(None);
                        }
                    }
                }
            }
        }
        for ((table, old), now) in seen {
            let (Some(now), Some(name)) = (
                now,
                ALIAS_TABLES.iter().find(|(t, _)| *t == table).map(|(_, n)| *n),
            ) else {
                continue;
            };
            out.tables.entry(name.to_owned()).or_default().insert(old, now);
        }
        out
    }
}
