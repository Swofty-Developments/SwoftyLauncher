//! The profiles: listing, creating, renaming and deleting them, the active one, and their changes written one
//! at a time.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::config::warn_once;
use super::{DEFAULT_PROFILE, Store};
use crate::addons::builtin::CORE_ID;
use crate::profile::{self, Profile};
use crate::{Error, Result, io_err, with_retry};

pub(super) static PROFILE_WRITE: Mutex<()> = Mutex::new(());

pub(super) fn profile_write() -> MutexGuard<'static, ()> {
    PROFILE_WRITE.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileDeletion {
    pub name: String,
    pub switch_to: Option<String>,
    pub saves: bool,
    pub paths: Vec<PathBuf>,
}

pub(super) fn check_new_name(name: &str) -> Result<()> {
    if profile::reads_as_flag(name) {
        return Err(Error::Other(format!(
            "{name:?} starts with -, which commands read as an option; start the profile name with a letter, digit or _"
        )));
    }
    if !profile::valid_name(name) {
        return Err(Error::Other(format!(
            "{name:?} is not a valid profile name (letters, digits, - and _)"
        )));
    }
    Ok(())
}

pub(super) fn load_profile(path: &Path) -> Result<Option<Profile>> {
    Ok(match crate::load_json(path)? {
        crate::Loaded::Found(p) => Some(p),
        crate::Loaded::Missing | crate::Loaded::Damaged(_) => None,
    })
}

impl Store {
    pub(super) fn update_profile_if(
        &self,
        name: &str,
        f: impl FnOnce(&mut Profile) -> Result<Vec<String>>,
    ) -> Result<Vec<String>> {
        let _write = profile_write();
        let mut p = self.profile(name)?;
        let added = f(&mut p)?;
        if !added.is_empty() {
            self.save_profile(&p)?;
        }
        Ok(added)
    }

    pub fn profile(&self, name: &str) -> Result<Profile> {
        if name == crate::profile::VANILLA {
            return Ok(Profile::vanilla());
        }
        let path = self.profiles_dir().join(format!("{name}.json"));
        let mut p = load_profile(&path)?.unwrap_or_else(|| Profile::new(name));
        let unknown = p.unknown_fields();
        if !unknown.is_empty() {
            warn_once(format!(
                "profile {name} has settings from a newer Swofty Launcher ({}); they are kept but not used",
                unknown.join(", ")
            ));
        }
        p.normalise();
        self.adopt_builtins(&mut p);
        Ok(p)
    }

    pub fn profile_names(&self) -> Result<Vec<String>> {
        let mut names = vec![profile::VANILLA.to_owned()];
        let rd = match std::fs::read_dir(self.profiles_dir()) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                names.push(DEFAULT_PROFILE.into());
                return Ok(names);
            }
            Err(e) => return Err(io_err(self.profiles_dir())(e)),
        };
        let mut found: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_str()?.strip_suffix(".json")?.to_owned();
                profile::valid_name(&name).then_some(name)
            })
            .collect();
        if found.is_empty() {
            found.push(DEFAULT_PROFILE.into());
        }
        found.sort();
        names.extend(found);
        Ok(names)
    }

    /// Refuses a name that isn't valid, or that another profile holds. Windows ignores
    /// capitals in file names, so `Grid` and `grid` would be one profile's files.
    pub(super) fn check_new_profile_name(&self, name: &str) -> Result<()> {
        check_new_name(name)?;
        if let Some(taken) = self
            .profile_names()?
            .iter()
            .find(|n| n.eq_ignore_ascii_case(name))
        {
            return Err(Error::Other(format!("a profile named {taken} already exists")));
        }
        Ok(())
    }

    pub fn create_profile(&self, name: &str, from: Option<&str>) -> Result<Profile> {
        self.check_new_profile_name(name)?;
        let mut p = match from {
            Some(src) => self.profile(src)?,
            None => Profile::new(name),
        };
        p.name = name.into();
        p.normalise();
        self.adopt_builtins(&mut p);
        if from.is_none() {
            for m in self.addons()?.into_values().filter(|m| {
                !m.is_builtin()
                    && !self.is_builtin(&m.id)
                    && m.id != crate::updates::managed::ID
                    && m.id != CORE_ID
            }) {
                p.add_addon(&m.id)?;
            }
        }
        self.save_profile(&p)?;
        crate::addons::storage::profile_created(&self.root, name, from)?;
        Ok(p)
    }

    pub fn rename_profile(&self, from: &str, to: &str) -> Result<Profile> {
        if from == profile::VANILLA {
            return Err(Error::Other("the vanilla profile cannot be renamed".into()));
        }
        // The running game keeps writing to the saves folder this would move.
        if let Some(j) = crate::game_folder::journal::Journal::open(&self.root)?
            .into_iter()
            .find(|j| j.profile == from)
        {
            return Err(Error::Other(format!(
                "{from} is installed in the game folder (journal {}); close F1 25 or run \
                 swofty restore, then rename it",
                j.id
            )));
        }
        if from != to && from.eq_ignore_ascii_case(to) {
            return self.rename_profile_capitals(from, to);
        }
        self.check_new_profile_name(to)?;
        let saves = self.saves_dir(from);
        let saves_to = self.saves_dir(to);
        let has_saves = saves.is_dir();
        if has_saves {
            if std::fs::read_dir(&saves_to).is_ok_and(|mut rd| rd.next().is_some()) {
                return Err(Error::Other(format!(
                    "{} already holds saves; move them away, then rename {from}",
                    saves_to.display()
                )));
            }
            std::fs::remove_dir(&saves_to).ok();
        }
        let p = self.create_profile(to, Some(from))?;
        // The saves move first: they fail most often, and until the old profile is
        // gone everything can still go back to how it was.
        let undo = || {
            std::fs::remove_file(self.profiles_dir().join(format!("{to}.json"))).ok();
            crate::addons::storage::profile_deleted(&self.root, to).ok();
        };
        if has_saves && let Err(e) = with_retry(&saves, || std::fs::rename(&saves, &saves_to)) {
            undo();
            return Err(e);
        }
        let old = self.profiles_dir().join(format!("{from}.json"));
        if old.is_file()
            && let Err(e) = with_retry(&old, || std::fs::remove_file(&old))
        {
            if has_saves {
                std::fs::rename(&saves_to, &saves).ok();
            }
            undo();
            return Err(e);
        }
        crate::saves::launcher::rename_vaults(&self.root, from, to)?;
        crate::saves::launcher::rename_in_marker(&self.root, from, to)?;
        crate::stdlib::careers::rename_profile(&self.root, from, to)?;
        crate::addons::storage::profile_deleted(&self.root, from)?;
        if self.config().active_profile.as_deref().unwrap_or(DEFAULT_PROFILE) == from {
            self.set_active_profile(to)?;
        }
        let mut config = self.config();
        if config.vr_profiles.remove(from) {
            config.vr_profiles.insert(to.to_owned());
            self.save_config(&config)?;
        }
        Ok(p)
    }

    /// `from` and `to` differ only in capitals. On Windows they name the same files, so going
    /// straight across would write over the profile and then delete it: it goes by way of
    /// another name.
    pub(super) fn rename_profile_capitals(&self, from: &str, to: &str) -> Result<Profile> {
        check_new_name(to)?;
        if self.profile_names()?.iter().any(|n| n == to) {
            return Err(Error::Other(format!("a profile named {to} already exists")));
        }
        let between = format!("renaming-{}", std::process::id());
        self.rename_profile(from, &between)?;
        self.rename_profile(&between, to).inspect_err(|_| {
            self.rename_profile(&between, from).ok();
        })
    }

    pub fn plan_profile_deletion(&self, name: &str) -> Result<ProfileDeletion> {
        if name == profile::VANILLA {
            return Err(Error::Other(
                "the vanilla profile is built in and cannot be deleted".into(),
            ));
        }
        let names = self.profile_names()?;
        if !names.iter().any(|n| n == name) {
            return Err(Error::Other(format!("no profile named {name}")));
        }
        let others: Vec<&String> = names
            .iter()
            .filter(|n| *n != profile::VANILLA && *n != name)
            .collect();
        let Some(first) = others.first() else {
            return Err(Error::Other(format!(
                "{name} is the only profile; create another profile before deleting it"
            )));
        };
        if let Some(j) = crate::game_folder::journal::Journal::open(&self.root)?
            .into_iter()
            .find(|j| j.profile == name)
        {
            return Err(Error::Other(format!(
                "{name} is installed in the game folder (journal {}); close F1 25 or run \
                 swofty restore, then delete it",
                j.id
            )));
        }
        let switch_to = (self.active_profile() == name).then(|| {
            others
                .iter()
                .find(|n| n.as_str() == DEFAULT_PROFILE)
                .unwrap_or(first)
                .to_string()
        });
        let saves = self.saves_dir(name);
        let has_saves = std::fs::read_dir(&saves).is_ok_and(|mut rd| rd.next().is_some());
        let paths = [
            self.overlays_dir(name),
            self.requests_path(name),
            saves,
            self.profiles_dir().join(format!("{name}.json")),
        ]
        .into_iter()
        .filter(|p| p.symlink_metadata().is_ok())
        .collect();
        Ok(ProfileDeletion {
            name: name.into(),
            switch_to,
            saves: has_saves,
            paths,
        })
    }

    pub fn delete_profile(&self, name: &str) -> Result<ProfileDeletion> {
        let _write = profile_write();
        let plan = self.plan_profile_deletion(name)?;
        if let Some(to) = &plan.switch_to {
            self.set_active_profile(to)?;
        }
        self.forget_launch_profile(name)?;
        let mut config = self.config();
        if config.vr_profiles.remove(name) {
            self.save_config(&config)?;
        }
        for path in &plan.paths {
            let removed = if path.is_dir() && !path.is_symlink() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            };
            if let Err(e) = removed
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(io_err(path)(e));
            }
        }
        crate::addons::storage::profile_deleted(&self.root, name)?;
        Ok(plan)
    }

    pub(super) fn forget_launch_profile(&self, name: &str) -> Result<()> {
        let path = self.launch_request_path();
        let Ok(mut request) = crate::read_json::<serde_json::Value>(&path) else {
            return Ok(());
        };
        if request.get("profile").and_then(|p| p.as_str()) != Some(name) {
            return Ok(());
        }
        if let Some(fields) = request.as_object_mut() {
            fields.remove("profile");
        }
        crate::write_json(&path, &request)
    }

    pub fn active_profile(&self) -> String {
        let names = self.profile_names().unwrap_or_default();
        self.config()
            .active_profile
            .filter(|p| names.contains(p))
            .or_else(|| names.get(1).cloned())
            .unwrap_or_else(|| DEFAULT_PROFILE.into())
    }

    pub fn set_active_profile(&self, name: &str) -> Result<()> {
        let mut config = self.config();
        config.active_profile = Some(name.into());
        self.save_config(&config)
    }

    pub fn update_profile<T>(&self, name: &str, f: impl FnOnce(&mut Profile) -> Result<T>) -> Result<T> {
        let _write = profile_write();
        let mut p = self.profile(name)?;
        let out = f(&mut p)?;
        self.save_profile(&p)?;
        Ok(out)
    }

    /// Writes a load-order fix: the profile's new order and the folders its
    /// ejected addons leave, together. A folders failure puts the profile back.
    pub fn apply_fix(
        &self,
        name: &str,
        plan: &crate::profile::order::FixPlan,
        default_place: &dyn Fn(&str) -> Option<crate::profile::folders::Key>,
    ) -> Result<()> {
        let _write = profile_write();
        let saved = self.profile(name)?;
        let mut fixed = saved.clone();
        crate::profile::folders::reorder(&mut fixed, &plan.order, &|_| false);
        self.save_profile(&fixed)?;
        if plan.ejected.is_empty() {
            return Ok(());
        }
        let mut folders = self.folders();
        for (id, _) in &plan.ejected {
            folders.set_place(id, None, default_place(id).as_ref());
        }
        if let Err(e) = self.save_folders(&folders) {
            self.save_profile(&saved)?;
            return Err(e);
        }
        Ok(())
    }

    pub fn save_profile(&self, p: &Profile) -> Result<()> {
        if p.is_vanilla() {
            return Err(Error::Other(
                "the vanilla profile is built in and cannot be changed".into(),
            ));
        }
        crate::write_json(&self.profiles_dir().join(format!("{}.json", p.name)), p)
    }
}
