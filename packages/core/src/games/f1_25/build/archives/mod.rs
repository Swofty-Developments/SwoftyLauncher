use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod merges;
mod puts;
mod warnings;

#[cfg(test)]
pub(super) use self::merges::kept_menus_line;
pub(super) use self::merges::{carries_handling, merged_later, slice_merges};
pub(super) use self::warnings::{
    missing_archives, stock_holders, stray_asset_group_line, stray_asset_groups,
};

use self::merges::{contested_in, keep_menus, record_merge, ui_merge_notes};
use self::puts::{ArchivePut, after_puts, archive_puts};
use self::warnings::{duplicate_entry, missing_resource};
use super::inputs::Inputs;
use super::run::Run;
use super::{Built, Pipeline, source_dir};
use crate::activation::ers::{self, ErsMap};
use crate::activation::ui::Kept;
use crate::asset::Kind;
use crate::asset::{AssetOp, Op, Source};
use crate::content::runtime_assets::{Prebuilt, overlay_fits};
use crate::erp::{ErpBuild, Held, Resized, reads, writes};
use crate::manifest::relative_inside;
use crate::ops::ErpOp;
use crate::planner::Route;
use crate::{Error, Result};

/// What applying the archive changes leaves for placing the files.
pub(super) struct Archives {
    /// The `file.put` ops, which the file stage places.
    pub file_ops: Vec<Op>,
    /// The mip files the archive build writes, by the game path they go to.
    pub mipmaps: BTreeMap<String, PathBuf>,
    /// The archives put whole that later resource edits were built over.
    pub built_over: BTreeSet<String>,
}

type Puts = BTreeMap<String, ArchivePut>;

/// Where the archive build reads each archive from: the copy an earlier stage staged, else the one an
/// addon puts whole, else the game's own.
struct Bases<'s> {
    staged: BTreeMap<String, PathBuf>,
    puts: &'s Puts,
    run: Run<'s>,
}

impl Bases<'_> {
    fn path(&self, rel: &str) -> PathBuf {
        self.staged
            .get(rel)
            .or_else(|| self.puts.get(rel).map(|p| &p.path))
            .cloned()
            .unwrap_or_else(|| self.run.original(rel))
    }
}

/// What applying the ERP ops leaves for writing the archives: every origin that writes, in load order (the
/// later wins), and the overlay archives that load as they are.
#[derive(Default)]
struct Applied {
    ranks: Vec<String>,
    prebuilt: Vec<Prebuilt>,
}

impl Pipeline<'_, '_> {
    /// Applies every addon's ERP ops to the game's archives and the archives addons put whole, as runtime
    /// resources or as archives written to the overlay.
    pub(super) fn apply_archives(
        &mut self,
        inputs: &Inputs,
        ers_maps: &[ErsMap],
        menu_kept: &Kept,
    ) -> Result<Archives> {
        let run = self.run;
        let (file_ops, mut erp_ops, puts) = self.archive_ops()?;
        for (id, folder, n) in stray_asset_groups(&run.game.dir, &file_ops) {
            self.built
                .warnings
                .push(stray_asset_group_line(&run.origin_name(&id), &folder, n));
        }
        let bases = Bases {
            staged: self.built.plan.data.clone(),
            puts: &puts,
            run,
        };
        if !inputs.hud_inputs.is_empty() {
            let ops = crate::diagnostics::phases::time_part("build.apply.hud", || {
                self.compose_hud(inputs, &bases, &erp_ops)
            })?;
            erp_ops.extend(ops);
        }
        let mut archives = Archives {
            file_ops,
            mipmaps: BTreeMap::new(),
            built_over: BTreeSet::new(),
        };
        if erp_ops.is_empty() && puts.is_empty() {
            return Ok(archives);
        }
        self.drop_unreadable(&mut erp_ops, &bases);
        self.note_built_over_puts(&erp_ops, &puts);
        let base = |rel: &str| bases.path(rel);
        let resolve = |rel: &str| run.original(rel);
        let mut erps = ErpBuild::new(&base)
            .with_baseline(&resolve)
            .with_packer(self.run.packer.clone());
        for rel in puts.keys() {
            erps.load(rel)?;
        }
        for (erp, resource) in menu_kept.keys() {
            erps.keep(erp, resource);
        }
        let applied = crate::diagnostics::phases::time_part("build.apply.ops", || {
            self.apply_erp_ops(&mut erps, &erp_ops, &puts, &base)
        })?;
        crate::diagnostics::phases::time_part("build.apply.checks", || {
            self.check_archives(&mut erps, &erp_ops, inputs, ers_maps, menu_kept)
        })?;
        archives.mipmaps = erps.take_mipmaps();
        let resized = crate::diagnostics::phases::time_part("build.apply.write", || {
            self.write_archives(erps, &bases.staged, applied)
        })?;
        self.built.warnings.extend(crate::diagnostics::phases::time_part(
            "build.apply.stock_holders",
            || stock_holders(&run.game.dir, &resized),
        ));
        archives.built_over = self.built_over(&puts, &bases.staged);
        Ok(archives)
    }

    /// The planner's file and ERP ops, and the archives addons put whole; the ERP ops an archive put
    /// replaces are dropped.
    fn archive_ops(&mut self) -> Result<(Vec<Op>, Vec<Op>, Puts)> {
        let run = self.run;
        let planner = &mut self.planner;
        let file_ops = planner.take_files();
        let mut erp_ops = planner.take_erp();
        let puts = archive_puts(run.store, &file_ops, &erp_ops, &|o| run.rank(o))?;
        for (rel, put) in &puts {
            planner.put_archive(rel, &put.origin);
        }
        erp_ops.retain_mut(|op| {
            let rank = run.rank(&op.origin);
            after_puts(op, &puts, rank)
        });
        Ok((file_ops, erp_ops, puts))
    }

    /// Composes the HUD addons' layers into the game's HUD, and the ERP ops that put it in.
    fn compose_hud(&mut self, inputs: &Inputs, bases: &Bases, erp_ops: &[Op]) -> Result<Vec<Op>> {
        let run = self.run;
        let (store, game) = (run.store, run.game);
        let packer = self.run.eager_packer();
        let Self { built, session, .. } = self;
        let (hud_inputs, shared) = crate::hud::shared_records(inputs.hud_inputs.clone())?;
        built.lines.extend(shared.iter().map(|l| format!("hud: {l}")));
        let hud_inputs = &hud_inputs;
        let stock = crate::hud::GameStock::with_baseline(&game.dir, &store.root)
            .unwrap_or_else(|_| crate::hud::GameStock::new(&game.dir));
        let base = |rel: &str| bases.path(rel);
        let resolve = |rel: &str| run.original(rel);
        let name = |origin: &str| run.origin_name(origin);
        let mfd_rows = match inputs.menus {
            true => crate::ui::mfd_rows(session),
            false => Vec::new(),
        };
        let page = crate::ui::mfd_page_rows(&mfd_rows);
        let mfd_edit = |data: &[u8]| crate::ui::mfd::templates(data, &page);
        let put_names: Vec<String> = bases.puts.keys().cloned().collect();
        let composed = crate::hud::integrate::compose_in_build(
            &crate::hud::integrate::Context {
                store,
                stock: &stock,
                base: &base,
                baseline: &resolve,
                puts: &put_names,
                name: &name,
                packer: &packer,
            },
            erp_ops,
            hud_inputs,
            (!page.is_empty()).then_some(&mfd_edit as crate::hud::TemplateBase),
        )?;
        let owner = hud_inputs[0].addon.clone();
        composed
            .output
            .write(&store.addon_cache_dir(&owner).join("hud"))?;
        built
            .lines
            .extend(composed.lines.iter().map(|l| format!("hud: {l}")));
        built.warnings.extend(composed.warnings);
        Ok(composed.ops)
    }

    /// Drops the ERP ops that read an archive the game folder lacks, with a warning for each addon.
    fn drop_unreadable(&mut self, erp_ops: &mut Vec<Op>, bases: &Bases) {
        let run = self.run;
        let mut absent: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        erp_ops.retain(|op| {
            let AssetOp::Erp(e) = &op.op else {
                return true;
            };
            let gone: Vec<&str> = reads(e)
                .into_iter()
                .filter(|rel| relative_inside(rel) && !bases.path(rel).is_file())
                .collect();
            if gone.is_empty() {
                return true;
            }
            absent
                .entry(op.origin.addon.clone())
                .or_default()
                .extend(gone.into_iter().map(str::to_owned));
            false
        });
        for (id, rels) in &absent {
            self.built
                .warnings
                .push(missing_archives(&run.origin_name(id), rels));
        }
    }

    fn note_built_over_puts(&mut self, erp_ops: &[Op], puts: &Puts) {
        for (rel, put) in puts {
            if erp_ops
                .iter()
                .any(|op| matches!(&op.op, AssetOp::Erp(e) if writes(e).contains(&rel.as_str())))
            {
                self.built.lines.push(format!(
                    "erp: {rel}: later changes apply to the whole archive {} puts",
                    put.origin
                ));
            }
        }
    }

    /// Applies each ERP op in load order. On the runtime route an overlay archive that fits loads as it is;
    /// any other goes in resource by resource.
    fn apply_erp_ops(
        &mut self,
        erps: &mut ErpBuild,
        erp_ops: &[Op],
        puts: &Puts,
        base: &dyn Fn(&str) -> PathBuf,
    ) -> Result<Applied> {
        let run = self.run;
        let store = run.store;
        let name = |origin: &str| run.origin_name(origin);
        let Self { planner, built, .. } = self;
        let contested =
            crate::erp::contested_ui(erp_ops, &|op, s| source_dir(store, &op.origin.addon, s.root));
        let runtime_route = planner.route(Kind::Erp) == Route::Runtime;
        let mut applied = Applied::default();
        let slices = slice_merges(erp_ops);
        for (at, op) in erp_ops.iter().enumerate() {
            run.cancel()?;
            crate::now_doing(format!("applying {}", op.origin));
            let id = &op.origin.addon;
            let resolve = |s: &Source| Ok(s.resolve(&source_dir(store, id, s.root))?);
            let origin = op.origin.to_string();
            if matches!(op.op, AssetOp::Erp(_)) && !applied.ranks.contains(&origin) {
                applied.ranks.push(origin.clone());
            }
            match &op.op {
                AssetOp::Erp(ErpOp::Overlay {
                    erps: targets,
                    source,
                }) if runtime_route => {
                    let path = resolve(source)?;
                    match overlay_loads_whole(erps, &path, at, targets, &slices, &contested, base)? {
                        Ok(resources) => applied.prebuilt.push(Prebuilt {
                            origin,
                            path,
                            erps: targets.clone(),
                            resources,
                        }),
                        Err(why) => {
                            built.lines.push(format!(
                                "erp: {origin}: its overlay archive goes in resource by resource; {why}"
                            ));
                            erps.apply_for(
                                &ErpOp::Overlay {
                                    erps: targets.clone(),
                                    source: source.clone(),
                                },
                                &origin,
                                &resolve,
                            )
                            .map_err(|e| Error::Other(format!("applying {}: {e}", op.origin)))?;
                            ui_merge_notes(built, planner, erps.take_ui_merges(), &name);
                        }
                    }
                }
                AssetOp::Erp(erp) => {
                    erps.set_merge_ui(op.origin.item.as_deref() != Some(crate::hud::integrate::ORIGIN));
                    match erps.apply_for(erp, &op.origin.to_string(), &resolve) {
                        Ok(()) => {}
                        Err(Error::ResourceMissing { erp, resource }) if puts.contains_key(&erp) => {
                            built.warnings.push(missing_resource(
                                &name(&op.origin.to_string()),
                                &erp,
                                &resource,
                                &name(&puts[&erp].origin.to_string()),
                            ));
                            continue;
                        }
                        Err(Error::DuplicateEntry {
                            resource,
                            at,
                            entry,
                            earlier,
                        }) => {
                            return Err(Error::Other(duplicate_entry(
                                &name(&op.origin.to_string()),
                                &name(&earlier),
                                &format!("{}", op.origin),
                                &format!("{resource}: {at}"),
                                &entry,
                            )));
                        }
                        Err(Error::StaleInput { rel, detail }) => {
                            return Err(Error::StaleInput {
                                rel,
                                detail: format!("applying {}: {detail}", op.origin),
                            });
                        }
                        Err(e) => {
                            return Err(Error::Other(format!("applying {}: {e}", op.origin)));
                        }
                    }
                    for m in erps.take_merges() {
                        record_merge(built, planner, &op.origin, m);
                    }
                    ui_merge_notes(built, planner, erps.take_ui_merges(), &name);
                }
                AssetOp::Warn { message } => built.warnings.push(format!("{}: {message}", op.origin)),
                _ => {}
            }
        }
        Ok(applied)
    }

    /// The checks and repairs over the archives as the ops left them: wheel positions, the tracks' ERS
    /// settings in other addons' handling files, the track menus kept, UI rules, texture arrays and
    /// textures, and stale size headers, which fail the build when they cannot be fixed.
    fn check_archives(
        &mut self,
        erps: &mut ErpBuild,
        erp_ops: &[Op],
        inputs: &Inputs,
        ers_maps: &[ErsMap],
        menu_kept: &Kept,
    ) -> Result<()> {
        let run = self.run;
        let name = |origin: &str| run.origin_name(origin);
        let packer = self.run.packer.clone();
        let Self { planner, built, .. } = self;
        built.warnings.extend(crate::diagnostics::phases::time_part(
            "build.check.wheels",
            || erps.wheel_warnings(),
        ));
        if !ers_maps.is_empty() {
            let edited = erps.edit_written(|rel, r| match ers::in_teams(rel) && r.kind() == "VTF" {
                true => ers::edit_vtf(r, ers_maps, &packer),
                false => Ok(None),
            })?;
            if edited > 0 {
                built.lines.push(format!(
                    "ers: {edited} handling file(s) other addons change get the tracks' settings too"
                ));
            }
        }
        let keeper = |h: &Held| {
            let codes = menu_kept.get(&(h.erp.clone(), h.resource.clone()))?;
            inputs
                .tracks
                .kept
                .iter()
                .filter(|(_, p, _)| codes.contains(&p.track_code))
                .map(|(_, _, o)| o)
                .max_by_key(|o| run.rank(o))
                .cloned()
        };
        let origin = |s: &str| {
            erp_ops
                .iter()
                .find(|op| op.origin.to_string() == s)
                .map(|op| op.origin.clone())
        };
        keep_menus(built, planner, erps.take_held(), &keeper, &origin, &name);
        for n in erps.take_notes() {
            built.lines.push(format!("erp: {n}"));
        }
        for w in crate::diagnostics::phases::time_part("build.check.ui", || erps.check_ui(&name))? {
            built.warnings.push(format!("ui: {w}"));
        }
        for line in crate::diagnostics::phases::time_part("build.check.arrays", || erps.cohere_arrays())? {
            built.lines.push(format!("erp: {line}"));
        }
        for (severity, w) in
            crate::diagnostics::phases::time_part("build.check.textures", || erps.check_textures(&name))
        {
            let kind = match severity {
                crate::content::texture_check::Severity::Invalid => "texture the game's renderer may reject",
                _ => "texture",
            };
            built.warnings.push(format!("{kind}: {w}"));
        }
        let stats = crate::diagnostics::phases::time_part("build.check.stats", || erps.repair_stats());
        if let Some((origin, files)) = by_origin(&stats.broken).first() {
            let who = name(origin);
            let fix = format!("Turn {who} off in this profile.");
            return Err(Error::Other(crate::erp::unrepairable_message(&who, files, &fix)));
        }
        for (origin, files) in by_origin(&stats.repaired) {
            built.lines.push(format!(
                "erp: {}",
                crate::erp::repaired_note(&name(&origin), &files)
            ));
        }
        Ok(())
    }

    /// Writes the changed archives. On the runtime route only those the in-game loader can't patch are
    /// built into the game files and the rest become runtime resources; else every changed archive is
    /// written, linked from the cache where an earlier build wrote it the same. The archives whose changes
    /// alter a stock layout.
    fn write_archives(
        &mut self,
        mut erps: ErpBuild,
        staged: &BTreeMap<String, PathBuf>,
        applied: Applied,
    ) -> Result<Vec<Resized>> {
        let run = self.run;
        let Self {
            planner,
            runtime,
            built,
            ..
        } = self;
        if let Some(note) = planner.route_note(Kind::Erp) {
            built.lines.push(format!("erp: {note}"));
        }
        let archives = run.store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
        if planner.route(Kind::Erp) != Route::Runtime {
            let resized = erps.resized();
            let (written, reused) = erps.write_cached(&built.overlay, Some(&archives), &|| run.cancel())?;
            if reused > 0 {
                built.lines.push(format!(
                    "erp: {reused} of {} archive(s) unchanged since an earlier build, linked from the cache",
                    written.len()
                ));
            }
            stage_erps(built, staged, written)?;
            return Ok(resized);
        }
        let crate::erp::BuiltIn {
            outside,
            resized,
            reused,
        } = erps.take_built_in(&built.overlay, Some(&archives), &|| run.cancel())?;
        for rel in &outside {
            built.lines.push(format!(
                "erp: {rel} built into the game files; the in-game loader patches only archives under 2025_asset_groups"
            ));
        }
        for r in &resized {
            let (uri, why) = &r.resources[0];
            built.lines.push(format!(
                "erp: {} built into the game files; {} resource(s) change their stock layout, which the in-game loader cannot load, such as {uri}: {why}",
                r.erp,
                r.resources.len()
            ));
        }
        let built_in: Vec<String> = resized.iter().map(|r| r.erp.clone()).chain(outside).collect();
        if reused > 0 {
            built.lines.push(format!(
                "erp: {reused} of the {} archive(s) built into the game files unchanged since an earlier build, linked from the cache",
                built_in.len()
            ));
        }
        let Applied { ranks, prebuilt } = applied;
        if !prebuilt.is_empty() {
            built.lines.push(format!(
                "erp: {} ready overlay archive(s) load as they are",
                prebuilt.len()
            ));
        }
        crate::now_doing("collecting the runtime resources");
        let notes = runtime.resources(erps.resources(), &ranks, &prebuilt, &built.overlay)?;
        built.warnings.extend(notes);
        stage_erps(built, staged, built_in)?;
        Ok(resized)
    }

    /// The archives addons put whole that the build wrote over with later changes.
    fn built_over(&self, puts: &Puts, staged: &BTreeMap<String, PathBuf>) -> BTreeSet<String> {
        let in_plan: BTreeSet<String> = self
            .built
            .plan
            .data
            .keys()
            .map(|r| r.to_ascii_lowercase())
            .collect();
        let staged_l: BTreeSet<String> = staged.keys().map(|r| r.to_ascii_lowercase()).collect();
        puts.keys()
            .filter(|rel| {
                let l = rel.to_ascii_lowercase();
                in_plan.contains(&l) && !staged_l.contains(&l)
            })
            .cloned()
            .collect()
    }
}

/// Whether an overlay archive loads as it is on the runtime route: its resource count, or why it goes in
/// resource by resource.
fn overlay_loads_whole(
    erps: &ErpBuild,
    path: &Path,
    at: usize,
    targets: &[String],
    slices: &[(usize, &str, &str)],
    contested: &BTreeSet<(String, String)>,
    base: &dyn Fn(&str) -> PathBuf,
) -> Result<std::result::Result<usize, String>> {
    let file = swofty_formats::erp::ErpFile::open(path)?;
    let stale = crate::erp::stale_stats_in(&file)?.len();
    let menus = file
        .erp
        .resources
        .iter()
        .any(|r| targets.iter().any(|t| erps.keeps(t, &r.name)));
    Ok(match merged_later(slices, at, targets, path)? {
        Some(why) => Err(why),
        None if menus => Err("it carries the track menu pictures".to_owned()),
        None if targets.iter().any(|t| !crate::erp::served_at_runtime(t)) => {
            Err("the in-game loader patches only archives under 2025_asset_groups".to_owned())
        }
        None if stale > 0 => Err(format!("{stale} resource(s) in it need their size headers fixed")),
        None if carries_handling(path, targets)? => {
            Err("it carries car handling files, which the game reads from the game file".to_owned())
        }
        None if let Some(uri) = contested_in(contested, path, targets)? => Err(format!(
            "another addon replaces {uri} whole too, and the two are merged node by node"
        )),
        None => overlay_fits(path, targets, base)?,
    })
}

/// The resources in `list`, grouped by the origin that wrote them, in the order the origins first come.
fn by_origin(list: &[(String, String)]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (origin, r) in list {
        match out.iter_mut().find(|(o, _)| o == origin) {
            Some((_, rs)) => rs.push(r.clone()),
            None => out.push((origin.clone(), vec![r.clone()])),
        }
    }
    out
}

fn stage_erps(built: &mut Built, staged: &BTreeMap<String, PathBuf>, rels: Vec<String>) -> Result<()> {
    for rel in rels {
        let src = built.overlay.join(&rel);
        if built.plan.data.insert(rel.clone(), src).is_some() && !staged.contains_key(&rel) {
            return Err(Error::Other(format!("{rel} is replaced twice")));
        }
    }
    Ok(())
}
