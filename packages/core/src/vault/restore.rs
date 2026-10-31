//! Whole-profile restore from a vault snapshot. Steam-safe: it refuses while
//! F1 25 or Steam runs (the EA app for an EA copy), snapshots the current
//! save first, writes each file through a temporary name and checks its
//! hash, and never touches `remotecache.vdf`.

use std::path::PathBuf;

use super::{Event, EventKind, Layout, Location, Meta, Notice, Reason, Taken, Vault};
use crate::baseline::sha256_file;
use crate::{Error, Result, io_err};

pub const CLOSE_STEAM: &str = "Exit Steam completely (Steam menu > Exit), then restore again. Starting Steam \
                               afterwards uploads the restored save.";
pub const CLOSE_EA: &str = "Exit the EA app completely (its tray icon > Quit), then restore again.";

/// What runs that a restore must wait for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Running {
    pub game: bool,
    pub steam: bool,
    pub ea_app: bool,
}

impl Running {
    pub fn now() -> Self {
        Self {
            game: crate::process::game_running(),
            steam: crate::steam_client::status().state != crate::steam_client::State::Stopped,
            ea_app: crate::ea_app::running(),
        }
    }
}

/// Why a restore into a save of `layout` must wait.
pub fn blocker(layout: Layout, r: Running) -> Option<String> {
    let close = match layout {
        Layout::Steam => CLOSE_STEAM,
        Layout::Ea => CLOSE_EA,
    };
    if r.game {
        return Some(format!("F1 25 is running; close it first. {close}"));
    }
    match layout {
        Layout::Steam if r.steam => Some(format!(
            "Steam is running and would call the restored save corrupted. {CLOSE_STEAM}"
        )),
        Layout::Ea if r.ea_app => Some(format!("The EA app is running. {CLOSE_EA}")),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    pub snapshot: String,
    /// The snapshot holding the save the restore replaced.
    pub before: Option<String>,
    pub dir: PathBuf,
    /// Profile files in the save folder that the snapshot doesn't have; they
    /// stay as they are.
    pub kept: Vec<String>,
}

impl Restored {
    pub fn log_line(&self) -> String {
        format!("[vault] restored {} into {}", self.snapshot, self.dir.display())
    }
}

pub fn restore(vault: &Vault, id: &str, loc: &Location, running: Running, now: i64) -> Result<Restored> {
    if let Some(why) = blocker(loc.layout, running) {
        vault.log(
            &Event::new(now, EventKind::RestoreRefused)
                .snapshot(id)
                .detail(why.clone()),
        );
        return Err(Error::Other(why));
    }
    let _lock = vault.lock()?;
    let snap = vault.find(id)?;
    if snap.layout != loc.layout || snap.source != loc.source {
        return Err(Error::Other(format!(
            "snapshot {id} is a save of {}, not of {}",
            snap.source, loc.source
        )));
    }
    let mut plan = Vec::new();
    for f in &snap.files {
        let blob = vault.blob(&f.sha256);
        if !blob.is_file() || sha256_file(&blob)? != f.sha256 {
            return Err(Error::Other(format!(
                "snapshot {id} is damaged: {} does not match its manifest",
                f.name
            )));
        }
        let to = loc
            .path_of(&f.name)
            .ok_or_else(|| Error::Other(format!("snapshot {id} names an odd file, {}", f.name)))?;
        plan.push((blob, to, f));
    }
    let before = vault.snapshot_locked(
        loc,
        Meta::new(Reason::BeforeRestore)
            .pin("before-restore")
            .note(format!("before restoring {id}")),
        now,
    )?;
    let before = match before {
        Taken::New(s) | Taken::Unchanged(s) => Some(s.id),
        Taken::NoSave => None,
    };
    std::fs::create_dir_all(&loc.dir).map_err(io_err(&loc.dir))?;
    for (blob, to, f) in &plan {
        let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let tmp = loc.dir.join(format!(".{name}.swofty-restore"));
        std::fs::copy(blob, &tmp).map_err(io_err(&tmp))?;
        std::fs::rename(&tmp, to).map_err(io_err(to))?;
        if sha256_file(to)? != f.sha256 {
            return Err(Error::Other(format!("{} did not restore cleanly", to.display())));
        }
    }
    let kept = loc
        .files()?
        .into_iter()
        .filter(|f| !snap.files.iter().any(|s| s.name == f.key))
        .map(|f| f.key)
        .collect();
    let mut e = Event::new(now, EventKind::Restored).snapshot(id);
    e.from = before.clone();
    e.detail = Some(loc.dir.display().to_string());
    vault.log(&e);
    vault.snapshot_locked(
        loc,
        Meta::new(Reason::Restored).note(format!("restored from {id}")),
        now,
    )?;
    if Notice::read(&vault.data).is_some_and(|n| n.source == vault.source) {
        Notice::clear(&vault.data);
    }
    vault.prune_locked(now)?;
    Ok(Restored {
        snapshot: id.to_owned(),
        before,
        dir: loc.dir.clone(),
        kept,
    })
}
