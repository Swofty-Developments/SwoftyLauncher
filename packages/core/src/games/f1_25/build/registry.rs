use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::Built;
use super::run::Run;
use crate::addons::builtin::loader_dir;
use crate::content::runtime_assets::Runtime;
use crate::stdlib::deps::Std;
use crate::stdlib::manifest::RegistryManifest;

/// Loads the core at the early stage when it has registry records to apply at boot or applies an
/// addon's operations from them.
pub(super) fn early_stage(
    run: Run,
    std16: &BTreeMap<String, Std>,
    left_to_core: bool,
    built: &mut Built,
    runtime: &mut Runtime,
) {
    // The core applies registry records at boot only when it loads at the early stage, on the runtime and the
    // file database route alike.
    let registering = match loader_dir().is_some() {
        true => registering_addons(run.order, std16),
        false => Vec::new(),
    };
    if !registering.is_empty() {
        built.lines.push(early_stage_line(&registering));
    }
    runtime.assets.early_stage |= left_to_core || !registering.is_empty();
}

/// The names of the addons in `order` the core has registry records of to apply at boot (data
/// files or `SWOFTY_REGISTER` registrations), whatever route the database takes.
pub(super) fn registering_addons<'a>(order: &[String], std16: &'a BTreeMap<String, Std>) -> Vec<&'a str> {
    order
        .iter()
        .filter_map(|id| std16.get(id))
        .filter(|s| s.registers())
        .map(|s| s.name.as_str())
        .collect()
}

/// The build line for the addons whose registry records turn the early stage on.
pub(super) fn early_stage_line(names: &[&str]) -> String {
    format!(
        "registry: the core loads at the early stage for the registry records of {}",
        names.join(", ")
    )
}

/// The `registry.json` of each addon whose `replaced_operations` the core in
/// `loader` applies instead: the core lists the rows feature and the addon has
/// data files, which reach the core through the registry index.
pub(super) fn replaced_operations<'a>(
    std16: &'a BTreeMap<String, Std>,
    loader: Option<&Path>,
) -> BTreeMap<&'a str, &'a RegistryManifest> {
    if !loader.is_some_and(crate::stdlib::manifest::core_applies_rows) {
        return BTreeMap::new();
    }
    std16
        .iter()
        .filter(|(_, s)| !s.data_files.is_empty())
        .filter_map(|(id, s)| Some((id.as_str(), s.registry.as_ref()?)))
        .filter(|(_, r)| !r.replaced_operations.is_empty())
        .collect()
}

/// The tables the `add-row` ops of the database operations file at `path` add rows to.
pub(super) fn added_tables(path: &Path) -> BTreeSet<String> {
    let Ok(file) = crate::read_json::<serde_json::Value>(path) else {
        return BTreeSet::new();
    };
    file.get("ops")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|op| op.get("op").and_then(serde_json::Value::as_str) == Some("add-row"))
        .filter_map(|op| op.get("table")?.as_str().map(str::to_owned))
        .collect()
}
