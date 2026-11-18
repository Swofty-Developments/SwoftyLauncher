//! Planning an install: walking the requirements of an addon and of each addon they bring, and choosing a
//! version of each.

use std::collections::{BTreeMap, BTreeSet};

use super::{Have, Need, Plan, Problem, ROUNDS, Refusal, Step, Why};
use crate::catalog::{Addon, Version};
use crate::manifest::Requirement;
use crate::{Error, Result};

pub(super) struct Walk<'w> {
    pub(super) catalog: &'w [Addon],
    pub(super) have: &'w Have<'w>,
    pub(super) launcher: &'w semver::Version,
    pub(super) learned: &'w BTreeMap<String, Vec<Need>>,
    pub(super) root: String,
    pub(super) decided: BTreeMap<String, semver::Version>,
    pub(super) planned: BTreeSet<String>,
    pub(super) needs: BTreeMap<String, Vec<Need>>,
    pub(super) stack: Vec<String>,
    pub(super) steps: Vec<Step>,
    pub(super) problems: Vec<Problem>,
    pub(super) retry: Option<(String, Need)>,
}

pub(super) type Reader<'r> = dyn FnMut(&Addon, &Version) -> Result<Vec<Requirement>> + 'r;

impl<'w> Walk<'w> {
    fn visit(&mut self, addon: &Addon, v: &Version, read: &mut Reader<'_>) -> Result<()> {
        self.stack.push(addon.id.clone());
        for r in read(addon, v)? {
            if self.retry.is_some() {
                break;
            }
            self.need(&r, &addon.name, read)?;
        }
        self.stack.pop();
        Ok(())
    }

    fn need(&mut self, r: &Requirement, by: &str, read: &mut Reader<'_>) -> Result<()> {
        let need = Need {
            by: by.to_owned(),
            version: r.version.clone(),
        };
        let id = r.id.as_str();
        if let Some(at) = self.stack.iter().position(|s| s == id) {
            let ring = self.stack[at..].to_vec();
            let mut key = ring.clone();
            key.sort();
            let known = self.problems.iter().any(|p| match p {
                Problem::Cycle(other) => {
                    let mut other = other.clone();
                    other.sort();
                    other == key
                }
                Problem::Unmet { .. } => false,
            });
            if !known {
                self.problems.push(Problem::Cycle(ring));
            }
            return Ok(());
        }
        if let Some(Problem::Unmet { needs, .. }) = self
            .problems
            .iter_mut()
            .find(|p| matches!(p, Problem::Unmet { id: other, .. } if other == id))
        {
            needs.push(need);
            return Ok(());
        }
        if let Some(at) = self.decided.get(id).cloned() {
            if crate::addons::version::satisfies(&r.version, &at) {
                self.needs.entry(id.to_owned()).or_default().push(need);
                return Ok(());
            }
            let why = if id == self.root {
                Why::Installing(at)
            } else if self.have.builtin.contains(id) {
                Why::BuiltIn(at)
            } else if self.have.kept.contains(id) {
                Why::Kept(at)
            } else {
                self.retry = Some((id.to_owned(), need));
                return Ok(());
            };
            let mut needs = self.needs.get(id).cloned().unwrap_or_default();
            needs.push(need);
            self.unmet(id, needs, why);
            return Ok(());
        }
        let installed = self.have.addons.get(id).copied();
        let mut needs = self.learned.get(id).cloned().unwrap_or_default();
        if let Some(m) = installed {
            needs.extend(self.dependents(id, &m.version));
        }
        needs.push(need);
        let fits = |v: &semver::Version| {
            needs
                .iter()
                .all(|n| crate::addons::version::satisfies(&n.version, v))
        };
        if let Some(m) = installed {
            if fits(&m.version) {
                self.decided.insert(id.to_owned(), m.version.clone());
                self.needs.insert(id.to_owned(), needs);
                return Ok(());
            }
            let fixed = if self.have.builtin.contains(id) {
                Some(Why::BuiltIn(m.version.clone()))
            } else if self.have.kept.contains(id) {
                Some(Why::Kept(m.version.clone()))
            } else {
                None
            };
            if let Some(why) = fixed {
                self.unmet(id, needs, why);
                return Ok(());
            }
        }
        let Some(addon) = self.catalog.iter().find(|a| a.id == id) else {
            self.unmet(id, needs, Why::NotListed(installed.map(|m| m.version.clone())));
            return Ok(());
        };
        let listed = addon.listed();
        let matching: Vec<&Version> = listed.iter().copied().filter(|v| fits(&v.version)).collect();
        let Some(v) = matching
            .iter()
            .find(|v| v.accepts(self.launcher))
            .map(|v| (*v).clone())
        else {
            let why = match matching.is_empty() {
                true => Why::NoMatch(listed.iter().map(|v| v.version.clone()).collect()),
                false => Why::NewerLauncher,
            };
            self.unmet(id, needs, why);
            return Ok(());
        };
        let addon = addon.clone();
        self.decided.insert(id.to_owned(), v.version.clone());
        self.planned.insert(id.to_owned());
        self.needs.insert(id.to_owned(), needs);
        self.visit(&addon, &v, read)?;
        self.steps.push(Step {
            replaces: installed.map(|m| m.version.clone()),
            addon,
            version: v,
        });
        Ok(())
    }

    /// The requirements the other installed addons have on `id` that its
    /// installed version meets, so replacing it keeps them met.
    fn dependents(&self, id: &str, at: &semver::Version) -> Vec<Need> {
        self.have
            .addons
            .values()
            .filter(|m| m.id != self.root && !self.planned.contains(&m.id))
            .flat_map(|m| {
                m.requires
                    .iter()
                    .filter(|r| r.id == id && crate::addons::version::satisfies(&r.version, at))
                    .map(|r| Need {
                        by: m.name.clone(),
                        version: r.version.clone(),
                    })
            })
            .collect()
    }

    fn unmet(&mut self, id: &str, needs: Vec<Need>, why: Why) {
        let name = self
            .catalog
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .or_else(|| self.have.addons.get(id).map(|m| m.name.clone()));
        self.problems.push(Problem::Unmet {
            id: id.to_owned(),
            name,
            needs,
            why,
        });
    }
}

/// What installing `v` of `addon` installs. `read` gives a version's
/// requirements. A requirement the installed addons meet needs nothing; the
/// rest come from `catalog`, newest matching version first. Fails with
/// [`Error::Requires`] when one can't be met.
pub fn plan(
    addon: &Addon,
    v: &Version,
    catalog: &[Addon],
    have: &Have<'_>,
    launcher: &semver::Version,
    read: &mut Reader<'_>,
) -> Result<Plan> {
    let mut learned: BTreeMap<String, Vec<Need>> = BTreeMap::new();
    for _ in 0..ROUNDS {
        let mut walk = Walk {
            catalog,
            have,
            launcher,
            learned: &learned,
            root: addon.id.clone(),
            decided: BTreeMap::from([(addon.id.clone(), v.version.clone())]),
            planned: BTreeSet::new(),
            needs: BTreeMap::new(),
            stack: Vec::new(),
            steps: Vec::new(),
            problems: Vec::new(),
            retry: None,
        };
        walk.visit(addon, v, read)?;
        // The root can itself be a dependency of an already installed addon.
        // Protect those dependents just as we do when replacing a requirement.
        if let Some(installed) = have.addons.get(addon.id.as_str()) {
            let needs: Vec<_> = walk
                .dependents(&addon.id, &installed.version)
                .into_iter()
                .filter(|need| !crate::addons::version::satisfies(&need.version, &v.version))
                .collect();
            if !needs.is_empty() {
                walk.unmet(&addon.id, needs, Why::Installing(v.version.clone()));
            }
        }
        if let Some((id, need)) = walk.retry.take() {
            learned.entry(id).or_default().push(need);
            continue;
        }
        if !walk.problems.is_empty() {
            return Err(Error::Requires(Refusal {
                addon: addon.name.clone(),
                problems: walk.problems,
            }));
        }
        let mut steps = walk.steps;
        steps.push(Step {
            addon: addon.clone(),
            version: v.clone(),
            replaces: have.addons.get(addon.id.as_str()).map(|m| m.version.clone()),
        });
        return Ok(Plan { steps });
    }
    Err(Error::Other(format!(
        "{}'s requirements ask for versions that keep changing",
        addon.name
    )))
}

/// [`plan`] from the requirements the registry lists, or none when a version
/// it reaches lists none; the install then reads them from the download.
pub fn preview(
    addon: &Addon,
    v: &Version,
    catalog: &[Addon],
    have: &Have<'_>,
    launcher: &semver::Version,
) -> Option<Result<Plan>> {
    let mut unknown = false;
    let result = plan(addon, v, catalog, have, launcher, &mut |_, v| match &v.requires {
        Some(r) => Ok(r.clone()),
        None => {
            unknown = true;
            Ok(Vec::new())
        }
    });
    (!unknown).then_some(result)
}
