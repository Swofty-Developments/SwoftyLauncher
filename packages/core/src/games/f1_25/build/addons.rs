use std::collections::BTreeMap;

use super::Built;
use super::buildhash::BuildHash;
use super::hooks::{self, Hooks};
use super::op_files::{core_intro, package_ops, refuse_by, refuse_placements, render_file, unbuilt};
use super::registry::added_tables;
use super::run::Run;
use crate::addons::builtin::{CORE_DB, CORE_ID, core_db_ops};
use crate::addons::registry::{Host, Registry};
use crate::asset::Features;
use crate::asset::{AssetOp, FileKind, Op, OpFile, Origin, Root, Source};
use crate::content::runtime_assets::Runtime;
use crate::manifest::AddonManifest;
use crate::online::multiplayer::fingerprint::{self, OutputBuilder};
use crate::planner::Planner;
use crate::stdlib::manifest::RegistryManifest;
use crate::{Error, Result, io_err};

/// What reading every addon's operations gave besides the ops it hands the planner.
#[derive(Default)]
pub(super) struct Gathered {
    /// What the build made of each addon, for its fingerprint.
    pub outputs: BTreeMap<String, fingerprint::Output>,
    /// The Driver Manager drivers each DB Editor package picks, by the package's name.
    pub driver_picks: Vec<(String, Vec<String>)>,
    pub hud_inputs: Vec<crate::hud::Input>,
    pub head_puts: crate::game_db::heads::Puts,
    /// An addon's operations file is applied by the core from its registry records.
    pub left_to_core: bool,
}

/// Reads the operations of every addon on, in load order, into the planner.
pub(super) struct Gather<'a, 'm, 'p> {
    pub run: Run<'a>,
    pub registry: &'a Registry,
    /// The build hooks started before the build read its inputs.
    pub hooks: &'m mut Hooks,
    pub features: &'a Features,
    /// The DB Editor packages on, by id, with their names.
    pub packages: &'a BTreeMap<String, String>,
    /// The registry manifests whose `replaced_operations` the core applies.
    pub replaced: &'a BTreeMap<&'a str, &'a RegistryManifest>,
    pub planner: &'m mut Planner<'p>,
    pub built: &'m mut Built,
    pub hash: &'m mut BuildHash,
}

/// The ops one addon gives, and what its fingerprint counts of them.
struct AddonOps {
    out: OutputBuilder,
    files: Vec<(FileKind, OpFile)>,
}

impl Gather<'_, '_, '_> {
    pub fn gather(mut self) -> Result<Gathered> {
        let mut gathered = Gathered::default();
        for id in self.run.order {
            self.addon(id, &mut gathered)?;
        }
        for input in &gathered.hud_inputs {
            for sprite in input.record.sprites.values() {
                let path = input.dir.join(&sprite.png);
                self.hash.update(std::fs::read(&path).map_err(io_err(&path))?);
            }
        }
        Ok(gathered)
    }

    fn addon(&mut self, id: &str, gathered: &mut Gathered) -> Result<()> {
        let run = self.run;
        let m = &run.addons[id];
        self.hash.update(format!("{id}@{};", m.version));
        let origin = Origin {
            addon: id.to_owned(),
            item: None,
        };
        let mut ops = AddonOps {
            out: OutputBuilder::default(),
            files: Vec::new(),
        };
        self.package(id, &mut ops, gathered);
        if id == CORE_ID {
            self.hash.update(CORE_DB);
            ops.files.push((FileKind::Database, core_db_ops()));
            let intro = core_intro(run.store, m, run.settings, run.on, self.hash)?;
            if !intro.ops.is_empty() {
                ops.files.push((FileKind::Assets, intro));
            }
        }
        self.op_files(m, &mut ops, gathered)?;
        let declared: Vec<AssetOp> = m.operations.file_ops().collect();
        self.hash.sources(run.store, id, &declared)?;
        if let Some(op) = unbuilt(ops.files.iter().flat_map(|(_, f)| &f.ops)) {
            self.built
                .warnings
                .push(format!("{id}: ERP operations skipped; {op} is not built yet"));
            for (_, f) in &mut ops.files {
                f.ops
                    .retain(|op| !matches!(op, AssetOp::Erp(_) | AssetOp::Warn { .. }));
            }
        }
        let roots = |root: Root| run.source_dir(id, root);
        let AddonOps { mut out, files } = ops;
        for (kind, file) in files {
            out.made(&format!("{kind:?}"), &file);
            for op in &file.ops {
                op.sources().into_iter().for_each(|src| out.reads(id, src));
                gathered.head_puts.op(id, op, &roots);
            }
            self.planner.push_file(&origin, kind, file);
        }
        for op in declared {
            out.made("declared", &op);
            op.sources().into_iter().for_each(|src| out.reads(id, src));
            gathered.head_puts.op(id, &op, &roots);
            self.planner.push(Op {
                op,
                origin: origin.clone(),
            });
        }
        self.hook(m, &mut out, gathered)?;
        self.screens(m, &mut out)?;
        self.hud(m, &mut out, gathered)?;
        let pending = m.operations.ui.len();
        if pending > 0 {
            self.built.warnings.push(format!(
                "{id}: {pending} UI operation file(s) skipped; that stage is not built yet"
            ));
        }
        if !run.store.is_builtin(id) {
            for n in &m.operations.native {
                out.made("native", n);
                for rel in std::iter::once(&n.dll).chain(&n.files) {
                    out.reads(id, &Source::addon(rel.trim_end_matches('/')));
                }
            }
        }
        gathered.outputs.insert(id.to_owned(), out.finish());
        Ok(())
    }

    /// A DB Editor package's ops come from its `dbedit.json`; one that can't be read is skipped.
    fn package(&mut self, id: &str, ops: &mut AddonOps, gathered: &mut Gathered) {
        let Some(name) = self.packages.get(id) else {
            return;
        };
        match package_ops(self.run.store, id) {
            Ok((file, picks)) => {
                self.hash.json(&file);
                ops.out.made("picks", &picks);
                ops.files.push((FileKind::Database, file));
                gathered.driver_picks.push((name.clone(), picks));
            }
            Err(e) => {
                self.hash.update(format!("unread {e};"));
                self.built.warnings.push(format!("{name}: package skipped: {e}"));
            }
        }
    }

    fn op_files(&mut self, m: &AddonManifest, ops: &mut AddonOps, gathered: &mut Gathered) -> Result<()> {
        let run = self.run;
        let id = m.id.as_str();
        let dir = run.addon_dir(id);
        let package = self.packages.contains_key(id);
        for (kind, rel) in m.operations.op_files() {
            if package && kind == FileKind::Database {
                continue;
            }
            if self.replaced.get(id).is_some_and(|r| r.replaces(rel)) {
                gathered.left_to_core = true;
                self.planner.add_core_rows(id, added_tables(&dir.join(rel)));
                self.hash.update(format!("replaced {rel};"));
                self.built.lines.push(format!(
                    "registry: {}: {rel} is applied by the core from the addon's registry records",
                    m.name
                ));
                continue;
            }
            let mut file = render_file(&dir.join(rel), kind, m, run.settings, run.on, self.hash)?;
            refuse_placements(id, rel, &mut file.ops, &mut self.built.warnings);
            self.hash.sources(run.store, id, &file.ops)?;
            ops.files.push((kind, file));
        }
        Ok(())
    }

    /// The ops the addon's build hook gives, for an addon whose ops aren't all declared.
    fn hook(&mut self, m: &AddonManifest, out: &mut OutputBuilder, gathered: &mut Gathered) -> Result<()> {
        let run = self.run;
        let id = m.id.as_str();
        let Some(entry) = self.registry.get(id).filter(|e| !e.declared) else {
            return Ok(());
        };
        let reply = match self.hooks.take(id, self.features) {
            Some(reply) => reply,
            None => {
                let host = Host::new(run.store, id, run.settings, Some(run.game))?.read_only(run.checking);
                let kept = super::inputs::hook_reply_path(run.store, run.profile, id);
                hooks::call(entry, &host, &hooks::options(run, id), self.features, &kept, id)
            }
        };
        let (batch, ran) = reply?;
        if let Some(ran) = ran {
            crate::diagnostics::phases::note(format!("hook {id} {ran}"));
        }
        self.hash.json(&batch.ops);
        if !batch.fields.is_empty() {
            self.hash.json(&batch.fields);
        }
        let ops: Vec<AssetOp> = batch.ops.iter().map(|op| op.op.clone()).collect();
        self.hash.sources(run.store, id, &ops)?;
        for line in &batch.lines {
            out.line(line);
        }
        self.built
            .lines
            .extend(batch.lines.iter().map(|l| format!("{id}: {l}")));
        self.built
            .warnings
            .extend(batch.warnings.iter().map(|w| format!("{id}: {w}")));
        let mut ops = batch.ops;
        refuse_by(id, "build hook", &mut ops, |o| &o.op, &mut self.built.warnings);
        if let Some(op) = unbuilt(ops.iter().map(|o| &o.op)) {
            self.built
                .warnings
                .push(format!("{id}: ERP operations skipped; {op} is not built yet"));
            ops.retain(|o| !matches!(o.op, AssetOp::Erp(_)));
        }
        out.made("fields", &batch.fields);
        self.planner.add_fields(id, batch.fields);
        for op in ops {
            out.made("hook", &op);
            op.op
                .sources()
                .into_iter()
                .for_each(|src| out.reads(&op.origin.addon, src));
            gathered.head_puts.op(&op.origin.addon, &op.op, &|root| {
                run.source_dir(&op.origin.addon, root)
            });
            self.planner.push(op);
        }
        Ok(())
    }

    /// The addon's own screens; a packaged screen that breaks the game's UI loader rules stops the build.
    fn screens(&mut self, m: &AddonManifest, out: &mut OutputBuilder) -> Result<()> {
        let id = m.id.as_str();
        let dir = self.run.addon_dir(id);
        for s in m.operations.screens.iter().filter(|s| s.page.is_none()) {
            out.reads(id, &Source::addon(s.file.clone()));
            let path = dir.join(&s.file);
            let data = std::fs::read(&path).map_err(io_err(&path))?;
            if swofty_formats::pkg::is_pkg(&data) {
                let (cat, engine) = (crate::ui::catalog::stock(), crate::ui::catalog::engine());
                let none = crate::ui::catalog::Scenes::default();
                let resource = format!("{id}/{}", s.file);
                let input = crate::ui::check::UiInput {
                    resource: &resource,
                    frag: "temp",
                    data: &data,
                    stock: None,
                };
                let report = crate::ui::check::ui_edit(&input, &none, cat, engine)?;
                if !report.hard.is_empty() {
                    let lines: Vec<String> = report.hard.iter().map(|h| h.to_string()).collect();
                    return Err(Error::Other(format!(
                        "{id}: screen {} breaks rules the game's UI loader enforces:\n  {}",
                        s.name,
                        lines.join("\n  ")
                    )));
                }
                self.built
                    .warnings
                    .extend(report.warnings.iter().map(|w| format!("{id}: ui: {w}")));
            }
            self.hash.update(&data);
        }
        Ok(())
    }

    fn hud(&mut self, m: &AddonManifest, out: &mut OutputBuilder, gathered: &mut Gathered) -> Result<()> {
        let id = m.id.as_str();
        let dir = self.run.addon_dir(id);
        for f in &m.operations.hud {
            out.reads(id, &Source::addon(f.clone()));
            let path = dir.join(f);
            self.hash.update(std::fs::read(&path).map_err(io_err(&path))?);
            gathered.hud_inputs.extend(crate::hud::inputs_of(id, &dir, f)?);
        }
        Ok(())
    }
}

/// Turns on in the runtime the experimental features the addons on list.
pub(super) fn experimental_features(
    run: Run,
    built: &mut Built,
    hash: &mut BuildHash,
    runtime: &mut Runtime,
) {
    let mut experimental: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for id in run.order {
        for f in &run.addons[id].experimental_features {
            experimental
                .entry(f.name())
                .or_default()
                .push(&run.addons[id].name);
        }
    }
    for (feature, by) in &experimental {
        runtime.enable(feature);
        hash.update(format!("feature {feature};"));
        built.lines.push(format!(
            "features: the experimental {feature} feature is on for {}",
            by.join(", ")
        ));
    }
}
