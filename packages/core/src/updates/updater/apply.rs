//! Running a plan: each part's install as one step of the progress, what installed and what failed, and
//! whether the launcher restarts.

use semver::Version;

use super::plan::Plan;
use super::probe::Part;
use crate::launcher::Progress;
use crate::store::Store;
use crate::updates::managed;
use crate::updates::release::{self, Install, Release};
use crate::{Error, Result};

pub struct Step<'a> {
    pub(super) inner: &'a dyn Progress,
    pub(super) index: usize,
    pub(super) count: usize,
}

impl Progress for Step<'_> {
    fn report(&self, done: f32, text: &str) {
        let overall = (self.index as f32 + done.clamp(0., 1.)) / self.count.max(1) as f32;
        if self.count > 1 {
            self.inner
                .report(overall, &format!("{text} ({} of {})", self.index + 1, self.count));
        } else {
            self.inner.report(overall, text);
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Ran {
    pub done: Vec<(Part, Version)>,
    pub failed: Vec<(Part, String, String)>,
}

impl Ran {
    pub fn restart(&self) -> bool {
        self.launcher().is_some() || (!self.done.is_empty() && self.failed.is_empty())
    }

    fn version(&self, part: Part) -> Option<&Version> {
        self.done.iter().find(|(p, _)| *p == part).map(|(_, v)| v)
    }

    pub fn launcher(&self) -> Option<&Version> {
        self.version(Part::Launcher)
    }

    pub fn addon(&self) -> Option<&Version> {
        self.version(Part::Addon)
    }

    pub fn failed_tag(&self, part: Part) -> Option<&str> {
        self.failed
            .iter()
            .find(|(p, _, _)| *p == part)
            .map(|(_, tag, _)| tag.as_str())
    }

    pub fn error(&self) -> Option<String> {
        (!self.failed.is_empty()).then(|| {
            self.failed
                .iter()
                .map(|(part, tag, e)| format!("{} {tag}: {e}", part.name()))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
}

pub(super) type Installer<'a> = dyn FnMut(Part, &Release, &Version, &dyn Progress) -> Result<Version> + 'a;

pub(super) fn attempt(
    ran: &mut Ran,
    install: &mut Installer<'_>,
    part: Part,
    release: &Release,
    launcher: &Version,
    step: Step<'_>,
) -> Option<Version> {
    step.report(0., &format!("Updating {}", part.name()));
    match install(part, release, launcher, &step) {
        Ok(version) => {
            ran.done.push((part, version.clone()));
            Some(version)
        }
        Err(e) => {
            ran.failed.push((part, release.tag_name.clone(), e.to_string()));
            None
        }
    }
}

pub fn run(
    plan: &Plan,
    current: &Version,
    progress: &dyn Progress,
    mut install: impl FnMut(Part, &Release, &Version, &dyn Progress) -> Result<Version>,
) -> Ran {
    let count = plan.steps().len();
    let mut ran = Ran::default();
    let (mut next, mut addon, mut index) = (current.clone(), plan.addon.as_ref(), 0);
    if let Some(release) = &plan.launcher {
        let step = Step {
            inner: progress,
            index,
            count,
        };
        match attempt(&mut ran, &mut install, Part::Launcher, release, current, step) {
            Some(version) => next = version,
            None => addon = plan.fallback.as_ref(),
        }
        index += 1;
    }
    if let Some(release) = addon {
        let step = Step {
            inner: progress,
            index,
            count: count.max(index + 1),
        };
        attempt(&mut ran, &mut install, Part::Addon, release, &next, step);
    }
    ran
}

pub fn install(
    store: &Store,
    copy: Option<&Install>,
    part: Part,
    release: &Release,
    launcher: &Version,
    progress: &dyn Progress,
) -> Result<Version> {
    if crate::platform::process::game_running() {
        return Err(Error::Other("close F1 25 before updating".into()));
    }
    match part {
        Part::Addon => managed::install_release(store, release, launcher, progress),
        Part::Launcher => copy
            .ok_or_else(|| Error::Other("this copy cannot update itself".into()))?
            .update(release, progress),
    }
}

pub fn did_not_take(expected: Option<&str>, current: &Version) -> Option<String> {
    let wanted = release::parse_version(expected?)?;
    (wanted > *current).then(|| {
        format!("Swofty Launcher {wanted} was installed, but the restarted launcher is still {current}")
    })
}
