use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use gpui_kit::assets::IconName;
use swofty_core::catalog::Stage;
use swofty_core::catalog::requires::Event;

use crate::format;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Progress {
    Downloading { done: u64, size: u64, rate: f64 },
    Verifying,
    Installing,
}

impl Progress {
    pub(super) fn fraction(self) -> f32 {
        match self {
            Self::Downloading { done, size, .. } if size > 0 => (done as f32 / size as f32).min(1.),
            Self::Downloading { .. } => 0.,
            Self::Verifying | Self::Installing => 1.,
        }
    }

    pub(super) fn short(self) -> String {
        match self {
            Self::Downloading { size: 0, .. } => "Downloading…".into(),
            Self::Downloading { .. } => format!("Downloading {:.0}%", self.fraction() * 100.),
            Self::Verifying => "Verifying…".into(),
            Self::Installing => "Installing…".into(),
        }
    }

    /// What it's doing, naming `item` when that's a requirement, not the addon itself.
    pub(super) fn long(self, item: Option<&str>) -> String {
        match self {
            Self::Downloading { done, size, rate } => {
                let mut text = match item {
                    Some(item) => format!("Downloading {item} · "),
                    None => "Downloading ".to_owned(),
                };
                if size == 0 {
                    text.push_str(&format::size(done));
                } else {
                    text.push_str(&format!(
                        "{} of {} · {:.0}%",
                        format::size(done),
                        format::size(size),
                        self.fraction() * 100.
                    ));
                }
                if rate > 0. {
                    text.push_str(&format!(" · {}/s", format::size(rate as u64)));
                }
                text
            }
            Self::Verifying => match item {
                Some(item) => format!("Verifying {item}'s size and SHA-256…"),
                None => "Verifying the download's size and SHA-256…".into(),
            },
            Self::Installing => match item {
                Some(item) => format!("Installing {item}…"),
                None => "Installing…".into(),
            },
        }
    }
}

/// An install's downloads so far: every addon it fetches, requirements included.
#[derive(Default)]
pub(super) struct Tally {
    pub(super) finished: u64,
    pub(super) current: u64,
    pub(super) size: u64,
    pub(super) stage: u8,
    pub(super) item: Option<String>,
    pub(super) also: Vec<String>,
    pub(super) installed: Vec<String>,
}

#[derive(Clone)]
pub(super) struct Job {
    pub(super) tally: Arc<Mutex<Tally>>,
    pub(super) started: Instant,
}

impl Job {
    pub(super) fn new(size: u64) -> Self {
        Self {
            tally: Arc::new(Mutex::new(Tally {
                size,
                ..Tally::default()
            })),
            started: Instant::now(),
        }
    }

    pub(super) fn tally(&self) -> std::sync::MutexGuard<'_, Tally> {
        self.tally.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn progress(&self) -> Progress {
        let t = self.tally();
        match t.stage {
            VERIFYING => Progress::Verifying,
            INSTALLING => Progress::Installing,
            _ => {
                let done = t.finished + t.current;
                let secs = self.started.elapsed().as_secs_f64();
                Progress::Downloading {
                    done,
                    size: t.size,
                    rate: if secs > 0.3 { done as f64 / secs } else { 0. },
                }
            }
        }
    }

    /// The requirement it's on, when it isn't on the addon itself.
    pub(super) fn item(&self) -> Option<String> {
        self.tally().item.clone()
    }

    /// The other addons it installs, once they're known.
    pub(super) fn also(&self) -> Vec<String> {
        self.tally().also.clone()
    }

    pub(super) fn failure(&self, error: &str) -> String {
        let installed = self.tally().installed.clone();
        if installed.is_empty() {
            error.to_owned()
        } else {
            format!("{error} · Already installed: {}", installed.join(", "))
        }
    }

    pub(super) fn track(&self, root: &str, e: Event) {
        let mut t = self.tally();
        match e {
            Event::Planned(plan) => {
                t.size = plan.size();
                t.also = plan.also().iter().map(|s| s.label()).collect();
            }
            Event::Addon(a, _) => {
                t.finished += t.current;
                t.current = 0;
                t.stage = 0;
                t.item = (a.id != root).then(|| a.name.clone());
            }
            Event::Stage(Stage::Downloading(n)) => t.current = n,
            Event::Stage(Stage::Verifying) => t.stage = VERIFYING,
            Event::Stage(Stage::Installing) => t.stage = INSTALLING,
            Event::Installed(m) if m.id != root => t.installed.push(format!("{} {}", m.name, m.version)),
            Event::Installed(_) => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Offer {
    Install,
    Installed(semver::Version),
    /// Installed, but not one of this profile's addons.
    Elsewhere,
    Update(semver::Version, semver::Version),
    TooOld,
    Queued,
    Working(Progress),
    Failed,
    /// Not installed, and its requirements can't all be met.
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Catalog,
    Stars,
    Install,
    Details,
    Profile,
}

impl Op {
    pub(super) fn icon(self) -> IconName {
        match self {
            Self::Catalog => IconName::RefreshCw,
            Self::Stars => IconName::Star,
            Self::Install => IconName::Download,
            Self::Details => IconName::Info,
            Self::Profile => IconName::UserRound,
        }
    }
}

pub(super) struct Activity {
    pub(super) at: SystemTime,
    pub(super) op: Op,
    pub(super) ok: bool,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) took: Duration,
}

#[derive(Default)]
pub(super) struct Batch {
    pub(super) left: BTreeSet<String>,
    pub(super) updated: Vec<String>,
    pub(super) failed: Vec<(String, String)>,
}

impl Batch {
    pub(super) fn total(&self) -> usize {
        self.left.len() + self.updated.len() + self.failed.len()
    }

    pub(super) fn summary(&self) -> String {
        let mut lines = Vec::new();
        if !self.updated.is_empty() {
            lines.push(format!("Updated {}", self.updated.join(", ")));
        }
        lines.extend(
            self.failed
                .iter()
                .map(|(name, e)| format!("Couldn't update {name}: {e}")),
        );
        lines.join("\n")
    }
}
