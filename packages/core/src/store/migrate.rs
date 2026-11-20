//! Moving an older launcher's data into this one's layout, and listing in each profile the installed addons
//! it lacks.

use std::collections::BTreeSet;

use super::config::warn;
use super::scan::{manifest_id, subdirs};
use super::{DATA_VERSION, LEGACY_ADDONS_DIR, Store};
use crate::addons::builtin::CORE_ID;
use crate::manifest::{AddonManifest, MANIFEST_FILE};
use crate::profile;
use crate::session::Config;
use crate::{Error, Result, io_err};

impl Store {
    pub fn migrate(&self) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        self.refuse_dangling_root()?;
        if !self.root.is_dir() {
            return Ok(notes);
        }
        notes.extend(self.finish_swaps());
        notes.extend(self.sweep_unfinished());
        notes.extend(crate::catalog::pending::settle(self));
        let config = self.read_config();
        if config.as_ref().is_ok_and(|c| c.data_version >= DATA_VERSION) {
            return Ok(notes);
        }
        notes.extend(self.move_legacy_addons()?);
        let from = config.as_ref().map_or(0, |c| c.data_version);
        notes.extend(self.migrate_builtins(from)?);
        let backfilled = self.backfill(&config, from, &mut notes);
        match config {
            Ok(_) if !backfilled => {}
            Ok(mut config) => {
                config.data_version = DATA_VERSION;
                self.save_config(&config)?;
            }
            Err(e) => notes.push(format!("data_version not recorded: {e}")),
        }
        Ok(notes)
    }

    /// Refuses a data folder that is a link to a folder that is gone, so a
    /// drive that is not connected never gets a new, empty data folder.
    pub(super) fn refuse_dangling_root(&self) -> Result<()> {
        match crate::data::paths::dangling_link(&self.root) {
            Some(target) => Err(Error::Other(format!(
                "the data folder {} links to {}, which is missing. Reconnect that drive or restore the folder; the launcher will not start a new data folder in its place",
                self.root.display(),
                target.display()
            ))),
            None => Ok(()),
        }
    }

    /// The launcher config as a migration reads it: the defaults when it is
    /// missing or damaged, and an error when it can't be read.
    pub(super) fn read_config(&self) -> Result<Config> {
        match crate::load_json(&self.config_path()) {
            Ok(crate::Loaded::Found(c)) => Ok(c),
            Ok(crate::Loaded::Missing | crate::Loaded::Damaged(_)) => Ok(Config::default()),
            Err(e) => Err(e),
        }
    }

    /// Moves an older launcher's addons folder into place: whole when there
    /// is no addons folder yet, else each addon this one doesn't have.
    pub(super) fn move_legacy_addons(&self) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        let old = self.root.join(LEGACY_ADDONS_DIR);
        let new = self.addons_dir();
        if old.is_dir() && !new.exists() {
            std::fs::rename(&old, &new).map_err(io_err(&old))?;
            notes.push(format!("moved {} to {}", old.display(), new.display()));
        } else if old.is_dir() {
            let taken: BTreeSet<String> = subdirs(&new)?.iter().filter_map(|d| manifest_id(d)).collect();
            for dir in subdirs(&old)? {
                let to = new.join(dir.file_name().expect("has a name"));
                match manifest_id(&dir) {
                    Some(id) if !taken.contains(&id) && !to.exists() => {
                        std::fs::rename(&dir, &to).map_err(io_err(&dir))?;
                        notes.push(format!("moved {} to {}", dir.display(), to.display()));
                    }
                    Some(id) => notes.push(format!(
                        "kept {}: {} already has {id}",
                        dir.display(),
                        new.display()
                    )),
                    None => notes.push(format!(
                        "kept {}: no addon id in its {MANIFEST_FILE}",
                        dir.display()
                    )),
                }
            }
            if std::fs::remove_dir(&old).is_ok() {
                notes.push(format!("removed the empty {}", old.display()));
            }
        }
        Ok(notes)
    }

    /// Runs each built-in addon's own migration from data version `from`.
    pub(super) fn migrate_builtins(&self, from: u32) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        for b in self.builtins.iter() {
            let id = &b.addon.manifest().id;
            match b.addon.migrate(&self.root, from) {
                Ok(n) => notes.extend(n.into_iter().map(|n| format!("{id}: {n}"))),
                Err(e) => return Err(Error::Other(format!("migrating {id}: {e}"))),
            }
        }
        Ok(notes)
    }

    /// Lists in each profile the installed addons it lacks, when the config
    /// was read. Whether that is done; a failure is warned about and noted,
    /// and the next start tries again.
    pub(super) fn backfill(&self, config: &Result<Config>, from: u32, notes: &mut Vec<String>) -> bool {
        match config
            .as_ref()
            .map_or(Ok(Vec::new()), |_| self.backfill_profile_addons(from))
        {
            Ok(n) => {
                notes.extend(n);
                true
            }
            Err(e) => {
                let text = format!("couldn't list installed addons in the profiles, will retry: {e}");
                warn(text.clone());
                notes.push(text);
                false
            }
        }
    }

    /// Data version 4: a profile lists every installed addon, as the Addons page showed
    /// them before profiles listed their own. Each one a profile doesn't list is added off,
    /// at the end, in the order that page showed them; the profile's own entries stay as
    /// they are.
    pub(super) fn backfill_profile_addons(&self, from: u32) -> Result<Vec<String>> {
        let mut notes = Vec::new();
        if from >= 4 {
            return Ok(notes);
        }
        let mut installed: Vec<AddonManifest> = self
            .addons()?
            .into_values()
            .filter(|m| {
                !m.is_builtin()
                    && !self.is_builtin(&m.id)
                    && m.id != crate::updates::managed::ID
                    && m.id != CORE_ID
            })
            .collect();
        installed.sort_by_cached_key(|m| m.name.to_lowercase());
        if installed.is_empty() {
            return Ok(notes);
        }
        for name in self.profile_names()? {
            if name == profile::VANILLA {
                continue;
            }
            let added = self.update_profile_if(&name, |p| {
                let mut added = Vec::new();
                for m in &installed {
                    if !p.has(&m.id) {
                        p.add_addon(&m.id)?;
                        added.push(m.id.clone());
                    }
                }
                Ok(added)
            })?;
            if !added.is_empty() {
                notes.push(format!(
                    "listed {} installed addons in the {name} profile",
                    added.len()
                ));
            }
        }
        Ok(notes)
    }
}
