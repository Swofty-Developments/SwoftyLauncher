use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use swofty_addon::{
    Error, Result,
    asset::AssetBatch,
    helper::run_helper,
    launcher::{Addon, BuildCx, Cx, Summary},
    manifest::AddonManifest,
    view::{Button, Effect, Event, SectionSpec, View},
};
use swofty_core::helper::HelperAddon;

struct Compat {
    manifest: AddonManifest,
    original: HelperAddon,
    dir: PathBuf,
    catalog: Vec<Value>,
}
fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}
fn write(path: &Path, value: &Value) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| Error::Other(e.to_string()))?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| Error::Other(e.to_string()))?,
    )
    .map_err(|e| Error::Other(e.to_string()))
}
impl Compat {
    fn add_count(text: &mut String, marker: &str, extra: usize) {
        if let Some(end) = text.find(marker) {
            let mut start = end;
            while start > 0 && text.as_bytes()[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if let Ok(n) = text[start..end].parse::<usize>() {
                text.replace_range(start..end, &(n + extra).to_string());
            }
        }
    }
    fn state_path(cx: &Cx) -> PathBuf {
        let hash = cx.profile.name.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
        cx.data_dir.join(format!("classic-unlocks/{hash:016x}.json"))
    }
    fn unlocked(cx: &Cx) -> BTreeSet<String> {
        let doc = read(&Self::state_path(cx));
        if doc["profile"].as_str() != Some(&cx.profile.name) {
            return BTreeSet::new();
        }
        doc["unlocked"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }
    fn entries(&self, cx: &Cx) -> Result<Vec<Value>> {
        let addons = cx
            .addons()
            .ok_or_else(|| Error::Other("Addon discovery is required".into()))?
            .list()?;
        let enabled: BTreeSet<_> = addons.into_iter().filter(|a| a.enabled).map(|a| a.id).collect();
        Ok(self
            .catalog
            .iter()
            .filter(|e| enabled.contains(e["addon"].as_str().unwrap_or("")))
            .cloned()
            .collect())
    }
    fn active(cx: &Cx) -> Option<u64> {
        read(&cx.data_dir.join("page.json"))["season"]
            .as_str()?
            .strip_prefix("grid:cs_f1_")?
            .parse()
            .ok()
    }
    fn decorate(value: &mut Value, entries: &[Value], selected: &BTreeSet<String>, year: Option<u64>) {
        if let Some(tabs) = value.get_mut("tabs") {
            for tab in tabs["items"].as_array_mut().into_iter().flatten() {
                let y = tab["id"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("grid:cs_f1_"))
                    .and_then(|s| s.parse::<u64>().ok());
                if let Some(y) = y {
                    let rows: Vec<_> = entries.iter().filter(|e| e["year"].as_u64() == Some(y)).collect();
                    if !rows.is_empty() {
                        let n = rows
                            .iter()
                            .filter(|e| selected.contains(e["key"].as_str().unwrap_or("")))
                            .count();
                        tab["badge"] = json!(format!("{n}/{}", rows.len()));
                    }
                }
            }
        }
        if let Some(table) = value.get_mut("table") {
            let is_liveries = table["columns"].as_array().is_some_and(|cols| {
                cols.iter().any(|c| {
                    c["key"] == "team"
                        || c["title"]
                            .as_str()
                            .is_some_and(|s| s.eq_ignore_ascii_case("team"))
                        || c.as_str().is_some_and(|s| s.eq_ignore_ascii_case("team"))
                })
            });
            if is_liveries {
                if let Some(rows) = table["rows"].as_array_mut() {
                    for entry in entries.iter().filter(|e| e["year"].as_u64() == year) {
                        let key = entry["key"].as_str().unwrap();
                        let on = selected.contains(key);
                        rows.push(json!({"id":format!("classic:{key}"),"title":entry["name"],"cells":[
                            entry["team_name"],entry["name"],"Unlockable",format!("F1 {}",entry["year"]),
                            format!("Official F1 {} livery",entry["year"]),"—",
                            {"text":if on {"Unlocked"} else {"Locked"},"edit":{"toggle":{"id":format!("classic:{key}"),"value":on}}}
                        ]}));
                    }
                }
                table["empty"] = json!("No special liveries match these filters.");
            }
        }
        match value {
            Value::Object(map) => {
                for child in map.values_mut() {
                    Self::decorate(child, entries, selected, year);
                }
            }
            Value::Array(array) => {
                for child in array {
                    Self::decorate(child, entries, selected, year);
                }
            }
            Value::String(s) if year.is_some() && s.contains("No Special Liveries found in this season") => {
                *s = "Official classic special liveries".into()
            }
            Value::String(s) if year.is_some() && s.contains(" of ") && s.ends_with("liveries") => {
                *s = format!(
                    "{} classic special liveries in this season",
                    entries.iter().filter(|e| e["year"].as_u64() == year).count()
                );
            }
            _ => {}
        }
    }
    fn layers(entries: &[Value], selected: &BTreeSet<String>) -> Value {
        let mut layers = serde_json::Map::new();
        for entry in entries {
            let key = entry["key"].as_str().unwrap();
            layers.insert(
                key.into(),
                json!({"unlock":if selected.contains(key) {"owned"} else {"game"}}),
            );
        }
        json!({"livery":layers})
    }
}
impl Addon for Compat {
    fn manifest(&self) -> &AddonManifest {
        &self.manifest
    }
    fn sections(&self) -> Vec<SectionSpec> {
        self.original.sections()
    }
    fn view(&self, cx: &Cx, section: &str) -> Result<View> {
        let mut view = self.original.view(cx, section)?;
        if section != "liveries" {
            return Ok(view);
        }
        let entries = self.entries(cx)?;
        let selected = Self::unlocked(cx);
        let year = Self::active(cx);
        let query = read(&cx.data_dir.join("page.json"));
        let visible: Vec<_> = entries
            .iter()
            .filter(|entry| {
                let checks = [
                    ("type", "Unlockable".to_string()),
                    ("team", entry["team_name"].as_str().unwrap_or("").to_string()),
                    (
                        "status",
                        if selected.contains(entry["key"].as_str().unwrap_or("")) {
                            "Unlocked"
                        } else {
                            "Locked"
                        }
                        .to_string(),
                    ),
                ];
                checks.iter().all(|(key, want)| {
                    query["filters"][key].as_array().is_none_or(|values| {
                        values.is_empty() || values.iter().any(|v| v.as_str() == Some(want))
                    })
                })
            })
            .cloned()
            .collect();
        let mut value = serde_json::to_value(&view).map_err(|e| Error::Other(e.to_string()))?;
        Self::decorate(&mut value, &visible, &selected, year);
        view = serde_json::from_value(value).map_err(|e| Error::Other(e.to_string()))?;
        Self::add_count(&mut view.subtitle, " special liveries", entries.len());
        Self::add_count(&mut view.subtitle, " unlockable", entries.len());
        Self::add_count(
            &mut view.subtitle,
            " unlocked",
            entries
                .iter()
                .filter(|e| selected.contains(e["key"].as_str().unwrap_or("")))
                .count(),
        );
        if let Some(y) = year.filter(|y| entries.iter().any(|e| e["year"].as_u64() == Some(*y))) {
            view.actions
                .push(Button::new("classic:unlock-all", format!("Unlock all in F1 {y}")));
            view.actions.push(Button::new("classic:lock-all", "Lock all"));
        }
        Ok(view)
    }
    fn event(&self, cx: &Cx, section: &str, event: Event) -> Result<Effect> {
        let entries = self.entries(cx)?;
        let mut selected = Self::unlocked(cx);
        match &event {
            Event::Toggle { id, on } if id.starts_with("classic:") => {
                let key = id.trim_start_matches("classic:");
                if !entries.iter().any(|e| e["key"] == key) {
                    return Err(Error::Other("Unknown classic livery".into()));
                }
                if *on {
                    selected.insert(key.into());
                } else {
                    selected.remove(key);
                }
            }
            Event::Click { id } if id == "classic:unlock-all" || id == "classic:lock-all" => {
                for entry in entries.iter().filter(|e| e["year"].as_u64() == Self::active(cx)) {
                    let key = entry["key"].as_str().unwrap();
                    if id == "classic:unlock-all" {
                        selected.insert(key.into());
                    } else {
                        selected.remove(key);
                    }
                }
            }
            _ => return self.original.event(cx, section, event),
        }
        write(
            &Self::state_path(cx),
            &json!({"profile":cx.profile.name,"unlocked":selected}),
        )?;
        Ok(Effect::Reload)
    }
    fn tick(&self, cx: &Cx, section: &str) -> Result<bool> {
        self.original.tick(cx, section)
    }
    fn summary(&self, cx: &Cx) -> Option<Summary> {
        self.original.summary(cx)
    }
    fn build(&self, cx: &BuildCx) -> Result<AssetBatch> {
        let result = self.original.build(cx)?;
        write(
            &self.dir.join("registry/classic_unlocks.json"),
            &Self::layers(&self.entries(&cx.cx)?, &Self::unlocked(&cx.cx)),
        )?;
        Ok(result)
    }
}
fn main() -> Result<()> {
    let dir = std::env::current_exe()
        .map_err(|e| Error::Other(e.to_string()))?
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let manifest: AddonManifest =
        serde_json::from_value(read(&dir.join("swofty.json"))).map_err(|e| Error::Other(e.to_string()))?;
    let mut original_doc = read(&dir.join("swofty.json"));
    original_doc["launcher"]["program"]["windows"] = json!("bin/more-special-liveries-helper.exe");
    let original_manifest = serde_json::from_value(original_doc).map_err(|e| Error::Other(e.to_string()))?;
    let original = HelperAddon::new(original_manifest, &dir)
        .ok_or_else(|| Error::Other("Missing original helper".into()))?;
    let catalog = read(&dir.join("classic-liveries.json"))
        .as_array()
        .cloned()
        .ok_or_else(|| Error::Other("Missing classic catalog".into()))?;
    run_helper(Arc::new(Compat {
        manifest,
        original,
        dir,
        catalog,
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locked_default_and_selected_only_layers() {
        let entries = vec![json!({"key":"season/a"}), json!({"key":"season/b"})];
        let locked = Compat::layers(&entries, &BTreeSet::new());
        assert_eq!(locked["livery"]["season/a"]["unlock"], "game");
        let unlocked = Compat::layers(&entries, &BTreeSet::from(["season/b".into()]));
        assert_eq!(unlocked["livery"]["season/a"]["unlock"], "game");
        assert_eq!(unlocked["livery"]["season/b"]["unlock"], "owned");
    }
    #[test]
    fn extends_only_the_active_season_table() {
        let entries = vec![
            json!({"key":"season/a","name":"Official Name","team_name":"Team","year":2021}),
            json!({"key":"season/b","name":"Other","team_name":"Team","year":2024}),
        ];
        let mut view = json!({"table":{"columns":[{"key":"team"}],"rows":[]}});
        Compat::decorate(&mut view, &entries, &BTreeSet::new(), Some(2021));
        assert_eq!(view["table"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(view["table"]["rows"][0]["cells"][1], "Official Name");
        assert_eq!(
            view["table"]["rows"][0]["cells"][6]["edit"]["toggle"]["value"],
            false
        );
    }
}
