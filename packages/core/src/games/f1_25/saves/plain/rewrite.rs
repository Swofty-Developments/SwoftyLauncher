//! Maps the stat ids a career in the newest plain generation holds under rows that have since moved
//! (`swofty/id_aliases.json`) and hands the rewritten containers to the core as replace requests, which it
//! applies at load. The launcher never writes the game's encrypted save: the core replaces the loaded
//! container and the game saves it in its own form.

use std::collections::BTreeMap;
use std::io::Write;

use sha2::{Digest, Sha256};

use super::{Container, Event, Manifest, Plain, Source, inject, mode_of};
use crate::saves::bag;
use crate::saves::remap::{self, IdMaps, Remapped};
use crate::{Error, Result, io_err};

/// A save the game wrote more than this many seconds after the generation was written has moved on, so
/// the generation is not the career the game will load.
pub const STALE_AFTER: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CareerMapped {
    pub mode: String,
    pub slot: Option<u64>,
    pub counts: Remapped,
}

impl CareerMapped {
    pub fn line(&self) -> String {
        let id = self
            .slot
            .map_or_else(|| self.mode.clone(), |s| format!("{} ({})", s, self.mode));
        format!(
            "[saves] career {id}: mapped {} team, {} vehicle, {} driver id(s) to this build's rows",
            self.counts.teams, self.counts.vehicles, self.counts.drivers
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub generation: u64,
    pub from: u64,
    pub careers: Vec<CareerMapped>,
    pub requests: Vec<inject::Request>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Stale { generation: u64 },
    NoStore,
    Refused(String),
    Wrote(Rewrite),
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_err(name: &str, e: impl std::fmt::Display) -> Error {
    Error::Other(format!("plain container {name}: {e}"))
}

#[derive(Debug, Clone)]
pub struct Changed {
    pub bytes: Vec<u8>,
    pub base_sha256: String,
}

#[derive(Debug, Clone)]
pub struct Planned {
    pub manifest: Manifest,
    pub changed: BTreeMap<String, Changed>,
    pub careers: Vec<CareerMapped>,
    /// Career containers and slots (`<mode> career <slot id>`) that hold earlier driver or vehicle ids but no
    /// earlier team id, which are left alone: they are in the current numbering.
    pub skipped: Vec<String>,
}

const CHECKED: &str = "remap-checked.json";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CheckedMarker {
    generation: u64,
    maps: String,
}

pub struct CareerPass {
    pub changed: BTreeMap<String, bag::Bag>,
    pub careers: Vec<CareerMapped>,
    pub skipped: Vec<String>,
}

/// Maps `maps` in the career containers `parts` (name and decoded tree). A career container, and each slot of a
/// slot component, is mapped only if it holds an earlier team id: without one it is in the current numbering and
/// its other earlier ids are live rows of this build.
pub fn career_pass(parts: Vec<(String, bag::Bag)>, maps: &IdMaps) -> CareerPass {
    let mut changed: BTreeMap<String, bag::Bag> = BTreeMap::new();
    let mut gm_counts: BTreeMap<String, Remapped> = BTreeMap::new();
    let mut slot_counts: BTreeMap<String, Vec<(u64, Remapped)>> = BTreeMap::new();
    let mut active: BTreeMap<String, u64> = BTreeMap::new();
    let mut skipped = Vec::new();
    for (name, mut tree) in parts {
        let mode = mode_of(&name);
        if name.ends_with(super::manifest::SLOT_SUFFIX) {
            if let Some(id) = remap::active_slot(&tree) {
                active.insert(mode.clone(), id);
            }
            let (slots, held) = remap::remap_slots_gated(&mut tree, maps);
            skipped.extend(held.iter().map(|id| format!("{mode} career {id}")));
            if slots.is_empty() {
                continue;
            }
            slot_counts.insert(mode, slots);
        } else {
            let mut mapped = tree.clone();
            let done = remap::remap_container(&mut mapped, maps);
            if done.teams == 0 {
                if done.total() > 0 {
                    skipped.push(mode);
                }
                continue;
            }
            tree = mapped;
            gm_counts.insert(mode, done);
        }
        changed.insert(name, tree);
    }
    let careers = Plain::career_lines(&gm_counts, &slot_counts, &active);
    CareerPass {
        changed,
        careers,
        skipped,
    }
}

impl Plain {
    fn next_generation(&self) -> u64 {
        self.numbers()
            .first()
            .copied()
            .into_iter()
            .chain(self.current())
            .max()
            .map_or(1, |n| n + 1)
    }

    fn read_bag(&self, m: &Manifest, c: &Container) -> Result<Option<bag::Bag>> {
        let bytes =
            std::fs::read(self.bag_path(m.generation, c)).map_err(io_err(self.bag_path(m.generation, c)))?;
        match bag::decode(&bytes) {
            Ok(b) => Ok(Some(b)),
            Err(e) => Err(decode_err(&c.name, e)),
        }
    }

    /// What mapping `maps` would change in the newest generation, without writing anything.
    pub fn plan_rewrite(&self, maps: &IdMaps) -> Result<Option<Planned>> {
        let Some(m) = self.newest() else {
            return Ok(None);
        };
        let mut parts = Vec::new();
        for c in m
            .containers
            .iter()
            .filter(|c| remap::is_career_container(&c.name))
        {
            if let Some(tree) = self.read_bag(&m, c)? {
                parts.push((c.name.clone(), tree));
            }
        }
        let pass = career_pass(parts, maps);
        let mut changed = BTreeMap::new();
        for (name, tree) in &pass.changed {
            let bytes = bag::encode(tree).map_err(|e| decode_err(name, e))?;
            let base_sha256 = m.container(name).map(|c| c.sha256.clone()).unwrap_or_default();
            changed.insert(name.clone(), Changed { bytes, base_sha256 });
        }
        Ok(Some(Planned {
            manifest: m,
            changed,
            careers: pass.careers,
            skipped: pass.skipped,
        }))
    }

    fn checked_marker(&self, maps: &IdMaps) -> String {
        sha_hex(format!("{maps:?}").as_bytes())
    }

    /// Whether `generation` (0 for a store with none) is covered by the marker: a store checked or rewritten
    /// under these maps holds the current numbering from the marked generation on, so a later generation is
    /// never mapped again, whatever ids it holds.
    pub fn already_checked(&self, generation: u64, maps: &IdMaps) -> bool {
        crate::read_json::<CheckedMarker>(&self.root.join(CHECKED))
            .is_ok_and(|c| generation >= c.generation && c.maps == self.checked_marker(maps))
    }

    pub fn mark_checked(&self, generation: u64, maps: &IdMaps) {
        let marker = CheckedMarker {
            generation,
            maps: self.checked_marker(maps),
        };
        crate::write_json(&self.root.join(CHECKED), &marker).ok();
    }

    /// Careers in the newest generation that hold ids of `maps`, per slot, and the replace requests that put
    /// their containers back with this build's ids. A generation the game's own save has outrun
    /// (`save_written`, seconds since the epoch) is left alone. A generation checked once against the same
    /// maps is not decoded again.
    pub fn rewrite_for_aliases(&self, maps: &IdMaps, save_written: Option<i64>, now: i64) -> Result<Outcome> {
        if maps.is_empty() {
            return Ok(Outcome::Nothing);
        }
        let Some(newest) = self.newest() else {
            return Ok(match self.already_checked(0, maps) {
                true => Outcome::Nothing,
                false => Outcome::NoStore,
            });
        };
        if self.already_checked(newest.generation, maps) {
            return Ok(Outcome::Nothing);
        }
        if let Some(why) = maps.refusal() {
            self.mark_checked(newest.generation, maps);
            return Ok(Outcome::Refused(why));
        }
        let plan = self.plan_rewrite(maps)?;
        let Some(plan) = plan.filter(|p| !p.changed.is_empty()) else {
            self.mark_checked(newest.generation, maps);
            return Ok(Outcome::Nothing);
        };
        let m = &plan.manifest;
        if save_written.is_some_and(|w| w > m.written_at + STALE_AFTER) {
            return Ok(Outcome::Stale {
                generation: m.generation,
            });
        }
        let n = self.next_generation();
        self.write_rewritten(m, n, &plan.changed, now)?;
        self.pin(n, "rewrite")?;
        let mut requests = Vec::new();
        for (name, c) in &plan.changed {
            requests.push(inject::ask_based(
                &self.root,
                n,
                name,
                true,
                Some(c.base_sha256.clone()).filter(|h| !h.is_empty()),
                now,
            )?);
        }
        self.mark_checked(n, maps);
        Ok(Outcome::Wrote(Rewrite {
            generation: n,
            from: m.generation,
            careers: plan.careers,
            requests,
        }))
    }

    fn career_lines(
        gm: &BTreeMap<String, Remapped>,
        slots: &BTreeMap<String, Vec<(u64, Remapped)>>,
        active: &BTreeMap<String, u64>,
    ) -> Vec<CareerMapped> {
        let mut out = Vec::new();
        let modes: std::collections::BTreeSet<&String> = gm.keys().chain(slots.keys()).collect();
        for mode in modes {
            let mut per: BTreeMap<Option<u64>, Remapped> = BTreeMap::new();
            for (slot, counts) in slots.get(mode).into_iter().flatten() {
                per.insert(Some(*slot), *counts);
            }
            if let Some(counts) = gm.get(mode) {
                let at = per.entry(active.get(mode).copied()).or_default();
                at.teams += counts.teams;
                at.vehicles += counts.vehicles;
                at.drivers += counts.drivers;
            }
            for (slot, counts) in per {
                out.push(CareerMapped {
                    mode: mode.clone(),
                    slot,
                    counts,
                });
            }
        }
        out
    }

    fn write_rewritten(
        &self,
        m: &Manifest,
        n: u64,
        changed: &BTreeMap<String, Changed>,
        now: i64,
    ) -> Result<()> {
        let dir = self.generation_dir(n);
        let containers = dir.join("containers");
        std::fs::create_dir_all(&containers).map_err(io_err(&containers))?;
        let mut manifest = m.clone();
        manifest.generation = n;
        manifest.event = Event::Other;
        manifest.session = None;
        manifest.written_at = now;
        for c in &mut manifest.containers {
            let to = containers.join(c.file());
            match changed.get(&c.name) {
                Some(Changed { bytes, .. }) => {
                    std::fs::write(&to, bytes).map_err(io_err(&to))?;
                    c.bytes = bytes.len() as u64;
                    c.sha256 = sha_hex(bytes);
                    c.source = Source::Injected;
                }
                None => {
                    let from = self.bag_path(m.generation, c);
                    if std::fs::hard_link(&from, &to).is_err() {
                        std::fs::copy(&from, &to).map_err(io_err(&to))?;
                    }
                }
            }
        }
        let text = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::Other(e.to_string()))?;
        let path = dir.join("manifest.json");
        std::fs::write(&path, text).map_err(io_err(&path))?;
        let tmp = self.root.join("CURRENT.tmp");
        let mut f = std::fs::File::create(&tmp).map_err(io_err(&tmp))?;
        f.write_all(n.to_string().as_bytes()).map_err(io_err(&tmp))?;
        f.sync_all().map_err(io_err(&tmp))?;
        let current = self.root.join(super::CURRENT);
        std::fs::rename(&tmp, &current).map_err(io_err(&current))?;
        Ok(())
    }
}

/// The launcher's own save file to map when the plain store cannot be: the data file, and the snapshot the
/// vault took of it just before.
#[derive(Debug, Clone, Copy)]
pub struct SaveFile<'a> {
    pub path: &'a std::path::Path,
    pub snapshot: Option<&'a str>,
}

/// The log lines of a build's rewrite for `profile`: one per career mapped, or why nothing was. A launcher
/// save is checked against the generation; a session on the copy's own save has no file to compare.
pub fn prepare(
    data: &std::path::Path,
    profile: &str,
    on_launcher_saves: bool,
    aliases: &crate::row_ids::IdAliases,
    now: i64,
) -> Vec<String> {
    prepare_play(data, profile, on_launcher_saves, aliases, now, None)
}

fn mtime(path: &std::path::Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// [`prepare`] at Play. With `save` (the launcher's own save file, never Steam's), a store with no
/// generation, or one the save has outrun, maps the save file itself: the game loads it as it is.
pub fn prepare_play(
    data: &std::path::Path,
    profile: &str,
    on_launcher_saves: bool,
    aliases: &crate::row_ids::IdAliases,
    now: i64,
    save: Option<SaveFile<'_>>,
) -> Vec<String> {
    let maps = IdMaps::from_aliases(aliases);
    let Some(root) = super::root_of_session(data, profile, on_launcher_saves) else {
        return Vec::new();
    };
    let file = save.filter(|_| on_launcher_saves);
    let written = on_launcher_saves
        .then(|| {
            let path = file
                .map(|f| f.path.to_path_buf())
                .unwrap_or_else(|| crate::saves::launcher::remote_dir(data, profile).join(SAVE_FILE));
            mtime(&path)
        })
        .flatten();
    let plain = Plain::at(root);
    let outcome = plain.rewrite_for_aliases(&maps, written, now);
    let generation = plain.newest().map_or(0, |m| m.generation);
    let from_file = |why: &str| match file {
        Some(f) => {
            let done = map_save_file(f, &maps, true);
            if done.settled {
                plain.mark_checked(generation, &maps);
            }
            let mut lines = vec![format!("[saves] {why}; mapping the launcher save file")];
            lines.extend(done.lines);
            lines
        }
        None => vec![format!("[saves] {why}; not mapped")],
    };
    match outcome {
        Ok(Outcome::Nothing) => Vec::new(),
        Ok(Outcome::Refused(why)) => vec![format!(
            "[saves] earlier row ids are not mapped in the plain store: {why}"
        )],
        Ok(Outcome::Stale { generation }) => from_file(&format!(
            "careers in plain generation {generation} hold earlier row ids but the game saved after it"
        )),
        Ok(Outcome::NoStore) => from_file("the plain store has no generation yet"),
        Ok(Outcome::Wrote(r)) => r.careers.iter().map(CareerMapped::line).collect(),
        Err(e) => vec![format!(
            "[saves] could not map earlier row ids in the plain store: {e}"
        )],
    }
}

pub const SAVE_FILE: &str = "savegame@profile#NXDSMWW.BWW";
const SAVE_LEVEL: i32 = 3;

fn mapped_stream(stream: &[u8], maps: &IdMaps) -> std::result::Result<(Vec<u8>, CareerPass), String> {
    let mut root = bag::decode(stream).map_err(|e| format!("the save stream does not decode: {e}"))?;
    let parts = remap::save_containers(&root)
        .filter(|(name, _)| remap::is_career_container(name))
        .map(|(name, b)| (name, b.clone()))
        .collect();
    let pass = career_pass(parts, maps);
    if pass.changed.is_empty() {
        return Ok((Vec::new(), pass));
    }
    for (name, child) in remap::save_containers_mut(&mut root) {
        if let Some(tree) = pass.changed.get(&name) {
            *child = tree.clone();
        }
    }
    let bytes = bag::encode(&root).map_err(|e| format!("the mapped save stream does not encode: {e}"))?;
    Ok((bytes, pass))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub lines: Vec<String>,
    /// The file is in the build's numbering now, or never held the earlier one: the store is marked. A file
    /// that could not be read, decoded or written is tried again at the next Play.
    pub settled: bool,
}

/// Maps the careers in the launcher's save data file and writes it back atomically with the file's own IV
/// and modification time; the index does not depend on the data and stays. One line per career, or why the
/// file is left alone.
pub fn map_save_file(save: SaveFile<'_>, maps: &IdMaps, write: bool) -> FileResult {
    let alone = |why: String, settled: bool| FileResult {
        lines: vec![format!("[saves] launcher save file left alone: {why}")],
        settled,
    };
    let bytes = match std::fs::read(save.path) {
        Ok(b) => b,
        Err(e) => return alone(format!("{} is not readable ({e})", save.path.display()), false),
    };
    let decoded = match crate::saves::codec::decode_data(&bytes) {
        Ok(d) => d,
        Err(e) => return alone(format!("it does not decode ({e})"), false),
    };
    let (stream, pass) = match mapped_stream(&decoded.stream, maps) {
        Err(why) => return alone(why, false),
        Ok(x) => x,
    };
    let mut lines: Vec<String> = pass
        .skipped
        .iter()
        .map(|m| {
            format!("[saves] mode {m}: earlier driver or vehicle ids but no earlier team id; left alone")
        })
        .collect();
    if pass.changed.is_empty() {
        lines.push("[saves] launcher save file left alone: no career in it holds an earlier team id".into());
        return FileResult { lines, settled: true };
    }
    let written = match decoded.reseal(&stream, SAVE_LEVEL) {
        Ok(w) => w,
        Err(e) => return alone(format!("the mapped save does not encode ({e})"), false),
    };
    if crate::saves::codec::decode_data(&written)
        .map(|d| d.stream)
        .ok()
        .as_deref()
        != Some(stream.as_slice())
    {
        return alone("the mapped save does not read back".into(), false);
    }
    let original = std::fs::metadata(save.path).and_then(|m| m.modified()).ok();
    let tmp = save.path.with_extension("remap.tmp");
    let put = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&written)?;
        if let Some(t) = original {
            f.set_modified(t)?;
        }
        f.sync_all()?;
        std::fs::rename(&tmp, save.path)
    };
    if write && let Err(e) = put() {
        std::fs::remove_file(&tmp).ok();
        return alone(format!("it could not be written ({e})"), false);
    }
    let snapshot = save.snapshot.map_or(String::new(), |s| {
        format!(" (the vault snapshot before it is {s})")
    });
    lines.extend(pass.careers.iter().map(|c| {
        let id = c
            .slot
            .map_or_else(|| c.mode.clone(), |s| format!("{s} ({})", c.mode));
        format!(
            "[saves] career {id}: mapped {} team, {} vehicle, {} driver id(s) in the save file{snapshot}",
            c.counts.teams, c.counts.vehicles, c.counts.drivers
        )
    }));
    FileResult { lines, settled: true }
}

/// What `prepare` would do on `root` with `aliases`, as the log lines it would write, without writing
/// anything: the careers mapped, or the reason nothing is.
pub fn dry_run(root: &std::path::Path, aliases: &crate::row_ids::IdAliases) -> Result<Vec<String>> {
    let maps = IdMaps::from_aliases(aliases);
    if maps.is_empty() {
        return Ok(vec!["[saves] the alias file maps no ids".to_owned()]);
    }
    if let Some(why) = maps.refusal() {
        return Ok(vec![format!(
            "[saves] earlier row ids are not mapped in the plain store: {why}"
        )]);
    }
    let plain = Plain::at(root);
    let generation = plain.newest().map_or(0, |m| m.generation);
    if plain.already_checked(generation, &maps) {
        return Ok(vec![
            "[saves] the store was checked under these aliases already; nothing to map".to_owned(),
        ]);
    }
    let none = || vec!["[saves] no career in the newest plain generation holds an earlier row id".to_owned()];
    Ok(match Plain::at(root).plan_rewrite(&maps)? {
        None => none(),
        Some(plan) if plan.changed.is_empty() && plan.skipped.is_empty() => none(),
        Some(plan) => plan
            .careers
            .iter()
            .map(CareerMapped::line)
            .chain(plan.skipped.iter().map(|m| {
                format!("[saves] mode {m}: earlier driver or vehicle ids but no earlier team id; left alone")
            }))
            .collect(),
    })
}

#[cfg(test)]
mod tests;
