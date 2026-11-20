//! What installing an addon from the Addon Browser brings with it: every addon
//! its manifest `requires`, and theirs, at versions the catalog lists and this
//! launcher can install. Nothing installs when one of them can't be met.

use crate::profile::conflicts::and_list;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use semver::VersionReq;

use super::{Addon, Version};
use crate::addons::builtin::CORE_ID;
use crate::manifest::AddonManifest;
use crate::store::Store;

mod downloads;
mod install;
mod manifest;
mod walk;

pub use install::{Event, install, install_in_profile};
pub use walk::{plan, preview};

const ROUNDS: usize = 32;
const SETTLE: usize = 4;
const MANIFEST_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub addon: Addon,
    pub version: Version,
    /// The installed version this one replaces.
    pub replaces: Option<semver::Version>,
}

impl Step {
    pub fn label(&self) -> String {
        format!("{} {}", self.addon.name, self.version.version)
    }
}

/// The addons an install puts in place, in install order: every requirement
/// before the addon that needs it, the addon itself last.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn addon(&self) -> &Step {
        self.steps.last().expect("a plan ends with its addon")
    }

    /// The other addons it installs or updates.
    pub fn also(&self) -> &[Step] {
        &self.steps[..self.steps.len().saturating_sub(1)]
    }

    pub fn size(&self) -> u64 {
        self.steps.iter().map(|s| s.version.size).sum()
    }
}

/// The addons a player has, for resolving requirements against.
#[derive(Debug, Clone, Default)]
pub struct Have<'a> {
    pub addons: BTreeMap<&'a str, &'a AddonManifest>,
    /// Ones the launcher ships: always present, never replaced.
    pub builtin: BTreeSet<&'a str>,
    /// Ones the Addon Browser never replaces: managed addons and DB Editor addons.
    pub kept: BTreeSet<&'a str>,
}

impl<'a> Have<'a> {
    pub fn new(addons: impl IntoIterator<Item = &'a AddonManifest>, builtin: impl Fn(&str) -> bool) -> Self {
        let mut have = Self::default();
        for m in addons {
            let id = m.id.as_str();
            have.addons.insert(id, m);
            if id == CORE_ID || builtin(id) {
                have.builtin.insert(id);
            } else if crate::updates::managed::is_managed(id)
                || id.starts_with(crate::dbedit::package::ID_PREFIX)
            {
                have.kept.insert(id);
            }
        }
        have
    }

    pub fn of(store: &Store, addons: &'a BTreeMap<String, AddonManifest>) -> Self {
        Self::new(addons.values(), |id| store.is_builtin(id))
    }
}

/// One addon's requirement on another.
#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    /// The name of the addon that requires it.
    pub by: String,
    pub version: VersionReq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    /// The Addon Browser doesn't list it; the version installed, if one is.
    NotListed(Option<semver::Version>),
    /// None of the versions it lists, newest first, matches.
    NoMatch(Vec<semver::Version>),
    /// The versions that match need a newer launcher.
    NewerLauncher,
    /// It's built into this launcher at this version.
    BuiltIn(semver::Version),
    /// It's installed at this version, and the Addon Browser doesn't replace it.
    Kept(semver::Version),
    /// It's the addon being installed, at this version.
    Installing(semver::Version),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    Unmet {
        id: String,
        name: Option<String>,
        needs: Vec<Need>,
        why: Why,
    },
    /// Addons that require each other, each requiring the next and the last the first.
    Cycle(Vec<String>),
}

/// Why an addon can't be installed: the requirements nothing can meet.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub addon: String,
    pub problems: Vec<Problem>,
}

impl Refusal {
    /// The addons it needs that the Addon Browser can't supply, by id.
    pub fn missing(&self) -> Vec<&str> {
        self.problems
            .iter()
            .filter_map(|p| match p {
                Problem::Unmet { id, .. } => Some(id.as_str()),
                Problem::Cycle(_) => None,
            })
            .collect()
    }
}

fn wanted(needs: &[Need], label: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for by in needs.iter().map(|n| n.by.as_str()).collect::<BTreeSet<_>>() {
        let mut reqs: Vec<String> = Vec::new();
        for n in needs
            .iter()
            .filter(|n| n.by == by && n.version != VersionReq::STAR)
        {
            let req = n.version.to_string();
            if !reqs.contains(&req) {
                reqs.push(req);
            }
        }
        parts.push(match reqs.is_empty() {
            true => format!("{by} needs {label}"),
            false => format!("{by} needs {label} {}", reqs.join(", ")),
        });
    }
    and_list(&parts)
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmet { id, name, needs, why } => {
                let label = match name {
                    Some(name) if name != id => format!("{name} ({id})"),
                    _ => id.clone(),
                };
                f.write_str(&wanted(needs, &label))?;
                match why {
                    Why::NotListed(None) => write!(f, ", which isn't on the Addon Browser"),
                    Why::NotListed(Some(v)) => {
                        write!(f, ", which isn't on the Addon Browser; you have {v}")
                    }
                    Why::NoMatch(listed) => {
                        let listed: Vec<String> = listed.iter().map(|v| v.to_string()).collect();
                        write!(f, "; the Addon Browser has {}", and_list(&listed))
                    }
                    Why::NewerLauncher => write!(f, "; the versions that match need a newer launcher"),
                    Why::BuiltIn(v) => write!(f, "; this launcher has {v} built in"),
                    Why::Kept(v) => write!(f, "; you have {v}, which the Addon Browser doesn't replace"),
                    Why::Installing(v) => write!(f, "; this installs {v}"),
                }
            }
            Self::Cycle(ids) => match ids.as_slice() {
                [one] => write!(f, "{one} requires itself"),
                [a, b] => write!(f, "{a} and {b} require each other"),
                many => write!(f, "{} require each other in a loop", and_list(many)),
            },
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.problems {
            write!(f, "{p}. ")?;
        }
        write!(f, "Nothing was installed.")
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
mod tests;
