//! Maps the stat ids a career in the newest plain generation holds under rows that have since moved
//! (`swofty/id_aliases.json`) and hands the rewritten containers to the core as replace requests, which it
//! applies at load. The launcher never writes the game's encrypted save: the core replaces the loaded
//! container and the game saves it in its own form.

use std::collections::BTreeMap;
use std::io::Write;

use sha2::{Digest, Sha256};

use super::{Container, Event, Manifest, Plain, Source, inject};
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
    Wrote(Rewrite),
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_err(name: &str, e: impl std::fmt::Display) -> Error {
    Error::Other(format!("plain container {name}: {e}"))
}

struct Changed {
    bytes: Vec<u8>,
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

    /// Careers in the newest generation that hold ids of `maps`, per slot, and the replace requests that put
    /// their containers back with this build's ids. A generation the game's own save has outrun
    /// (`save_written`, seconds since the epoch) is left alone.
    pub fn rewrite_for_aliases(&self, maps: &IdMaps, save_written: Option<i64>, now: i64) -> Result<Outcome> {
        if maps.is_empty() {
            return Ok(Outcome::Nothing);
        }
        let Some(m) = self.newest() else {
            return Ok(Outcome::Nothing);
        };
        let mut changed: BTreeMap<String, Changed> = BTreeMap::new();
        let mut gm_counts: BTreeMap<String, Remapped> = BTreeMap::new();
        let mut slot_counts: BTreeMap<String, Vec<(u64, Remapped)>> = BTreeMap::new();
        let mut active: BTreeMap<String, u64> = BTreeMap::new();
        for c in m
            .containers
            .iter()
            .filter(|c| remap::is_career_container(&c.name))
        {
            let Some(mut tree) = self.read_bag(&m, c)? else {
                continue;
            };
            let mode = c.mode();
            if c.name.ends_with(super::manifest::SLOT_SUFFIX) {
                if let Some(id) = remap::active_slot(&tree) {
                    active.insert(mode.clone(), id);
                }
                let slots = remap::remap_slots(&mut tree, maps);
                if slots.is_empty() {
                    continue;
                }
                slot_counts.insert(mode, slots);
            } else {
                let done = remap::remap_container(&mut tree, maps);
                if done.total() == 0 {
                    continue;
                }
                gm_counts.insert(mode, done);
            }
            let bytes = bag::encode(&tree).map_err(|e| decode_err(&c.name, e))?;
            changed.insert(c.name.clone(), Changed { bytes });
        }
        if changed.is_empty() {
            return Ok(Outcome::Nothing);
        }
        if save_written.is_some_and(|w| w > m.written_at + STALE_AFTER) {
            return Ok(Outcome::Stale {
                generation: m.generation,
            });
        }
        let careers = Self::career_lines(&gm_counts, &slot_counts, &active);
        let n = self.next_generation();
        self.write_rewritten(&m, n, &changed, now)?;
        self.pin(n, "rewrite")?;
        let mut requests = Vec::new();
        for name in changed.keys() {
            requests.push(inject::ask(&self.root, n, name, true, now)?);
        }
        Ok(Outcome::Wrote(Rewrite {
            generation: n,
            from: m.generation,
            careers,
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
                Some(Changed { bytes }) => {
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

/// The log lines of a Play's rewrite for `profile`: one per career mapped, or why nothing was. A launcher
/// save is checked against the generation; a session on the copy's own save has no file to compare.
pub fn prepare(
    data: &std::path::Path,
    profile: &str,
    on_launcher_saves: bool,
    aliases: &crate::row_ids::IdAliases,
    now: i64,
) -> Vec<String> {
    let maps = IdMaps::from_aliases(aliases);
    let Some(root) = super::root_of_session(data, profile, on_launcher_saves) else {
        return Vec::new();
    };
    let written = on_launcher_saves
        .then(|| {
            let file = crate::saves::launcher::remote_dir(data, profile).join("savegame@profile#NXDSMWW.BWW");
            let modified = std::fs::metadata(file).ok()?.modified().ok()?;
            Some(modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
        })
        .flatten();
    match Plain::at(root).rewrite_for_aliases(&maps, written, now) {
        Ok(Outcome::Nothing) => Vec::new(),
        Ok(Outcome::Stale { generation }) => vec![format!(
            "[saves] careers in plain generation {generation} hold earlier row ids but the game saved after it; not mapped"
        )],
        Ok(Outcome::Wrote(r)) => r.careers.iter().map(CareerMapped::line).collect(),
        Err(e) => vec![format!(
            "[saves] could not map earlier row ids in the plain store: {e}"
        )],
    }
}

#[cfg(test)]
mod tests;
