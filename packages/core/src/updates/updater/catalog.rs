//! The managed addon's releases, each with the launcher versions it needs, and when to list them again.

use std::time::Duration;

use semver::{Version, VersionReq};

use super::probe::{Part, probe};
use super::track::Track;
use super::{CHECK_INTERVAL, RELEASE_RECHECK};
use crate::Result;
use crate::store::Store;
use crate::updates::managed;
use crate::updates::release::{self, Failure, Release};

pub const LIST_RECHECK: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct Candidate {
    pub release: Release,
    pub version: Version,
    pub needs: Option<VersionReq>,
    pub learn_at: u64,
}

impl Candidate {
    pub fn accepts(&self, launcher: &Version) -> bool {
        self.needs.as_ref().is_some_and(|r| r.matches(launcher))
    }

    fn same_package(&self, release: &Release) -> bool {
        self.release.tag_name == release.tag_name
            && managed::package_asset(&self.release).ok() == managed::package_asset(release).ok()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub latest: Track,
    pub releases: Vec<Candidate>,
    pub listed_for: Option<String>,
    pub list_at: u64,
    pub wait_until: u64,
    pub error: Option<String>,
}

impl Catalog {
    pub fn probe_due(&self, now: u64) -> bool {
        self.latest.probe_due(now)
    }

    pub fn list_due(&self, now: u64) -> bool {
        now >= self.wait_until && (now >= self.list_at || self.latest.tag != self.listed_for)
    }

    pub fn on_list(&mut self, fetched: std::result::Result<Vec<Release>, Failure>, now: u64) {
        self.listed_for = self.latest.tag.clone();
        let list = match fetched {
            Ok(list) => list,
            Err(Failure::Wait(secs, text)) => {
                self.wait_until = now + secs.max(CHECK_INTERVAL.as_secs());
                self.error = Some(text);
                return;
            }
            Err(Failure::Failed(text)) => {
                self.list_at = now + RELEASE_RECHECK.as_secs();
                self.error = Some(text);
                return;
            }
        };
        let (mut next, mut unfinished) = (Vec::new(), None::<Version>);
        for release in list.into_iter().filter(|r| !r.draft && !r.prerelease) {
            let Some(version) = release.version() else {
                continue;
            };
            if !managed::ready(&release) {
                unfinished = unfinished.max(Some(version));
                continue;
            }
            let known = self.releases.iter().find(|c| c.same_package(&release));
            next.push(Candidate {
                needs: known.and_then(|c| c.needs.clone()),
                learn_at: known.map_or(0, |c| c.learn_at),
                version,
                release,
            });
        }
        next.sort_by(|a, b| b.version.cmp(&a.version));
        let waiting = unfinished.is_some_and(|u| next.first().is_none_or(|c| u > c.version));
        self.list_at = now + if waiting { RELEASE_RECHECK } else { LIST_RECHECK }.as_secs();
        self.releases = next;
        self.error = None;
    }

    fn newer(&self, installed: Option<&Version>) -> Vec<&Candidate> {
        self.releases
            .iter()
            .filter(|c| managed::newer(installed, &c.version))
            .collect()
    }

    pub fn to_learn(&self, installed: Option<&Version>, launcher: &Version, now: u64) -> Vec<Release> {
        let mut out = Vec::new();
        for c in self.newer(installed) {
            if c.accepts(launcher) {
                break;
            }
            if c.needs.is_none() && now >= c.learn_at {
                out.push(c.release.clone());
            }
        }
        out
    }

    pub fn learn(
        &mut self,
        installed: Option<&Version>,
        launcher: &Version,
        now: u64,
        mut needs: impl FnMut(&Release) -> Result<VersionReq>,
    ) {
        for release in self.to_learn(installed, launcher, now) {
            let learned = needs(&release);
            let Some(c) = self
                .releases
                .iter_mut()
                .find(|c| c.release.tag_name == release.tag_name)
            else {
                continue;
            };
            match learned {
                Ok(req) => {
                    c.needs = Some(req);
                    if c.accepts(launcher) {
                        break;
                    }
                }
                Err(e) => {
                    c.learn_at = now + RELEASE_RECHECK.as_secs();
                    self.error = Some(format!("{} {}: {e}", managed::NAME, release.tag_name));
                }
            }
        }
    }

    pub fn choose(&self, launcher: &Version, installed: Option<&Version>) -> Option<&Release> {
        self.releases
            .iter()
            .find(|c| c.accepts(launcher))
            .filter(|c| managed::newer(installed, &c.version))
            .map(|c| &c.release)
    }

    pub fn waiting(&self, launcher: &Version, installed: Option<&Version>) -> Option<&Candidate> {
        let chosen = self.choose(launcher, installed).and_then(Release::version);
        self.newer(installed).into_iter().find(|c| {
            c.needs.is_some() && !c.accepts(launcher) && chosen.as_ref().is_none_or(|v| c.version > *v)
        })
    }

    pub fn keep(&self, installed: Option<&Version>) -> Vec<&str> {
        self.newer(installed)
            .into_iter()
            .map(|c| c.release.tag_name.as_str())
            .collect()
    }

    /// Probes the importer's latest release, lists the releases when due, learns which launcher each
    /// newer one needs, and removes the cached packages of releases that are not newer than `installed`.
    pub fn check(&mut self, store: &Store, installed: Option<&Version>, launcher: &Version, now: u64) {
        self.latest
            .on_probe(probe(Part::Addon, release::CHECK_TIMEOUT), now);
        if self.list_due(now) {
            self.on_list(managed::fetch_releases(release::CHECK_TIMEOUT), now);
        }
        self.learn(installed, launcher, now, |r| managed::requirement(store, r));
        if !self.releases.is_empty() {
            managed::prune(store, &self.keep(installed));
        }
    }
}

/// [`Catalog::check`].
pub fn check_importer(
    store: &Store,
    catalog: &mut Catalog,
    installed: Option<&Version>,
    launcher: &Version,
    now: u64,
) {
    catalog.check(store, installed, launcher, now);
}
