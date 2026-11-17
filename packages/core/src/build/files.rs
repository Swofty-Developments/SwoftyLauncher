use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::archives::Archives;
use super::inputs::Inputs;
use super::tracks::left_over;
use super::{Built, Pipeline, source_dir};
use crate::activation::ers::{self, ErsMap};
use crate::activation::handling::{self, HandlingMap};
use crate::asset::{AssetOp, FileOp, Op, Origin};
use crate::baseline::Baseline;
use crate::install::Game;
use crate::manifest::relative_inside;
use crate::store::Store;
use crate::{Error, Result};

/// Places the files `file.put` ops and the archive build's mip files put, each as a game file it replaces or
/// a file it adds; car archives get the tracks' handling and ERS settings on the way.
pub(super) fn put_files(
    built: &mut Built,
    store: &Store,
    game: &Game,
    mipmaps: BTreeMap<String, PathBuf>,
    ops: Vec<Op>,
    built_over: &BTreeSet<String>,
    car_maps: (&[ErsMap], &[HandlingMap]),
) -> Result<()> {
    let (ers_maps, handling_maps) = car_maps;
    let baseline = Baseline::load_or_new(&store.root, &game.identity).ok();
    let mut spelling: BTreeMap<String, String> = BTreeMap::new();
    for rel in built.plan.data.keys().chain(built.plan.added.keys()) {
        spelling.insert(rel.to_ascii_lowercase(), rel.clone());
    }
    if let Some(b) = &baseline {
        for rel in b.files.keys() {
            spelling.insert(rel.to_ascii_lowercase(), rel.clone());
        }
    }
    let mut put: BTreeMap<String, (String, PathBuf, Option<Origin>)> = BTreeMap::new();
    for (rel, src) in mipmaps {
        let key = rel.to_ascii_lowercase();
        let real = spelling.entry(key.clone()).or_insert(rel).clone();
        put.insert(key, (real, src, None));
    }
    let mips = put.len();
    for op in ops {
        let AssetOp::File(FileOp::Put { source, target }) = op.op else {
            continue;
        };
        if !relative_inside(&target) {
            return Err(Error::Resolve(format!(
                "{target} must be a relative path inside the game folder"
            )));
        }
        let src = source.resolve(&source_dir(store, &op.origin.addon, source.root))?;
        let key = target.to_ascii_lowercase();
        let real = spelling.entry(key.clone()).or_insert(target).clone();
        if let Some((_, _, Some(first))) = put.get(&key)
            && *first != op.origin
        {
            built.lines.push(format!(
                "files: {real} is put by both {first} and {}; {} wins",
                op.origin, op.origin
            ));
        }
        put.insert(key, (real, src, Some(op.origin)));
    }
    if put.is_empty() {
        return Ok(());
    }
    let over: BTreeSet<String> = built_over.iter().map(|r| r.to_ascii_lowercase()).collect();
    let stock = |rel: &str| {
        baseline.as_ref().is_some_and(|b| b.files.contains_key(rel))
            || (game.dir.join(rel).is_file() && !left_over(store, game, rel))
    };
    let (mut replaced, mut added, mut handling) = (0, 0, 0);
    let addon_dir = |id: &str| store.addons_dir().join(id);
    for (_, (rel, mut src, origin)) in put {
        let lower = rel.to_ascii_lowercase();
        let group = handling::for_source(handling_maps, &addon_dir, &src);
        if !group.is_empty() && !over.contains(&lower) {
            let out = built.overlay.join("handling").join(&rel);
            let mut lines = Vec::new();
            if handling::edit_archive(&src, &group, &out, &mut lines)? > 0 {
                src = out;
            }
            built.lines.extend(lines);
        }
        let car = !ers_maps.is_empty() && ers::is_car_archive(&rel);
        if car && !over.contains(&lower) {
            let out = built.overlay.join("ers").join(&rel);
            let n = ers::edit_archive(&src, ers_maps, &out)?;
            if n > 0 {
                handling += n;
                src = out;
            }
        }
        let is_stock = stock(&rel);
        if is_stock {
            replaced += 1;
        } else {
            added += 1;
        }
        if over.contains(&lower) {
            if !is_stock && let Some(archive) = built.plan.data.remove(&rel) {
                built.plan.added.insert(rel, archive);
            }
            continue;
        }
        let into = match is_stock {
            true => &mut built.plan.data,
            false => &mut built.plan.added,
        };
        if into.insert(rel.clone(), src).is_some() && !car {
            return Err(Error::Other(match origin {
                Some(o) => format!(
                    "{rel} is replaced twice: {o} puts the whole file, and an added track changes it too"
                ),
                None => format!("{rel} is replaced twice"),
            }));
        }
    }
    built
        .lines
        .push(format!("files: {replaced} game file(s) replaced, {added} added"));
    if handling > 0 {
        built.lines.push(format!(
            "ers: {handling} handling file(s) in car archives addons put get the tracks' settings"
        ));
    }
    if mips > 0 {
        built
            .lines
            .push(format!("mipmaps: {mips} mip file(s) go in the game folder"));
    }
    Ok(())
}

impl Pipeline<'_, '_> {
    pub(super) fn place_files(
        &mut self,
        inputs: &Inputs,
        archives: Archives,
        ers_maps: &[ErsMap],
    ) -> Result<()> {
        let Archives {
            file_ops,
            mipmaps,
            built_over,
        } = archives;
        put_files(
            &mut self.built,
            self.run.store,
            self.run.game,
            mipmaps,
            file_ops,
            &built_over,
            (ers_maps, &inputs.handling_maps),
        )
    }
}
