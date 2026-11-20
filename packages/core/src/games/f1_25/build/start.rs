use std::collections::{BTreeMap, BTreeSet};

use super::Built;
use super::originals::Shipped;
use super::run::Run;
use crate::addons::settings::OnAddons;
use crate::install::Plan;
use crate::manifest::{AddonManifest, OptionType};
use crate::profile::Profile;
use crate::profile::conflicts::Report;
use crate::profile::resolver::resolve;
use crate::row_ids::Overrides;
use crate::stdlib::deps::Std;
use crate::store::Store;
use crate::{Error, Result};

/// A profile as a build reads it.
pub(super) struct AsBuilt {
    pub profile: Profile,
    /// A line for each on addon left out because its manifest no longer reads.
    pub skipped: Vec<String>,
    /// The lent load order and option values, with how many of each the profile took.
    pub lent: Option<((usize, usize), Overrides)>,
}

/// `profile` with each on addon whose manifest no longer reads turned off, and with the lent load order and option
/// values when some are lent.
pub(super) fn as_built(
    store: &Store,
    profile: &str,
    addons: &BTreeMap<String, AddonManifest>,
) -> Result<AsBuilt> {
    let mut p = store.profile(profile)?;
    let mut skipped = Vec::new();
    for u in store.unreadable_addons() {
        if !addons.contains_key(&u.id) && p.enabled().any(|id| id == u.id) {
            p.set_enabled(&u.id, false)?;
            skipped.push(format!("{} is left out: it {}", u.name, u.reason));
        }
    }
    let lent = crate::row_ids::lent().map(|o| (p.lend(&o), o));
    Ok(AsBuilt {
        profile: p,
        skipped,
        lent,
    })
}

/// The profile a build is of, as the build reads it: its addons on in load order, their options and
/// registry data, and a room launch's lent settings.
pub(super) struct Start {
    pub settings: Profile,
    pub order: Vec<String>,
    pub on: OnAddons,
    pub std16: BTreeMap<String, Std>,
    /// A room launch's lent load order and option values, with how many places and values they changed.
    pub lent: Option<((usize, usize), Overrides)>,
    pub built: Built,
}

impl Start {
    /// Leaves out the addons on that can't be read, resolves the load order, and stops a profile whose
    /// standard library dependencies fail.
    pub fn read(
        store: &Store,
        profile: &str,
        addons: &BTreeMap<String, AddonManifest>,
        shipped: &Shipped,
    ) -> Result<Self> {
        let AsBuilt {
            profile: p,
            mut skipped,
            lent: lent_settings,
        } = as_built(store, profile, addons)?;
        let resolved = resolve(&p, addons)?;
        let mut on = OnAddons::new(resolved.order.iter().filter_map(|id| addons.get(id)));
        let std16 = match p.is_vanilla() {
            true => BTreeMap::new(),
            false => crate::stdlib::deps::read_all(store, addons),
        };
        let deps = crate::stdlib::deps::for_order(profile, &resolved.order, addons, &std16);
        if let Some(stop) = deps.stop_text() {
            return Err(Error::Dependencies(stop));
        }
        skipped.extend(trackless_options(
            store,
            shipped,
            &p,
            &resolved.order,
            addons,
            &mut on,
        ));
        let mut built = Built {
            order: resolved.order.clone(),
            warnings: skipped
                .into_iter()
                .chain(resolved.warnings.iter().cloned())
                .collect(),
            plan: Plan {
                profile: profile.into(),
                ..Plan::default()
            },
            ..Built::default()
        };
        if let Some(((ordered, values), lent)) = &lent_settings
            && (*ordered > 0 || *values > 0)
        {
            built.lines.push(format!(
                "launch: using the host's load order and {values} option value(s) for {} addon(s)",
                lent.order
                    .iter()
                    .chain(lent.options.keys())
                    .collect::<BTreeSet<_>>()
                    .len()
            ));
        }
        built
            .warnings
            .extend(deps.of(crate::stdlib::deps::Level::Warn).map(|f| f.line()));
        built.lines.extend(
            deps.of(crate::stdlib::deps::Level::Hint)
                .map(|f| format!("registry: {}", f.line())),
        );
        built
            .lines
            .extend(deps.notes.iter().map(|n| format!("registry: {n}")));
        Ok(Self {
            order: resolved.order,
            settings: p,
            on,
            std16,
            lent: lent_settings,
            built,
        })
    }
}

/// The conflict report a build starts from: none yet, for the addons on in the profile.
pub(super) fn report(run: Run) -> Report {
    Report {
        profile: run.profile.into(),
        built_at: crate::profile::conflicts::now(),
        pairs: Vec::new(),
        order: run.settings.enabled().map(str::to_owned).collect(),
        stock_removals: BTreeMap::new(),
        kept_lists: BTreeMap::new(),
        waits: Vec::new(),
    }
}

/// Turns off, in `on`, each toggle that is on whose rows add a round at a track the build doesn't hold, and
/// says so: the game won't start with a race weekend at a track it has no row for.
pub(super) fn trackless_options(
    store: &Store,
    shipped: &Shipped,
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
    let Some(shipped) = shipped.get() else {
        return Vec::new();
    };
    let stock = crate::tracks::Schedule::of(shipped);
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
pub(super) fn career_warnings(run: Run, shipped: &Shipped) -> Vec<String> {
    let mut out = Vec::new();
    let index = crate::stdlib::careers::data_path(&run.store.root);
    let mut careers = crate::stdlib::careers::read(&index);
    let stock_max = crate::stdlib::careers::stock_max_for(&careers, || shipped.max_ids());
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
