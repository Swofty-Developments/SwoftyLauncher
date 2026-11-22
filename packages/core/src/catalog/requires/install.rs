//! Installing an addon from the Addon Browser with everything it requires, replanning until the downloads and
//! the store agree.

use std::path::Path;

use super::downloads::{Zips, fetch, release};
use super::manifest::requires_in;
use super::walk::plan;
use super::{Have, Plan, SETTLE};
use crate::catalog::{Addon, INSTALLING, Stage, Version, downloads_dir};
use crate::manifest::{AddonManifest, Requirement};
use crate::store::Store;
use crate::{Error, Result};

/// How far [`install`] has got.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event<'a> {
    /// What it will install, once the requirements are known.
    Planned(&'a Plan),
    /// The addon the next stages are about.
    Addon(&'a Addon, &'a Version),
    Stage(Stage),
    /// A manifest successfully installed by this job.
    Installed(&'a AddonManifest),
}

/// Downloads `v` of `addon` and every addon it requires that `store` lacks,
/// checks each download, then installs them, requirements first. Returns the
/// manifests it installed in install order, `addon`'s last; a requirement
/// another install put in place meanwhile is left out.
pub fn install(
    store: &Store,
    catalog: &[Addon],
    addon: &Addon,
    v: &Version,
    progress: &dyn Fn(Event),
) -> Result<Vec<AddonManifest>> {
    install_target(store, catalog, addon, v, None, progress)
}

/// Install a catalog job into the profile from which it was requested.
pub fn install_in_profile(
    store: &Store,
    catalog: &[Addon],
    addon: &Addon,
    v: &Version,
    profile: &str,
    progress: &dyn Fn(Event),
) -> Result<Vec<AddonManifest>> {
    install_target(store, catalog, addon, v, Some(profile), progress)
}

pub(super) fn install_target(
    store: &Store,
    catalog: &[Addon],
    addon: &Addon,
    v: &Version,
    profile: Option<&str>,
    progress: &dyn Fn(Event),
) -> Result<Vec<AddonManifest>> {
    let dir = downloads_dir(store)?;
    let zips = Zips::default();
    let result = install_with(store, catalog, addon, v, profile, (&dir, &zips), progress);
    release(zips);
    result
}

pub(super) fn install_with(
    store: &Store,
    catalog: &[Addon],
    addon: &Addon,
    v: &Version,
    profile: Option<&str>,
    downloads: (&Path, &Zips),
    progress: &dyn Fn(Event),
) -> Result<Vec<AddonManifest>> {
    let (dir, zips) = downloads;
    let launcher = crate::updates::release::current_version();
    // Other installs may change the store while this job is downloading. Replan
    // under the installation lock, but never perform network work while holding it.
    for _ in 0..SETTLE {
        let addons = store.addons()?;
        let have = Have::of(store, &addons);
        let mut read = |a: &Addon, v: &Version| -> Result<Vec<Requirement>> {
            let saved = zips.borrow().get(&(a.id.clone(), v.version.clone())).cloned();
            match (saved, &v.requires) {
                (Some(path), _) => requires_in(&path, a, v),
                (None, Some(listed)) => Ok(listed.clone()),
                (None, None) => requires_in(&fetch(dir, zips, a, v, progress)?, a, v),
            }
        };
        let mut planned = plan(addon, v, catalog, &have, &launcher, &mut read)?;
        let mut settled = false;
        for _ in 0..SETTLE {
            progress(Event::Planned(&planned));
            for s in &planned.steps {
                fetch(dir, zips, &s.addon, &s.version, progress)?;
            }
            let again = plan(addon, v, catalog, &have, &launcher, &mut read)?;
            if again == planned {
                settled = true;
                break;
            }
            planned = again;
        }
        if !settled {
            return Err(Error::Other(format!(
                "{}'s downloads list other requirements than the registry",
                addon.name
            )));
        }
        let _one = INSTALLING.lock().unwrap_or_else(|e| e.into_inner());
        let current = store.addons()?;
        let current_have = Have::of(store, &current);
        let mut missing = false;
        let refreshed = plan(addon, v, catalog, &current_have, &launcher, &mut |a, version| {
            let path = zips
                .borrow()
                .get(&(a.id.clone(), version.version.clone()))
                .cloned();
            match (path, &version.requires) {
                (Some(path), _) => requires_in(&path, a, version),
                (None, Some(listed)) => Ok(listed.clone()),
                (None, None) => {
                    missing = true;
                    Err(Error::Other("requirements need another download".into()))
                }
            }
        });
        if missing {
            continue;
        }
        let refreshed = refreshed?;
        if refreshed.steps.iter().any(|s| {
            !zips
                .borrow()
                .contains_key(&(s.addon.id.clone(), s.version.version.clone()))
        }) {
            continue;
        }
        if refreshed != planned {
            progress(Event::Planned(&refreshed));
        }
        let mut done = Vec::new();
        for s in &refreshed.steps {
            let path = zips.borrow()[&(s.addon.id.clone(), s.version.version.clone())].clone();
            progress(Event::Addon(&s.addon, &s.version));
            let installed = {
                if s.addon.id != addon.id
                    && store
                        .addons()?
                        .get(&s.addon.id)
                        .is_some_and(|m| m.version == s.version.version)
                {
                    continue;
                }
                progress(Event::Stage(Stage::Installing));
                let installed = requires_in(&path, &s.addon, &s.version)
                    .and_then(|_| store.install_listed(&path, profile, &s.addon.id, &s.version.version));
                if let Ok(m) = &installed
                    && m.id == s.addon.id
                    && let Err(e) = crate::catalog::record_install(store, &m.id, &s.version)
                {
                    crate::catalog::log(
                        &store.root,
                        &format!(
                            "{}: the install from {} went unrecorded: {e}",
                            m.id, s.version.url
                        ),
                    );
                }
                installed
            };
            let m = match installed {
                Ok(m) => m,
                Err(e) if s.addon.id == addon.id => return Err(e),
                Err(e) => {
                    return Err(Error::Other(format!(
                        "{}, which {} requires: {e}",
                        s.label(),
                        addon.name
                    )));
                }
            };
            if m.id != s.addon.id {
                return Err(Error::Other(format!(
                    "the registry lists {}, the download installed {}",
                    s.addon.id, m.id
                )));
            }
            progress(Event::Installed(&m));
            done.push(m);
        }
        return Ok(done);
    }
    Err(Error::Other(format!(
        "{}'s dependencies changed while installing; please retry",
        addon.name
    )))
}
