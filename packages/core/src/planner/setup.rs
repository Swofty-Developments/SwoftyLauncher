use std::collections::{BTreeMap, BTreeSet};

pub use crate::asset::{Features, Kind};
use crate::ops::{no_row, no_row_selectors};

use super::{BUILD_DELIVERIES, MipmapNames, OverlayNames, Planned, Planner, RUNTIME_DELIVERIES, Route};

impl<'a> Planner<'a> {
    pub fn new(features: &'a Features, opt_in: &'a BTreeSet<Kind>) -> Self {
        static NONE: BTreeSet<Kind> = BTreeSet::new();
        Self {
            features,
            opt_in,
            build_only: &NONE,
            db: Vec::new(),
            erp: Vec::new(),
            files: Vec::new(),
            tracks: Vec::new(),
            items: Vec::new(),
            planned: Planned::default(),
            writers: BTreeMap::new(),
            ranks: Vec::new(),
            removals: 0,
            fields: BTreeMap::new(),
            skipped: Vec::new(),
            id_left_out: BTreeMap::new(),
            waits: BTreeSet::new(),
            stock_removals: BTreeMap::new(),
            track_removals: BTreeMap::new(),
            lists: None,
            kept_lists: BTreeMap::new(),
            copies: Vec::new(),
            lenient: BTreeMap::new(),
            names: BTreeMap::new(),
            left_out_tracks: BTreeMap::new(),
            rewrites: Vec::new(),
            core_rows: BTreeMap::new(),
            touched: BTreeMap::new(),
            overlays: None,
            mipmaps: None,
            row_ids: None,
        }
    }

    pub fn overlays(mut self, names: &'a OverlayNames<'a>) -> Self {
        self.overlays = Some(names);
        self
    }

    pub fn mipmaps(mut self, names: &'a MipmapNames<'a>) -> Self {
        self.mipmaps = Some(names);
        self
    }

    pub fn build_only(mut self, kinds: &'a BTreeSet<Kind>) -> Self {
        self.build_only = kinds;
        self
    }

    /// Addons (id → name) whose failing database ops are skipped with a
    /// warning instead of failing the build: the DB Editor's packages.
    pub fn lenient(mut self, addons: BTreeMap<String, String>) -> Self {
        self.lenient = addons;
        self
    }

    /// The addons' names (id → name), for the errors that name an addon.
    pub fn names(mut self, names: BTreeMap<String, String>) -> Self {
        self.names = names;
        self
    }

    pub(super) fn name<'s>(&'s self, id: &'s str) -> &'s str {
        self.names
            .get(id)
            .or_else(|| self.lenient.get(id))
            .map_or(id, String::as_str)
    }

    /// Skips `id`'s failing database ops from now on: an addon whose track a
    /// build leaves out has ops on that track's rows.
    pub fn add_lenient(&mut self, id: &str, name: &str) {
        self.lenient
            .entry(id.to_owned())
            .or_insert_with(|| name.to_owned());
    }

    /// Skips, with a warning, any addon's change whose row of track `code` is still missing after every addon's
    /// changes: the build leaves that track, named `track`, out because the game's track lists are full.
    pub fn leave_out_track(&mut self, code: &str, track: &str) {
        self.left_out_tracks.insert(code.to_owned(), track.to_owned());
    }

    // The name of the left-out track whose row `e`'s selector matches no row of; `None` for any other failure.
    pub(super) fn left_out_track(&self, e: &swofty_addon::Error) -> Option<&str> {
        if !no_row(e) {
            return None;
        }
        let failing = no_row_selectors(e).pop()?;
        self.left_out_tracks.get(failing.want.trim()).map(String::as_str)
    }

    /// Records that the core adds rows to `tables` at the game's start, from
    /// the records of `addon` that the build leaves to it. An op that selects
    /// such a row finds none while the build runs.
    pub fn add_core_rows(&mut self, addon: &str, tables: impl IntoIterator<Item = String>) {
        self.core_rows.entry(addon.to_owned()).or_default().extend(tables);
    }

    pub(super) fn core_row_adders(&self, e: &swofty_addon::Error) -> Vec<&str> {
        let swofty_addon::Error::Resolve(m) = e else {
            return Vec::new();
        };
        if !no_row(e) {
            return Vec::new();
        }
        let path = m.split(['[', ':']).next().unwrap_or_default();
        self.core_rows
            .iter()
            .filter(|(_, tables)| {
                tables
                    .iter()
                    .any(|t| path == t || path.strip_prefix(t.as_str()).is_some_and(|r| r.starts_with('.')))
            })
            .map(|(addon, _)| addon.as_str())
            .collect()
    }

    pub fn route(&self, kind: Kind) -> Route {
        match self.delivery(kind) {
            Route::Runtime if kind == Kind::Erp && self.removals > 0 => Route::Build,
            route => route,
        }
    }

    pub fn route_note(&self, kind: Kind) -> Option<String> {
        (self.route(kind) != self.delivery(kind)).then(|| {
            format!(
                "built into the game files; {} resource removal(s) cannot apply at runtime",
                self.removals
            )
        })
    }

    fn delivery(&self, kind: Kind) -> Route {
        let stable = self.features.stable.contains(&kind);
        let experimental = self.features.experimental.contains(&kind);
        let build = BUILD_DELIVERIES.contains(&kind);
        if build && self.build_only.contains(&kind) {
            return Route::Build;
        }
        let runtime =
            RUNTIME_DELIVERIES.contains(&kind) && (stable || (experimental && self.opt_in.contains(&kind)));
        match (runtime, build) {
            (true, _) => Route::Runtime,
            (false, true) => Route::Build,
            (false, false) => Route::Waiting,
        }
    }
}
