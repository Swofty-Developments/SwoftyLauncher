//! Installing addons from a file, the catalog or a managed release into the addons folder and a profile, and
//! removing them.

use std::path::{Path, PathBuf};

use super::Store;
use super::config::warn;
use super::profiles::{load_profile, profile_write};
use super::scan::Unreadable;
use super::swap::{SCRATCH_PREFIX, STAGING_PREFIX, Scratch, delete_addon_dir, swap_in_held, swap_lock_for};
use crate::manifest::{AddonManifest, MANIFEST_FILE};
use crate::profile;
use crate::{Error, Result, io_err};

/// Where an addon being installed came from, which decides what it may be.
#[derive(Debug, Clone, Copy)]
pub(super) enum InstallFrom<'a> {
    /// A file the player chose.
    Other,
    /// A folder the helper of the addon with this id asked to install.
    Helper(&'a str),
    /// A download the catalog lists as this id and version.
    Catalog(&'a str, &'a semver::Version),
    /// A release of the addon the launcher manages.
    ManagedRelease,
}

pub(super) fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest).map_err(io_err(dest))?;
    for entry in std::fs::read_dir(src).map_err(io_err(src))? {
        let entry = entry.map_err(io_err(src))?;
        let to = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(io_err(&to))?;
        }
    }
    Ok(())
}

impl Store {
    pub fn install_addon(&self, src: &Path) -> Result<AddonManifest> {
        self.install_addon_for(src, &crate::updates::release::current_version())
    }

    /// Every addon `src` holds: a zip or rar may carry several addon folders
    /// and packages.
    pub fn install_addons(&self, src: &Path) -> Vec<Result<AddonManifest>> {
        let launcher = crate::updates::release::current_version();
        if !src.is_file() {
            return vec![self.install_addon_for(src, &launcher)];
        }
        let scratch = Scratch::new(&self.addons_dir());
        match self.unpacked_addons(src, &scratch.dir) {
            Ok(found) => found
                .iter()
                .map(|addon| self.install_addon_for(addon, &launcher))
                .collect(),
            Err(e) => vec![Err(e)],
        }
    }

    pub fn install_addon_for(&self, src: &Path, launcher: &semver::Version) -> Result<AddonManifest> {
        self.install_addon_target(src, launcher, None, InstallFrom::Other)
    }

    /// Install into a specific existing profile rather than whichever profile is active later.
    pub fn install_addon_in_profile(&self, src: &Path, profile: &str) -> Result<AddonManifest> {
        self.install_addon_target(
            src,
            &crate::updates::release::current_version(),
            Some(profile),
            InstallFrom::Other,
        )
    }

    /// Install a folder the helper of the addon `helper` made. An addon the Simplified Importer
    /// made may be installed under another id ([`crate::addons::imported::install_id`]).
    pub(crate) fn install_addon_by(&self, src: &Path, helper: &str) -> Result<AddonManifest> {
        self.install_addon_target(
            src,
            &crate::updates::release::current_version(),
            None,
            InstallFrom::Helper(helper),
        )
    }

    /// Install what the catalog lists as `id` at `version`, into `profile` when one is named.
    pub(crate) fn install_listed(
        &self,
        src: &Path,
        profile: Option<&str>,
        id: &str,
        version: &semver::Version,
    ) -> Result<AddonManifest> {
        self.install_addon_target(
            src,
            &crate::updates::release::current_version(),
            profile,
            InstallFrom::Catalog(id, version),
        )
    }

    /// Install a release of the addon the launcher manages.
    pub(crate) fn install_managed_release(
        &self,
        src: &Path,
        launcher: &semver::Version,
    ) -> Result<AddonManifest> {
        self.install_addon_target(src, launcher, None, InstallFrom::ManagedRelease)
    }

    pub(super) fn install_addon_target(
        &self,
        src: &Path,
        launcher: &semver::Version,
        profile: Option<&str>,
        from: InstallFrom,
    ) -> Result<AddonManifest> {
        if src.is_file() {
            let scratch = Scratch::new(&self.addons_dir());
            return self
                .unpacked_addons(src, &scratch.dir)
                .and_then(|found| match found.as_slice() {
                    [one] => self.install_addon_target(one, launcher, profile, from),
                    many => Err(Error::Other(format!(
                        "{} holds {} addons; drop it on the launcher to install them all",
                        src.display(),
                        many.len()
                    ))),
                });
        }
        let src = &crate::addons::import::addon_root(src).unwrap_or_else(|| src.to_path_buf());
        if src.join(crate::dbedit::package::MARKER).is_file() {
            return crate::dbedit::package::install(self, src).map(|(p, _)| {
                std::fs::remove_file(self.registry_install_path(&p.manifest.id)).ok();
                self.add_to_install_profile(&p.manifest.id, profile);
                p.manifest
            });
        }
        let mut m = AddonManifest::load(src).map_err(|e| {
            let u = Unreadable::of(src, &e, launcher);
            if u.newer {
                Error::Other(u.line())
            } else {
                Error::from(e)
            }
        })?;
        if self.is_builtin(&m.id) {
            return Err(Error::Other(format!("{} is built in", m.id)));
        }
        if let InstallFrom::Catalog(id, version) = from
            && (m.id != id || m.version != *version)
        {
            return Err(Error::Other(format!(
                "the registry lists {id} {version}, the download holds {} {}",
                m.id, m.version
            )));
        }
        if matches!(from, InstallFrom::Catalog(..)) {
            crate::addons::imported::refuse_listed(self, &m)?;
        }
        let id = match from {
            InstallFrom::Helper(helper) if crate::addons::imported::made_by(helper, &m) => {
                crate::addons::imported::install_id(self, src, &m)?
            }
            _ => m.id.clone(),
        };
        if crate::updates::managed::is_managed(&m.id) && !matches!(from, InstallFrom::ManagedRelease) {
            return Err(Error::Other(format!(
                "{} is managed by the launcher, which installs and updates it itself",
                m.id
            )));
        }
        let prefix = crate::dbedit::package::ID_PREFIX;
        if crate::dbedit::package::is_package(self, &m.id) {
            return Err(Error::Other(format!(
                "{} is a DB Editor addon you have; installing over it would delete its edits",
                m.id
            )));
        }
        if m.id.starts_with(prefix) {
            return Err(Error::Other(format!(
                "{}: the {prefix} prefix is kept for addons the DB Editor saves",
                m.id
            )));
        }
        if let Some(why) = crate::profile::resolver::launcher_refusal(&m, launcher) {
            return Err(Error::Other(why));
        }
        crate::game_folder::busy::refuse_change(&self.root, &id, &m.name)?;
        let dest = self.addons_dir().join(&id);
        let fresh = !dest.exists();
        self.helpers.forget(&id);
        std::fs::create_dir_all(self.addons_dir()).map_err(io_err(self.addons_dir()))?;
        let _lock = swap_lock_for(&dest, &m.name)?;
        let staging = self.addons_dir().join(format!("{STAGING_PREFIX}{id}"));
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(io_err(&staging))?;
        }
        let moved = self.in_scratch(src) && std::fs::rename(src, &staging).is_ok();
        if !moved && let Err(e) = copy_dir(src, &staging) {
            std::fs::remove_dir_all(&staging).ok();
            return Err(e);
        }
        if id != m.id {
            match crate::addons::imported::rename(&staging, &id) {
                Ok(renamed) => m = renamed,
                Err(e) => {
                    std::fs::remove_dir_all(&staging).ok();
                    return Err(e);
                }
            }
        }
        if let Err(e) = crate::addons::files::seal(&staging) {
            std::fs::remove_dir_all(&staging).ok();
            let verb = if fresh { "install" } else { "update" };
            return Err(Error::Other(format!("Couldn't {verb} {}: {e}", m.name)));
        }
        swap_in_held(
            &staging,
            &dest,
            &m.name,
            &mut |from, to| std::fs::rename(from, to),
            &mut |d| std::fs::remove_dir_all(d),
        )?;
        std::fs::remove_file(self.registry_install_path(&m.id)).ok();
        if fresh {
            self.add_to_install_profile(&m.id, profile);
        }
        Ok(m)
    }

    /// A new addon goes into the profile it was installed from, off, rather than
    /// every profile; the others add it from their own Addons page. A failure
    /// only leaves it out of the profile, so it doesn't fail the install.
    /// Every installed addon is in every profile, off until a profile turns it on, so an
    /// install lists it in all of them at the end of their order. `target` only names a
    /// profile that must still exist for a queued install to count.
    pub(super) fn add_to_install_profile(&self, id: &str, target: Option<&str>) {
        let names = match self.profile_names() {
            Ok(names) => names,
            Err(e) => {
                warn(format!("couldn't list the profiles for {id}: {e}"));
                return;
            }
        };
        if let Some(target) = target
            && target != profile::VANILLA
            && !names.iter().any(|n| n == target)
        {
            warn(format!(
                "profile {target} no longer exists; {id} is listed in the others"
            ));
        }
        for name in names.iter().filter(|n| n.as_str() != profile::VANILLA) {
            if let Err(e) = self.update_profile_if(name, |p| {
                let had = p.has(id);
                p.add_addon(id)?;
                Ok(if had { Vec::new() } else { vec![id.to_owned()] })
            }) {
                warn(format!("couldn't add {id} to the {name} profile: {e}"));
            }
        }
    }

    pub(super) fn in_scratch(&self, path: &Path) -> bool {
        path.strip_prefix(self.addons_dir())
            .ok()
            .and_then(|rest| rest.components().next())
            .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with(SCRATCH_PREFIX))
    }

    pub(super) fn unpacked_addons(&self, archive: &Path, scratch: &Path) -> Result<Vec<PathBuf>> {
        if scratch.exists() {
            std::fs::remove_dir_all(scratch).map_err(io_err(scratch))?;
        }
        crate::addons::import::unpack(archive, scratch)?;
        let found = if crate::addons::import::is_package(archive) {
            crate::addons::import::addon_root(scratch).into_iter().collect()
        } else {
            crate::addons::import::found_addons(scratch)
        };
        if found.is_empty() {
            return Err(Error::Other(format!(
                "{} has no {MANIFEST_FILE}",
                archive.display()
            )));
        }
        Ok(found)
    }

    pub fn remove_addon(&self, id: &str) -> Result<()> {
        self.remove_addon_with(id, |d| std::fs::remove_dir_all(d))
    }

    pub(crate) fn remove_addon_with(
        &self,
        id: &str,
        mut delete: impl FnMut(&Path) -> std::io::Result<()>,
    ) -> Result<()> {
        if self.is_builtin(id) {
            return Err(Error::Other(format!("{id} is built in and cannot be removed")));
        }
        if crate::updates::managed::is_managed(id) {
            return Err(Error::Other(format!(
                "{id} is managed by the launcher and cannot be removed"
            )));
        }
        let dir = self.addons_dir().join(id);
        if !dir.exists() {
            return Err(Error::Other(format!("no addon named {id} is installed")));
        }
        let name = AddonManifest::load(&dir).map_or_else(|_| id.to_owned(), |m| m.name);
        crate::game_folder::busy::refuse_change(&self.root, id, &name)?;
        let _write = profile_write();
        self.helpers.forget(id);
        delete_addon_dir(
            &dir,
            &name,
            &mut |from, to| std::fs::rename(from, to),
            &mut delete,
        )?;
        std::fs::remove_file(self.registry_install_path(id)).ok();
        for name in self.profile_names()? {
            let path = self.profiles_dir().join(format!("{name}.json"));
            if name == profile::VANILLA || !path.is_file() {
                continue;
            }
            let Some(mut p) = load_profile(&path)? else {
                continue;
            };
            let listed = p.addons.iter().any(|a| a.id == id);
            let had_options = p.options.contains_key(id) || p.applied.contains_key(id);
            let shader = p.shader.as_deref().is_some_and(|r| {
                r.strip_prefix("addon:")
                    .and_then(|r| r.split_once('/'))
                    .is_some_and(|(a, _)| a == id)
            });
            if listed || had_options || shader {
                if shader {
                    p.shader = None;
                }
                p.addons.retain(|a| a.id != id);
                p.options.remove(id);
                p.applied.remove(id);
                p.normalise();
                self.save_profile(&p)?;
            }
        }
        Ok(())
    }
}
