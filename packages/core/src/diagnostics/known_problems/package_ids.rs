use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::Diagnosis;

/// The stat ids of `vehicle_packages` rows, as `ids.json` and `row-ids.json` in the data folder hold them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageIds {
    /// Every id held, by the addon that holds it.
    pub held: BTreeMap<i64, String>,
}

pub const PACKAGE_ID_MAX: i64 = 254;

pub(super) const PACKAGE_TABLE: &str = "entrants.vehicles.vehicle_packages.vehicle_package";

pub(super) const PACKAGE_LOW: i64 = 8;

impl PackageIds {
    pub fn read(data: &Path) -> Option<Self> {
        let load = |name: &str| -> Option<serde_json::Value> {
            let text = std::fs::read_to_string(crate::stdlib::reports::path(data, name)).ok()?;
            serde_json::from_str(&text).ok()
        };
        let ids = load(crate::stdlib::reports::IDS);
        let rows = load(crate::row_ids::FILE);
        if ids.is_none() && rows.is_none() {
            return None;
        }
        let mut held = BTreeMap::new();
        let mut records = |doc: &serde_json::Value| {
            if let Some(map) = doc["vehicle.package"].as_object() {
                for (record, e) in map {
                    if let Some(id) = e.as_i64().or_else(|| e["id"].as_i64()) {
                        let addon = record.split_once('/').map_or(record.as_str(), |(a, _)| a);
                        held.insert(id, addon.to_owned());
                    }
                }
            }
        };
        if let Some(ids) = &ids {
            records(&ids["ids"]);
            if let Some(careers) = ids["careers"].as_object() {
                careers.values().for_each(&mut records);
            }
        }
        if let Some(rows) = &rows {
            let mut take = |map: &serde_json::Value| {
                for (key, e) in map.as_object().into_iter().flatten() {
                    let mut parts = key.splitn(3, '/');
                    let (Some(addon), Some(table)) = (parts.next(), parts.next()) else {
                        continue;
                    };
                    if table.starts_with(PACKAGE_TABLE)
                        && let Some(id) = e["id"].as_i64()
                    {
                        held.insert(id, addon.to_owned());
                    }
                }
            };
            take(&rows["rows"]);
            for p in rows["profiles"].as_object().into_iter().flat_map(|m| m.values()) {
                take(&p["rows"]);
            }
        }
        held.retain(|id, _| (1..=PACKAGE_ID_MAX).contains(id));
        Some(Self { held })
    }

    pub fn used(&self) -> usize {
        self.held.len()
    }

    pub fn highest(&self) -> Option<i64> {
        self.held.keys().next_back().copied()
    }

    pub fn free(&self) -> usize {
        PACKAGE_ID_MAX as usize - self.used()
    }

    /// The ids held by addons that aren't in `installed`, by addon.
    pub fn orphaned(&self, installed: &BTreeSet<String>) -> BTreeMap<&str, usize> {
        let mut out = BTreeMap::new();
        for addon in self.held.values().filter(|a| !installed.contains(*a)) {
            *out.entry(addon.as_str()).or_insert(0) += 1;
        }
        out
    }

    pub fn summary(&self, installed: &BTreeSet<String>) -> String {
        let orphans: usize = self.orphaned(installed).values().sum();
        format!(
            "{} of {PACKAGE_ID_MAX} used, highest {}, {orphans} held by addons not installed",
            self.used(),
            self.highest().unwrap_or(0)
        )
    }

    /// A diagnosis when few ids are left: the addons not installed whose ids could be freed come first.
    pub fn diagnosis(
        &self,
        installed: &BTreeSet<String>,
        names: &BTreeMap<String, String>,
    ) -> Option<Diagnosis> {
        if self.free() as i64 > PACKAGE_LOW {
            return None;
        }
        let orphans = self.orphaned(installed);
        let mut what = format!(
            "The car package ids (vehicle_packages stat_id) are nearly used up: {}.",
            self.summary(installed)
        );
        let mut steps = Vec::new();
        if orphans.is_empty() {
            steps.push(
                "Every held id belongs to an installed addon: turn off or remove car addons you don't use, then press Play again."
                    .to_owned(),
            );
        } else {
            let list: Vec<String> = orphans
                .iter()
                .map(|(a, n)| format!("{} ({n})", names.get(*a).map_or(*a, String::as_str)))
                .collect();
            what.push_str(&format!(
                " Ids are held by addons that are not installed: {}.",
                list.join(", ")
            ));
            steps.push("Their ids are reused once a build needs them; if a build still fails, remove car addons you don't use.".to_owned());
        }
        Some(Diagnosis {
            id: "package_ids_full",
            title: "Car package ids are nearly used up".to_owned(),
            what,
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(name: &str, ids: &str, rows: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-pkgids-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("registry")).unwrap();
        std::fs::write(dir.join("registry/ids.json"), ids).unwrap();
        std::fs::write(dir.join("registry/row-ids.json"), rows).unwrap();
        dir
    }

    #[test]
    fn a_nearly_full_table_names_the_ids_addons_that_are_gone_hold() {
        let records: Vec<String> = (1..=250).map(|i| format!(r#""gone.x/p{i}":{i}"#)).collect();
        let ids = format!(r#"{{"ids":{{"vehicle.package":{{{}}}}}}}"#, records.join(","));
        let rows = r#"{"rows":{"here/entrants.vehicles.vehicle_packages.vehicle_package/a":{"id":254,"from":"new"}}}"#;
        let dir = data("full", &ids, rows);
        let held = PackageIds::read(&dir).unwrap();
        let installed = BTreeSet::from(["here".to_owned()]);
        assert_eq!(held.used(), 251);
        assert_eq!(held.highest(), Some(254));
        assert_eq!(held.orphaned(&installed).get("gone.x"), Some(&250));
        let d = held.diagnosis(&installed, &BTreeMap::new()).unwrap();
        assert_eq!(d.id, "package_ids_full");
        assert!(
            d.what
                .contains("251 of 254 used, highest 254, 250 held by addons not installed")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_table_with_room_has_no_diagnosis() {
        let dir = data("room", r#"{"ids":{"vehicle.package":{"a/p":{"id":5}}}}"#, "{}");
        let held = PackageIds::read(&dir).unwrap();
        assert!(held.diagnosis(&BTreeSet::new(), &BTreeMap::new()).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
