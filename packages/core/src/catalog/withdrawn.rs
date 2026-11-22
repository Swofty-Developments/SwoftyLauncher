//! Installed addon versions the registry withdrew (yanked) from the Addon Browser. The catalog leaves
//! a withdrawn version out, so after each catalog fetch every installed addon whose version the catalog
//! doesn't list is asked about on its own page ([`super::fetch_one`]), which still lists withdrawn
//! versions. The answers are saved, so the Addons page, Play and the known problems read them offline.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Addon, Trouble};
use crate::manifest::AddonManifest;
use crate::store::Store;

/// The saved answers, by addon id.
pub const FILE: &str = "state/withdrawn-addons.json";

/// How long an answer that a version was not withdrawn is kept before the registry is asked again.
pub const RECHECK_SECS: i64 = 6 * 3600;

/// The most addons one catalog fetch asks the registry about.
pub const ASKED_PER_FETCH: usize = 20;

/// What the registry said about the installed version of one addon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub version: semver::Version,
    pub withdrawn: bool,
    /// When the registry was asked, in seconds since the Unix epoch.
    pub checked: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checks(pub BTreeMap<String, Check>);

/// An addon whose installed version was withdrawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawn {
    pub id: String,
    pub name: String,
    pub version: semver::Version,
}

impl Withdrawn {
    /// Its name and version, as `BMW 2026 1.0.7`.
    pub fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

impl Checks {
    /// The saved answers; none when the file is missing or can't be read.
    pub fn load(root: &Path) -> Self {
        crate::read_json(&path(root)).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> crate::Result<()> {
        crate::write_json_shared(&path(root), self)
    }

    /// Whether `version` of `id` is a withdrawn one.
    pub fn is_withdrawn(&self, id: &str, version: &semver::Version) -> bool {
        self.0
            .get(id)
            .is_some_and(|c| c.withdrawn && c.version == *version)
    }

    /// The addons of `ids` whose installed version in `installed` was withdrawn, in the order of `ids`.
    pub fn among<'a>(
        &self,
        installed: &BTreeMap<String, AddonManifest>,
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Withdrawn> {
        let mut out: Vec<Withdrawn> = Vec::new();
        for id in ids {
            let Some(m) = installed.get(id) else {
                continue;
            };
            if self.is_withdrawn(id, &m.version) && !out.iter().any(|w| w.id == id) {
                out.push(Withdrawn {
                    id: id.to_owned(),
                    name: m.name.clone(),
                    version: m.version.clone(),
                });
            }
        }
        out
    }
}

/// The installed addons whose versions are checked: the ones that take the Addon Browser's updates
/// ([`super::store_updates`]).
pub fn checked<'a>(
    store: &'a Store,
    installed: &'a BTreeMap<String, AddonManifest>,
) -> impl Iterator<Item = &'a AddonManifest> {
    installed.values().filter(move |m| {
        !store.is_builtin(&m.id)
            && !crate::updates::managed::is_managed(&m.id)
            && super::source::takes_updates(m)
    })
}

/// Whether the catalog lists `version` of `id` as a version players can install.
pub fn listed(catalog: &[Addon], id: &str, version: &semver::Version) -> bool {
    catalog
        .iter()
        .find(|a| a.id == id)
        .is_some_and(|a| a.listed().iter().any(|v| v.version == *version))
}

fn not_found(trouble: &Trouble) -> bool {
    matches!(trouble, Trouble::Failed(text) if text.ends_with("HTTP 404"))
}

/// The answers after a catalog fetch: an installed version the catalog lists needs none, a saved
/// answer for the same version is kept while it is withdrawn or younger than [`RECHECK_SECS`], and
/// `fetch` asks the registry about the rest, at most [`ASKED_PER_FETCH`] of them. An addon the
/// registry doesn't have counts as not withdrawn. When the registry can't be reached, the saved
/// answer stays. Also returns a line for the registry log for each version newly found withdrawn.
pub fn update<'a>(
    saved: &Checks,
    installed: impl IntoIterator<Item = &'a AddonManifest>,
    catalog: &[Addon],
    now: i64,
    fetch: &mut dyn FnMut(&str) -> Result<Addon, Trouble>,
) -> (Checks, Vec<String>) {
    let mut out = Checks::default();
    let mut lines = Vec::new();
    let mut asked = 0;
    let mut offline = false;
    for m in installed {
        if listed(catalog, &m.id, &m.version) {
            continue;
        }
        let kept = saved.0.get(&m.id).filter(|c| c.version == m.version);
        if let Some(c) = kept.filter(|c| c.withdrawn || now - c.checked < RECHECK_SECS) {
            out.0.insert(m.id.clone(), c.clone());
            continue;
        }
        if offline || asked >= ASKED_PER_FETCH {
            if let Some(c) = kept {
                out.0.insert(m.id.clone(), c.clone());
            }
            continue;
        }
        asked += 1;
        let withdrawn = match fetch(&m.id) {
            Ok(addon) => addon
                .versions
                .iter()
                .chain(addon.latest.iter())
                .any(|v| v.version == m.version && v.yanked),
            Err(e) if not_found(&e) => false,
            Err(e) => {
                offline |= matches!(e, Trouble::Offline(_));
                if let Some(c) = kept {
                    out.0.insert(m.id.clone(), c.clone());
                }
                continue;
            }
        };
        if withdrawn && !kept.is_some_and(|c| c.withdrawn) {
            lines.push(format!(
                "{} {} is installed and was withdrawn from the Addon Browser",
                m.id, m.version
            ));
        }
        out.0.insert(
            m.id.clone(),
            Check {
                version: m.version.clone(),
                withdrawn,
                checked: now,
            },
        );
    }
    (out, lines)
}

/// [`update`] for the addons installed in `store`, asking the registry with [`super::fetch_one`]; the
/// answers are saved and each version newly found withdrawn goes to the registry log.
pub fn refresh(store: &Store, catalog: &[Addon]) -> Checks {
    let saved = Checks::load(&store.root);
    let Ok(installed) = store.addons() else {
        return saved;
    };
    let (checks, lines) = update(
        &saved,
        checked(store, &installed),
        catalog,
        super::now_secs(),
        &mut |id| super::fetch_one(&store.root, id),
    );
    for line in &lines {
        super::log(&store.root, line);
    }
    if checks != saved
        && let Err(e) = checks.save(&store.root)
    {
        eprintln!("swofty-core: could not save the withdrawn addon versions: {e}");
    }
    checks
}

/// The addons on in `ids` whose installed version was withdrawn, from the saved answers.
pub fn enabled<'a>(store: &Store, ids: impl IntoIterator<Item = &'a str>) -> Vec<Withdrawn> {
    let checks = Checks::load(&store.root);
    if checks.0.values().all(|c| !c.withdrawn) {
        return Vec::new();
    }
    let installed = store.addons().unwrap_or_default();
    checks.among(&installed, ids.into_iter().filter(|id| !store.is_builtin(id)))
}

#[cfg(test)]
mod tests;
