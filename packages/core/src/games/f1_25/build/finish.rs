use super::database::{DbEdits, with_row_ids};
use super::inputs::Inputs;
use super::run::Run;
use super::saved::Saved;
use super::setup::Setup;
use super::tracks::track_files;
use super::{ARCHIVE_AGE, ARCHIVE_BYTES, Built, PACKED_AGE, PLAN_FILE, Pipeline, SAVED_FILE, source_dir};
use crate::addons::builtin::CORE_ID;
use crate::asset::{Op, Source};
use crate::online::multiplayer::fingerprint;
use crate::planner::{Target, waiting_lines};
use crate::profile::conflicts::Report;
use crate::{Result, io_err};

impl Pipeline<'_, '_> {
    /// Hands the runtime its ops, reports the conflicts and what each addon changed, writes the plan and
    /// the saved build, and prunes the caches no build uses.
    pub(super) fn finish(self, mut inputs: Inputs, edits: DbEdits, setup: &Setup) -> Result<Built> {
        let Self {
            run,
            planner,
            mut runtime,
            mut built,
            session,
            mut caches,
            ..
        } = self;
        let (store, game, profile) = (run.store, run.game, run.profile);
        let fitted = &inputs.tracks;
        let (lines_at, warnings_at) = inputs.saved_from;
        let DbEdits {
            stock_removals,
            kept_lists,
            waits,
            row_ids_unsaved,
            ..
        } = edits;
        let features = &setup.features;
        let packed = &setup.packed;
        built.settings = Some(session);
        let mut changes = planner.changes();
        let mut planned = planner.finish();
        for op in planned.runtime {
            let resolve =
                |op: &Op, s: &Source| Ok(s.resolve(&source_dir(store, &op.origin.addon, s.root))?);
            runtime.push(op, &resolve)?;
        }
        if let Some(runtime) = runtime.finish(&built.overlay)? {
            built.lines.push(runtime.summary());
            for (rel, src) in runtime.files {
                built.place_loader(rel, src)?;
            }
        }
        for (rel, bytes) in std::mem::take(&mut inputs.shader_files.files) {
            let path = built
                .overlay
                .join(crate::content::runtime_assets::SWOFTY)
                .join(&rel);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&path, bytes).map_err(io_err(&path))?;
            built
                .plan
                .loader
                .insert(format!("{}/{rel}", crate::content::runtime_assets::SWOFTY), path);
        }
        built.warnings.extend(waiting_lines(&planned.waiting, features));
        for (_, _, origin) in &fitted.kept {
            changes.entry(origin.addon.clone()).or_default().tracks += 1;
        }
        let name = |id: &str| run.name(id);
        for id in run.order.iter().filter(|id| *id != CORE_ID) {
            if let Some(c) = changes.get(id).filter(|c| !c.is_empty()) {
                built.lines.push(format!("{}: {c}", name(id)));
            }
        }
        for c in &mut planned.conflicts {
            if matches!(&c.target, Target::File(f) if f == crate::addons::builtin::CORE_INTRO_CACHED)
                || matches!(&c.target, Target::String { key, .. } if key == crate::addons::builtin::CORE_INTRO_SUBTITLE)
            {
                c.overridden.retain(|o| o.addon != CORE_ID);
            }
        }
        planned.conflicts.retain(|c| !c.overridden.is_empty());
        let records = planned
            .conflicts
            .iter()
            .any(|c| matches!(c.target, Target::Record(_)));
        let catalog = records
            .then(|| crate::dbedit::Catalog::open(store, game).ok())
            .flatten();
        let record = |key: u64| {
            planned.record_names.get(&key).cloned().or_else(|| {
                catalog
                    .as_deref()
                    .and_then(|c| crate::profile::conflicts::record_name(c, key))
            })
        };
        let (report, within) = crate::profile::conflicts::summarise(profile, &planned.conflicts, &record);
        built.warnings.extend(report.pairs.iter().map(|p| p.line(&name)));
        built.warnings.extend(within);
        built.report = Report {
            order: built.report.order.clone(),
            stock_removals,
            kept_lists,
            waits,
            hud_editor: built.report.hud_editor.take(),
            ..report
        };
        built.conflicts = planned.conflicts;
        built.waiting = planned.waiting;
        if !built.plan.is_vanilla() {
            let plan_tracks: Vec<crate::tracks::PlanTrack> = fitted
                .kept
                .iter()
                .map(|(_, pkg, origin)| crate::tracks::PlanTrack {
                    code: pkg.track_code.clone(),
                    name: pkg.track_name.clone().filter(|n| !n.trim().is_empty()),
                    addon: Some(origin.addon.clone()),
                })
                .collect();
            let listing = serde_json::json!({
                "profile": profile,
                "buildhash": built.buildhash,
                "tracks": plan_tracks,
                "replace": built.plan.data,
                "add": built.plan.added,
                "loader": built.plan.loader,
            });
            crate::write_json(&built.overlay.join(PLAN_FILE), &listing)?;
        }
        run.cancel()?;
        crate::now_doing("saving the build");
        let saved = (!built.plan.is_vanilla())
            .then(|| Saved::of(&built, lines_at, warnings_at))
            .transpose()?;
        caches.push(built.overlay.clone());
        caches.extend(
            [&built.plan.data, &built.plan.added, &built.plan.loader]
                .into_iter()
                .flat_map(|m| m.values().cloned()),
        );
        let removed = crate::cache::prune(&store.root.join("cache"), profile, &caches);
        if removed > 0 {
            built
                .lines
                .push(format!("cache: {removed} stale build folder(s) removed"));
        }
        crate::cache::prune_packed(packed, PACKED_AGE);
        crate::cache::prune_archives(
            &store.root.join("cache").join(crate::cache::ARCHIVES_DIR),
            ARCHIVE_AGE,
            ARCHIVE_BYTES,
        );
        // A check keeps no row ids, so a build that reused this one would never keep them
        // either, and a later build could give those rows other ids than a career holds.
        if let Some(saved) = saved.filter(|_| !row_ids_unsaved) {
            crate::write_json(&built.overlay.join(SAVED_FILE), &saved)?;
        }
        crate::write_json(&crate::profile::conflicts::path(store, profile), &built.report)?;
        record_build(run, &mut built, &mut inputs);
        Ok(built)
    }
}

/// Records what the build made of each addon and writes the track files; a check records neither.
pub(super) fn record_build(run: Run, built: &mut Built, inputs: &mut Inputs) {
    if run.checking {
        return;
    }
    let outputs = std::mem::take(&mut inputs.outputs);
    fingerprint::record_outputs(run.store, run.profile, with_row_ids(outputs, &built.row_ids)).ok();
    track_files(run.store, run.game, built, &inputs.tracks, &inputs.record_ui);
}

impl Pipeline<'_, '_> {
    /// Finishes the build from the saved one with the same build hash.
    pub(super) fn reuse(mut self, saved: Saved, mut inputs: Inputs) -> Result<Built> {
        let run = self.run;
        saved.restore(&mut self.built)?;
        self.built.reused = true;
        self.built.report.order = run.settings.enabled().map(str::to_owned).collect();
        crate::write_json(
            &crate::profile::conflicts::path(run.store, run.profile),
            &self.built.report,
        )?;
        record_build(run, &mut self.built, &mut inputs);
        Ok(self.built)
    }
}
