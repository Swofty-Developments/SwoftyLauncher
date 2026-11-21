//! The split of a profile's shared plain store. Before the EA layout launcher save had its own plain root, a session on
//! either copy wrote `saves/<profile>/plain`. Each generation's manifest names the exe that wrote it (`exe_sha256`), and
//! every build a core runs on is in `re/db/builds/`, so a generation an EA app build wrote is known by its manifest.
//!
//! ```text
//! saves/<profile>/plain/
//!   split.json                     the split ran: the EA app generations it set aside and whether it copied them
//!   split-ea/generations/<n>/      an EA app generation as the shared store held it, out of the Steam readers' view
//! saves/<profile>/ea-plain/
//!   generations/<n>/               a copy of it
//!   CURRENT                        the newest one copied
//! ```
//!
//! The split runs once per profile, with the session lock held and F1 25 closed. It copies before it sets anything aside
//! and writes `split.json` last, so a split that stopped half way runs again from where it stopped. A generation no
//! known build wrote stays in the shared store, which stays the Steam layout's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{CURRENT, GENERATIONS, Manifest, Plain};
use crate::{Result, io_err};

pub const SPLIT: &str = "split.json";
pub const SPLIT_ASIDE: &str = "split-ea";
const COPYING: &str = "split.copying";

/// `F1_25.exe` of the EA app builds a core runs on (`re/db/builds/ea.json`, `ea-1-0-145.json`).
const EA_EXES: &[&str] = &[
    "cc506879c1c834aa54d3478b2f2133d2ec243f2f2d313397631145ea618ff7f4",
    "8361a266d7aa1f43bdb35da89ccfe66010901e43dddd44b6d5cb8c9f090a99f7",
];
/// `F1_25.exe` of the Steam builds a core runs on (`re/db/builds/steam.json`, `steam-26566308.json`).
const STEAM_EXES: &[&str] = &[
    "d873a1c8d2044cef2fd76c99c9d5a97649c5b815d055540995105f46619865ae",
    "46009bd31f49d93a92d73cbd0b66131d2b37964a92da7293168d3c3a872d9389",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    Steam,
    Ea,
    Unknown,
}

pub fn writer(m: &Manifest) -> Writer {
    let exe = m.exe_sha256.to_ascii_lowercase();
    match (
        EA_EXES.contains(&exe.as_str()),
        STEAM_EXES.contains(&exe.as_str()),
    ) {
        (true, _) => Writer::Ea,
        (_, true) => Writer::Steam,
        _ => Writer::Unknown,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
    pub schema: u32,
    /// The EA app generations set aside under `split-ea/`.
    pub ea: Vec<u64>,
    /// The EA layout's root got a copy of each. Not when a core had written that root first: its own generations
    /// are newer than any set aside.
    pub copied: bool,
    /// Generations no known build wrote. They stay in the shared store.
    pub unknown: Vec<u64>,
}

fn aside(shared: &Plain) -> Plain {
    Plain::at(shared.root.join(SPLIT_ASIDE))
}

/// The manifest of EA app generation `n` where the split finds it: still in the shared store, or set aside.
fn ea_manifest(shared: &Plain, n: u64) -> Option<(Manifest, PathBuf)> {
    [shared.clone(), aside(shared)]
        .into_iter()
        .find_map(|p| Some((p.manifest(n)?, p.generation_dir(n))))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(io_err(to))?;
    for e in std::fs::read_dir(from).map_err(io_err(from))?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        match src.is_dir() {
            true => copy_tree(&src, &dst)?,
            false => {
                std::fs::copy(&src, &dst).map_err(io_err(&dst))?;
                crate::sync_file(&dst)?;
            }
        }
    }
    crate::sync_dir(Some(to));
    Ok(())
}

/// Copy a generation folder into `to` whole: into `split.copying/` first, then renamed into `generations/`.
fn copy_generation(from: &Path, to: &Plain, n: u64) -> Result<()> {
    let tmp = to.root.join(COPYING).join(format!("{n:012}"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(io_err(&tmp))?;
    }
    copy_tree(from, &tmp)?;
    let dst = to.generation_dir(n);
    std::fs::create_dir_all(to.root.join(GENERATIONS)).map_err(io_err(&dst))?;
    std::fs::rename(&tmp, &dst).map_err(io_err(&dst))?;
    crate::sync_dir(dst.parent());
    std::fs::remove_dir(to.root.join(COPYING)).ok();
    Ok(())
}

/// Split the shared plain store of `profile`: copy the EA app generations into the EA layout's root, then set them
/// aside in the shared store. `None` when there is nothing to split or the split already ran. The caller holds the
/// session lock and has checked that F1 25 is closed.
pub fn split_profile(data: &Path, profile: &str) -> Result<Option<Split>> {
    let shared = Plain::at(super::root_of_profile(data, profile));
    let marker = shared.root.join(SPLIT);
    if marker.is_file() || !shared.root.join(CURRENT).is_file() {
        return Ok(None);
    }
    let set_aside = aside(&shared);
    let mut ea: BTreeSet<u64> = set_aside
        .numbers()
        .into_iter()
        .filter(|n| set_aside.manifest(*n).is_some())
        .collect();
    let mut unknown = Vec::new();
    for m in shared.generations() {
        match writer(&m) {
            Writer::Ea => {
                ea.insert(m.generation);
            }
            Writer::Unknown => unknown.push(m.generation),
            Writer::Steam => {}
        }
    }
    unknown.sort_unstable();
    let ea_root = Plain::at(super::root_of_profile_ea(data, profile));
    let copied = !ea.is_empty()
        && ea_root.numbers().into_iter().all(|n| {
            ea_root
                .manifest(n)
                .is_some_and(|own| ea_manifest(&shared, n).is_some_and(|(m, _)| m == own))
        });
    if copied {
        for n in &ea {
            if ea_root.manifest(*n).is_some() {
                continue;
            }
            if let Some((_, from)) = ea_manifest(&shared, *n) {
                copy_generation(&from, &ea_root, *n)?;
            }
        }
        if let Some(top) = ea.last().copied()
            && ea_root.current().is_none_or(|c| c < top)
        {
            crate::write_json(&ea_root.root.join(CURRENT), &top)?;
        }
    }
    for n in &ea {
        let from = shared.generation_dir(*n);
        if !from.is_dir() {
            continue;
        }
        let to = set_aside.generation_dir(*n);
        std::fs::create_dir_all(set_aside.root.join(GENERATIONS)).map_err(io_err(&to))?;
        std::fs::rename(&from, &to).map_err(io_err(&to))?;
        crate::sync_dir(to.parent());
    }
    let split = Split {
        schema: 1,
        ea: ea.into_iter().collect(),
        copied,
        unknown,
    };
    crate::write_json(&marker, &split)?;
    Ok(Some(split))
}

fn split_line(data: &Path, profile: &str, s: &Split) -> String {
    let shared = super::root_of_profile(data, profile);
    let ea_root = super::root_of_profile_ea(data, profile);
    let copy = match s.copied {
        true => format!("{} has a copy of each", ea_root.display()),
        false => format!(
            "{} has generations of its own and gets no copy",
            ea_root.display()
        ),
    };
    let unknown = match s.unknown.is_empty() {
        true => String::new(),
        false => format!(
            "; {} generation(s) from a build the launcher does not know stay in the Steam layout's store",
            s.unknown.len()
        ),
    };
    format!(
        "saves: split the plain store of profile {profile}: {} generation(s) the EA app copy wrote ({}) are set aside \
         in {}; {copy}{unknown}",
        s.ea.len(),
        s.ea.iter().map(u64::to_string).collect::<Vec<_>>().join(", "),
        shared.join(SPLIT_ASIDE).display(),
    )
}

/// Split the shared plain store of every profile under `data`. The caller holds the session lock and has checked
/// that F1 25 is closed. Lines for the log.
pub fn split_all_held(data: &Path) -> Vec<String> {
    let profiles: Vec<String> = std::fs::read_dir(crate::saves::launcher::root(data))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    let mut out = Vec::new();
    for profile in profiles {
        match split_profile(data, &profile) {
            Ok(Some(s)) if !s.ea.is_empty() => out.push(split_line(data, &profile, &s)),
            Ok(_) => {}
            Err(e) => out.push(format!(
                "saves: could not split the plain store of profile {profile} between the Steam and EA app copies: {e}"
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests;
