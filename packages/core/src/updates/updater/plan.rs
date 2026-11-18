//! What to update and when: what the Play button offers while an update is pending or running, the plan of
//! releases to install, and how it is described.

use semver::Version;

use super::catalog::Catalog;
use super::probe::Part;
use crate::updates::release::Release;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Installing,
    Failed(String),
    Restarting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Play,
    Update,
    Installing,
    Retry,
    Restarting,
}

pub fn gate(phase: &Phase, pending: bool, game_busy: bool) -> Gate {
    match phase {
        Phase::Installing => Gate::Installing,
        Phase::Restarting => Gate::Restarting,
        _ if game_busy || !pending => Gate::Play,
        Phase::Failed(_) => Gate::Retry,
        Phase::Idle => Gate::Update,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub launcher: Option<Release>,
    pub addon: Option<Release>,
    pub fallback: Option<Release>,
}

impl Plan {
    pub fn steps(&self) -> Vec<(Part, &Release)> {
        [(Part::Launcher, &self.launcher), (Part::Addon, &self.addon)]
            .into_iter()
            .filter_map(|(part, release)| release.as_ref().map(|r| (part, r)))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.launcher.is_none() && self.addon.is_none()
    }

    pub fn versions(&self) -> Vec<(Part, Version)> {
        self.steps()
            .into_iter()
            .filter_map(|(part, r)| r.version().map(|v| (part, v)))
            .collect()
    }
}

// The importer is checked against the launcher that runs after this update,
// so one run can take a launcher and the importer that needs it.
pub fn plan(
    catalog: &Catalog,
    addon: Option<&Version>,
    launcher: Option<&Release>,
    current: &Version,
    held: Option<&str>,
) -> Plan {
    let pick = |target: &Version| {
        addon
            .and_then(|installed| catalog.choose(target, Some(installed)))
            .filter(|r| Some(r.tag_name.as_str()) != held)
            .cloned()
    };
    let fallback = pick(current);
    Plan {
        launcher: launcher.cloned(),
        addon: match launcher.and_then(Release::version) {
            Some(next) => pick(&next),
            None => fallback.clone(),
        },
        fallback,
    }
}

pub fn follow_up(restarted: bool, plan: &Plan) -> bool {
    restarted && plan.launcher.is_none() && plan.addon.is_some()
}

pub fn summary(parts: &[(Part, Version)]) -> String {
    match parts {
        [(part, version)] => format!("{} {version}", part.short()),
        _ => parts
            .iter()
            .map(|(part, _)| part.short())
            .collect::<Vec<_>>()
            .join(" + "),
    }
}

pub fn describe(parts: &[(Part, Version)]) -> String {
    parts
        .iter()
        .map(|(part, version)| format!("{} {version}", part.name()))
        .collect::<Vec<_>>()
        .join(" and ")
}
