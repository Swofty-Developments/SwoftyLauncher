use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::gvdb::Gvdb;

use super::run::Run;
use super::{GVDB, original, stock_max_ids};
use crate::install::Game;
use crate::manifest::{AddonManifest, OptionType};
use crate::profile::Profile;
use crate::settings::OnAddons;
use crate::store::Store;

/// Turns off, in `on`, each toggle that is on whose rows add a round at a track the build doesn't hold, and
/// says so: the game won't start with a race weekend at a track it has no row for.
pub(super) fn trackless_options(
    store: &Store,
    game: &Game,
    p: &Profile,
    order: &[String],
    addons: &BTreeMap<String, AddonManifest>,
    on: &mut OnAddons,
) -> Vec<String> {
    let gated: Vec<&AddonManifest> = order
        .iter()
        .filter_map(|id| addons.get(id))
        .filter(|m| {
            !m.operations.database.is_empty() && m.options.iter().any(|o| o.kind == OptionType::Toggle)
        })
        .collect();
    if gated.is_empty() {
        return Vec::new();
    }
    let Ok(shipped) = std::fs::read(original(&store.root, game, GVDB)).map(|d| Gvdb::parse(&d)) else {
        return Vec::new();
    };
    let Ok(shipped) = shipped else {
        return Vec::new();
    };
    let stock = crate::tracks::Schedule::of(&shipped);
    let held = stock.held(store, order, addons);
    let mut found = Vec::new();
    for m in gated {
        let dir = store.addons_dir().join(&m.id);
        let docs: Vec<serde_json::Value> = m
            .operations
            .database
            .iter()
            .filter_map(|rel| serde_json::from_slice(&std::fs::read(dir.join(rel)).ok()?).ok())
            .collect();
        for t in crate::tracks::option_tracks(m, &docs, &stock, &held) {
            let Some(o) = m.options.iter().find(|o| o.id == t.option) else {
                continue;
            };
            if on.value(p, m, o) == serde_json::Value::Bool(true) {
                found.push((m, o, t));
            }
        }
    }
    let codes = found.iter().map(|(_, _, t)| t.track.clone()).collect();
    let adders = crate::tracks::adders(store, &codes);
    let mut out = Vec::new();
    for (m, o, t) in found {
        on.force_off(&m.id, &o.id);
        let adds = adders
            .get(&t.track)
            .map(|id| addons.get(id).map_or(id.as_str(), |a| a.name.as_str()));
        out.push(t.line(&m.name, o.label.as_deref().unwrap_or(&o.id), adds));
    }
    out
}

/// The warnings about careers in the profile's saves that the addons on can't load or own.
pub(super) fn career_warnings(run: Run) -> Vec<String> {
    let mut out = Vec::new();
    let index = crate::stdlib::careers::data_path(&run.store.root);
    let mut careers = crate::stdlib::careers::read(&index);
    let stock_max =
        crate::stdlib::careers::stock_max_for(&careers, || stock_max_ids(&run.store.root, run.game));
    let played: BTreeSet<String> = crate::stdlib::careers::in_save_of(&careers, &run.store.root, run.profile)
        .into_iter()
        .map(|c| c.career.clone())
        .collect();
    if crate::stdlib::careers::settle_rows(
        &mut careers,
        &crate::row_ids::id_owners(&run.store.root),
        &stock_max,
        run.addons,
        Some((&played, run.order)),
    ) && !run.checking
    {
        crate::write_json_shared(&index, &serde_json::json!({ "version": 1, "careers": careers })).ok();
    }
    let scoped: Vec<_> = careers
        .iter()
        .filter(|c| played.contains(&c.career))
        .cloned()
        .collect();
    if let Some(line) = crate::stdlib::careers::stop_line(&scoped, run.order, run.addons) {
        out.push(line);
    }
    if let Some(line) = crate::stdlib::careers::unowned_line(
        &scoped,
        run.order,
        run.addons,
        &crate::row_ids::save_owners(&run.store.root),
        &stock_max,
    ) {
        out.push(line);
    }
    if let Some(line) = crate::stdlib::careers::play_line(&careers, run.order, run.addons) {
        out.push(line);
    }
    out
}
