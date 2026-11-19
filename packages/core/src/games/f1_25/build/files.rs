use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::archives::Archives;
use super::inputs::Inputs;
use super::run::Run;
use super::tracks::left_over;
use super::{Built, Pipeline, source_dir};
use crate::activation::ers::{self, ErsMap};
use crate::activation::handling::{self, HandlingMap};
use crate::asset::{AssetOp, FileOp, Op, Origin};
use crate::cache::archive_entry::ArchiveEntry;
use crate::game_folder::baseline::Baseline;
use crate::manifest::relative_inside;
use crate::{Error, Result};

/// Places the files `file.put` ops and the archive build's mip files put, each as a game file it replaces or
/// a file it adds; car archives get the tracks' handling and ERS settings on the way.
pub(super) fn put_files(
    built: &mut Built,
    run: Run,
    mipmaps: BTreeMap<String, PathBuf>,
    ops: Vec<Op>,
    built_over: &BTreeSet<String>,
    car_maps: (&[ErsMap], &[HandlingMap]),
) -> Result<BTreeSet<String>> {
    let (store, game) = (run.store, run.game);
    let packer = &run.eager_packer();
    let (ers_maps, handling_maps) = car_maps;
    let mut reached = BTreeSet::new();
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
        return Ok(reached);
    }
    let over: BTreeSet<String> = built_over.iter().map(|r| r.to_ascii_lowercase()).collect();
    let stock = |rel: &str| {
        baseline.as_ref().is_some_and(|b| b.files.contains_key(rel))
            || (game.dir.join(rel).is_file() && !left_over(store, game, rel))
    };
    let (mut replaced, mut added, mut handling) = (0, 0, 0);
    let addon_dir = |id: &str| store.addons_dir().join(id);
    let targets = handling::AddonTargets::new(handling_maps, &addon_dir);
    let edits = store.root.join("cache").join(crate::cache::ARCHIVES_DIR);
    for (_, (rel, mut src, origin)) in put {
        let lower = rel.to_ascii_lowercase();
        let group = targets.for_source(&src);
        reached.extend(group.iter().map(|m| m.id.clone()));
        let mut key = identity(&src);
        if !group.is_empty() && !over.contains(&lower) {
            let out = built.overlay.join("handling").join(&rel);
            let at = key.as_ref().map(|id| {
                let mut text = format!("{id}|{rel}");
                for m in &group {
                    text.push_str(&format!(
                        "|{}|{:?}|{:?}|{:?}|{:?}",
                        m.id, m.target, m.plan, m.resources, m.base
                    ));
                }
                edit_key("handling", &text)
            });
            let edit = |lines: &mut Vec<String>| handling::edit_archive(&src, &group, &out, lines, packer);
            if cached_edit(&edits, at.as_deref(), &out, &mut built.lines, edit)? > 0 {
                src = out;
                key = at;
            }
        }
        let car = !ers_maps.is_empty() && ers::is_car_archive(&rel);
        if car && !over.contains(&lower) {
            let out = built.overlay.join("ers").join(&rel);
            let at = key.as_ref().map(|k| {
                let mut text = format!("{k}|{rel}");
                for m in ers_maps {
                    let values = serde_json::to_string(m.values).expect("json");
                    text.push_str(&format!("|{}|{}|{values}", m.code, m.base));
                }
                edit_key("ers", &text)
            });
            let edit = |_: &mut Vec<String>| ers::edit_archive(&src, ers_maps, &out, packer);
            let n = cached_edit(&edits, at.as_deref(), &out, &mut built.lines, edit)?;
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
    Ok(reached)
}

pub(super) fn unshipped(
    maps: &[HandlingMap],
    reached: &BTreeSet<String>,
    liveries: &[handling::Livery],
) -> Vec<String> {
    maps.iter()
        .filter(|m| !reached.contains(&m.id) && !liveries.iter().any(|l| l.names(&m.target)))
        .filter_map(|m| match &m.target {
            handling::Target::Addon { addon, path } => Some(format!(
                "handling: {}: {path} in {addon} isn't put in the game folder by a file.put op and isn't a livery's \
                 archive; nothing applied",
                m.id
            )),
            handling::Target::Stock(_) => None,
        })
        .collect()
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
        let put = put_files(
            &mut self.built,
            self.run,
            mipmaps,
            file_ops,
            &built_over,
            (ers_maps, &inputs.handling_maps),
        )?;
        self.built
            .warnings
            .extend(unshipped(&inputs.handling_maps, &put, &inputs.liveries));
        Ok(())
    }
}

/// What the caches take as a file's contents: its path, length and modified time.
fn identity(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    Some(format!(
        "{}|{}|{:?}",
        path.display(),
        meta.len(),
        meta.modified().ok()?
    ))
}

const EDIT_KEY_VERSION: u32 = 1;

/// The archive cache key of a put file's `stage` edit, from `text`, which names its source and settings.
fn edit_key(stage: &str, text: &str) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(format!("put {stage} {EDIT_KEY_VERSION}|{text}"));
    crate::game_folder::baseline::hex(&digest)
}

/// Runs `edit`, which writes `out` and returns how many resources it changed, or links `out` and the lines
/// `edit` gave from the archive cache in `cache`, where an earlier build kept them under `key`. A record
/// beside the archive keeps the count and the lines; an edit that changed nothing keeps only the record.
/// Without a key, or without hard links, nothing is cached.
fn cached_edit(
    cache: &Path,
    key: Option<&str>,
    out: &Path,
    lines: &mut Vec<String>,
    edit: impl FnOnce(&mut Vec<String>) -> Result<usize>,
) -> Result<usize> {
    let Some(key) = key else {
        return edit(lines);
    };
    let entry = ArchiveEntry::at(cache, key);
    let record = entry.path().with_extension("json");
    let kept: Option<(usize, Vec<String>)> = std::fs::read(&record)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if let Some((n, kept)) = kept
        && (n == 0 || link(&entry, out))
    {
        crate::cache::mark_used(entry.path());
        lines.extend(kept);
        return Ok(n);
    }
    let mut new = Vec::new();
    let n = edit(&mut new)?;
    let dir = entry.path().parent().expect("has parent");
    if std::fs::create_dir_all(dir).is_ok() {
        std::fs::remove_file(entry.path()).ok();
        if n == 0 || entry.keep(out) {
            std::fs::write(&record, serde_json::to_vec(&(n, &new)).expect("json")).ok();
        }
    }
    lines.extend(new);
    Ok(n)
}

/// Links `out` to the archive `entry` keeps; false when there is none, it changed or it cannot link.
fn link(entry: &ArchiveEntry, out: &Path) -> bool {
    out.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && entry.link_to(out)
}
