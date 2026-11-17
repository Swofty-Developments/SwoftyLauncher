use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

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
use self::puts::{after_puts, archive_puts};
use self::warnings::missing_resource;
use super::inputs::Inputs;
use super::{Built, Pipeline, source_dir};
use crate::activation::ers::{self, ErsMap};
use crate::activation::ui::Kept;
use crate::asset::{AssetOp, Op, Origin, Source};
use crate::content::runtime_assets::{Prebuilt, overlay_fits};
use crate::erpbuild::{ErpBuild, Held, reads, writes};
use crate::manifest::relative_inside;
use crate::planner::{Kind, Route};
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
        let (store, game) = (run.store, run.game);
        let Self {
            planner,
            runtime,
            built,
            session,
            ..
        } = self;
        let (fitted, hud_inputs, menus) = (&inputs.tracks, &inputs.hud_inputs, inputs.menus);
        let resolve = |rel: &str| run.original(rel);
        let cancel = || run.cancel();
        let file_ops = planner.take_files();
        let mut erp_ops = planner.take_erp();
        let rank = |o: &Origin| {
            run.order
                .iter()
                .position(|id| *id == o.addon)
                .unwrap_or(usize::MAX)
        };
        let puts = archive_puts(store, &file_ops, &erp_ops, &rank)?;
        for (rel, put) in &puts {
            planner.put_archive(rel, &put.origin);
        }
        erp_ops.retain_mut(|op| {
            let rank = rank(&op.origin);
            after_puts(op, &puts, rank)
        });
        let mut mipmaps = BTreeMap::new();
        let mut built_over = BTreeSet::new();
        let name = |origin: &str| run.origin_name(origin);
        for (id, folder, n) in stray_asset_groups(&game.dir, &file_ops) {
            built
                .warnings
                .push(stray_asset_group_line(&name(&id), &folder, n));
        }
        if !hud_inputs.is_empty() {
            let stock = crate::hud::GameStock::with_baseline(&game.dir, &store.root)
                .unwrap_or_else(|_| crate::hud::GameStock::new(&game.dir));
            let staged = built.plan.data.clone();
            let base = |rel: &str| {
                staged
                    .get(rel)
                    .or_else(|| puts.get(rel).map(|p| &p.path))
                    .cloned()
                    .unwrap_or_else(|| resolve(rel))
            };
            let mfd_rows = match menus {
                true => crate::ui::mfd_rows(session),
                false => Vec::new(),
            };
            let page = crate::ui::mfd_page_rows(&mfd_rows);
            let mfd_edit = |data: &[u8]| crate::ui::mfd::templates(data, &page);
            let put_names: Vec<String> = puts.keys().cloned().collect();
            let composed = crate::hud::integrate::compose_in_build(
                &crate::hud::integrate::Context {
                    store,
                    stock: &stock,
                    base: &base,
                    baseline: &resolve,
                    puts: &put_names,
                    name: &name,
                },
                &erp_ops,
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
            erp_ops.extend(composed.ops);
        }
        if !erp_ops.is_empty() || !puts.is_empty() {
            let staged = built.plan.data.clone();
            let base = |rel: &str| {
                staged
                    .get(rel)
                    .or_else(|| puts.get(rel).map(|p| &p.path))
                    .cloned()
                    .unwrap_or_else(|| resolve(rel))
            };
            let mut absent: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            erp_ops.retain(|op| {
                let AssetOp::Erp(e) = &op.op else {
                    return true;
                };
                let gone: Vec<&str> = reads(e)
                    .into_iter()
                    .filter(|rel| relative_inside(rel) && !base(rel).is_file())
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
                built.warnings.push(missing_archives(&name(id), rels));
            }
            for (rel, put) in &puts {
                if erp_ops
                    .iter()
                    .any(|op| matches!(&op.op, AssetOp::Erp(e) if writes(e).contains(&rel.as_str())))
                {
                    built.lines.push(format!(
                        "erp: {rel}: later changes apply to the whole archive {} puts",
                        put.origin
                    ));
                }
            }
            let mut erps = ErpBuild::new(&base).with_baseline(&resolve);
            for rel in puts.keys() {
                erps.load(rel)?;
            }
            let contested =
                crate::erpbuild::contested_ui(&erp_ops, &|op, s| source_dir(store, &op.origin.addon, s.root));
            for (erp, resource) in menu_kept.keys() {
                erps.keep(erp, resource);
            }
            let runtime_route = planner.route(Kind::Erp) == Route::Runtime;
            // Every origin that writes, in load order: the later wins.
            let mut ranks: Vec<String> = Vec::new();
            let mut prebuilt: Vec<Prebuilt> = Vec::new();
            let slices = slice_merges(&erp_ops);
            for (at, op) in erp_ops.iter().enumerate() {
                cancel()?;
                crate::now_doing(format!("applying {}", op.origin));
                let id = &op.origin.addon;
                let resolve = |s: &Source| Ok(s.resolve(&source_dir(store, id, s.root))?);
                let origin = op.origin.to_string();
                if matches!(op.op, AssetOp::Erp(_)) && !ranks.contains(&origin) {
                    ranks.push(origin.clone());
                }
                match &op.op {
                    AssetOp::Erp(crate::ops::ErpOp::Overlay {
                        erps: targets,
                        source,
                    }) if runtime_route => {
                        let path = resolve(source)?;
                        let file = swofty_formats::erp::ErpFile::open(&path)?;
                        let stale = crate::erpbuild::stale_stats_in(&file)?.len();
                        let menus = file
                            .erp
                            .resources
                            .iter()
                            .any(|r| targets.iter().any(|t| erps.keeps(t, &r.name)));
                        let fits = match merged_later(&slices, at, targets, &path)? {
                            Some(why) => Err(why),
                            None if menus => Err("it carries the track menu pictures".to_owned()),
                            None if targets.iter().any(|t| !crate::erpbuild::served_at_runtime(t)) => {
                                Err("the in-game loader patches only archives under 2025_asset_groups"
                                    .to_owned())
                            }
                            None if stale > 0 => {
                                Err(format!("{stale} resource(s) in it need their size headers fixed"))
                            }
                            None if carries_handling(&path, targets)? => Err(
                                "it carries car handling files, which the game reads from the game file"
                                    .to_owned(),
                            ),
                            None if let Some(uri) = contested_in(&contested, &path, targets)? => {
                                Err(format!(
                                    "another addon replaces {uri} whole too, and the two are merged node by node"
                                ))
                            }
                            None => overlay_fits(&path, targets, &base)?,
                        };
                        match fits {
                            Ok(resources) => prebuilt.push(Prebuilt {
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
                                    &crate::ops::ErpOp::Overlay {
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
            built.warnings.extend(erps.wheel_warnings());
            if !ers_maps.is_empty() {
                let edited = erps.edit_written(|rel, r| match ers::in_teams(rel) && r.kind() == "VTF" {
                    true => ers::edit_vtf(r, ers_maps),
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
                fitted
                    .kept
                    .iter()
                    .filter(|(_, p, _)| codes.contains(&p.track_code))
                    .map(|(_, _, o)| o)
                    .max_by_key(|o| rank(o))
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
            for w in erps.check_ui(&name)? {
                built.warnings.push(format!("ui: {w}"));
            }
            for line in erps.cohere_arrays()? {
                built.lines.push(format!("erp: {line}"));
            }
            for (severity, w) in erps.check_textures(&name) {
                let kind = match severity {
                    crate::content::texture_check::Severity::Invalid => {
                        "texture the game's renderer may reject"
                    }
                    _ => "texture",
                };
                built.warnings.push(format!("{kind}: {w}"));
            }
            let stats = erps.repair_stats();
            let by_origin = |list: &[(String, String)]| {
                let mut out: Vec<(String, Vec<String>)> = Vec::new();
                for (origin, r) in list {
                    match out.iter_mut().find(|(o, _)| o == origin) {
                        Some((_, rs)) => rs.push(r.clone()),
                        None => out.push((origin.clone(), vec![r.clone()])),
                    }
                }
                out
            };
            if let Some((origin, files)) = by_origin(&stats.broken).first() {
                let who = name(origin);
                let fix = format!("Turn {who} off in this profile.");
                return Err(Error::Other(crate::erpbuild::unrepairable_message(
                    &who, files, &fix,
                )));
            }
            for (origin, files) in by_origin(&stats.repaired) {
                built.lines.push(format!(
                    "erp: {}",
                    crate::erpbuild::repaired_note(&name(&origin), &files)
                ));
            }
            mipmaps = erps.take_mipmaps();
            if let Some(note) = planner.route_note(Kind::Erp) {
                built.lines.push(format!("erp: {note}"));
            }
            let resized = if planner.route(Kind::Erp) == Route::Runtime {
                let outside = erps.take_outside(&built.overlay)?;
                for rel in &outside {
                    built.lines.push(format!(
                        "erp: {rel} built into the game files; the in-game loader patches only archives under 2025_asset_groups"
                    ));
                }
                let resized = erps.take_resized(&built.overlay)?;
                for r in &resized {
                    let (uri, why) = &r.resources[0];
                    built.lines.push(format!(
                        "erp: {} built into the game files; {} resource(s) change their stock layout, which the in-game loader cannot load, such as {uri}: {why}",
                        r.erp,
                        r.resources.len()
                    ));
                }
                let built_in: Vec<String> = resized.iter().map(|r| r.erp.clone()).chain(outside).collect();
                if !prebuilt.is_empty() {
                    built.lines.push(format!(
                        "erp: {} ready overlay archive(s) load as they are",
                        prebuilt.len()
                    ));
                }
                let notes = runtime.resources(erps.resources(), &ranks, &prebuilt, &built.overlay)?;
                built.warnings.extend(notes);
                stage_erps(built, &staged, built_in)?;
                resized
            } else {
                let resized = erps.resized();
                let archives = store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
                let (written, reused) = erps.write_cached(&built.overlay, Some(&archives), &cancel)?;
                if reused > 0 {
                    built.lines.push(format!(
                        "erp: {reused} of {} archive(s) unchanged since an earlier build, linked from the cache",
                        written.len()
                    ));
                }
                stage_erps(built, &staged, written)?;
                resized
            };
            built.warnings.extend(stock_holders(&game.dir, &resized));
            let in_plan: BTreeSet<String> = built.plan.data.keys().map(|r| r.to_ascii_lowercase()).collect();
            let staged_l: BTreeSet<String> = staged.keys().map(|r| r.to_ascii_lowercase()).collect();
            built_over = puts
                .keys()
                .filter(|rel| {
                    let l = rel.to_ascii_lowercase();
                    in_plan.contains(&l) && !staged_l.contains(&l)
                })
                .cloned()
                .collect();
        }
        Ok(Archives {
            file_ops,
            mipmaps,
            built_over,
        })
    }
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
