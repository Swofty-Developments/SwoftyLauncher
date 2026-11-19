use std::path::Path;

use swofty_core::profile::VANILLA;
use swofty_core::saves::stall;
use swofty_core::saves::vault::{self, Vault};
use swofty_i18n::{fill, tr, trf, trn};

use super::labels::row_name;

pub(crate) struct Missing {
    pub(super) profile: String,
    pub(super) source: String,
    pub(super) snapshot: String,
    pub(super) kept_at: i64,
    pub(super) names: Vec<String>,
    pub(super) generation: u64,
    pub(super) modes: Vec<String>,
}

impl Missing {
    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn modes(&self) -> &[String] {
        &self.modes
    }

    pub(crate) fn kept_at(&self) -> i64 {
        self.kept_at
    }
}

pub(crate) struct Stalled {
    pub(super) profile: String,
    pub(super) source: String,
    pub(super) sha256: String,
    pub(super) snapshot: Option<String>,
    pub(super) snapshot_at: Option<i64>,
    pub(super) sessions: usize,
}

pub(crate) enum Banner {
    Dropped(Missing),
    Lost(Missing),
    Stalled(Stalled),
}

pub(super) struct Shown {
    pub(super) text: String,
    pub(super) source: String,
    pub(super) snapshot: String,
    pub(super) kind: Kind,
    pub(super) generation: u64,
    pub(super) modes: Vec<String>,
    pub(super) sha256: String,
    pub(super) restorable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Dropped,
    Lost,
    Stalled,
}

impl Banner {
    pub(crate) fn lost(&self) -> Option<&Missing> {
        match self {
            Self::Lost(m) => Some(m),
            _ => None,
        }
    }

    pub(super) fn shown(&self) -> Shown {
        match self {
            Self::Dropped(m) => Shown {
                text: fill(
                    &trn(
                        m.names.len(),
                        "{count} career is missing from {whose}: {names}. It was last in the save on {when}. Restoring that version brings it back; the restore shows what else changes first.",
                        "{count} careers are missing from {whose}: {names}. They were last in the save on {when}. Restoring that version brings them back; the restore shows what else changes first.",
                    ),
                    &[
                        ("whose", whose(&m.profile)),
                        ("names", m.names.join("; ")),
                        ("when", vault::human(m.kept_at)),
                    ],
                ),
                source: m.source.clone(),
                snapshot: m.snapshot.clone(),
                kind: Kind::Dropped,
                generation: 0,
                modes: Vec::new(),
                sha256: String::new(),
                restorable: true,
            },
            Self::Lost(m) => Shown {
                text: fill(
                    &trn(
                        m.names.len(),
                        "{count} career is in the last save a launcher session wrote but not in the save the game read since: {names}. The launcher's saved copy from {when} still has it. Restore puts it back the next time you press Play.",
                        "{count} careers are in the last save a launcher session wrote but not in the save the game read since: {names}. The launcher's saved copy from {when} still has them. Restore puts them back the next time you press Play.",
                    ),
                    &[("names", m.names.join("; ")), ("when", vault::human(m.kept_at))],
                ),
                source: m.source.clone(),
                snapshot: m.snapshot.clone(),
                kind: Kind::Lost,
                generation: m.generation,
                modes: m.modes.clone(),
                sha256: String::new(),
                restorable: true,
            },
            Self::Stalled(s) => Shown {
                text: match s.snapshot_at {
                    Some(at) => fill(
                        tr(
                            "{whose} kept F1 25 on Loading... after Press any button in {count} session(s). The launcher left it as it is and kept a copy. The version from {when} loaded before; Restore puts it back, and the save as it is stays in Versions.",
                        ),
                        &[
                            ("whose", whose_cap(&s.profile)),
                            ("count", s.sessions.to_string()),
                            ("when", vault::human(at)),
                        ],
                    ),
                    None => fill(
                        tr(
                            "{whose} kept F1 25 on Loading... after Press any button in {count} session(s). The launcher left it as it is and kept a copy, and holds no earlier version to restore.",
                        ),
                        &[
                            ("whose", whose_cap(&s.profile)),
                            ("count", s.sessions.to_string()),
                        ],
                    ),
                },
                source: s.source.clone(),
                snapshot: s.snapshot.clone().unwrap_or_default(),
                kind: Kind::Stalled,
                generation: 0,
                modes: Vec::new(),
                sha256: s.sha256.clone(),
                restorable: s.snapshot.is_some(),
            },
        }
    }
}

pub(super) fn whose(profile: &str) -> String {
    match profile == VANILLA {
        true => tr("your Steam save").to_owned(),
        false => trf!("profile {profile}'s save", profile = profile),
    }
}

fn whose_cap(profile: &str) -> String {
    match profile == VANILLA {
        true => tr("Your Steam save").to_owned(),
        false => trf!("The save of profile {profile}", profile = profile),
    }
}

pub(super) fn banner_from(root: &Path, source: String, profile: String) -> Option<Banner> {
    let v = Vault::open(root, &source).ok()?;
    let table = v.careers();
    let shown = table.drop_to_show();
    if let Some(newest) = shown.iter().max_by_key(|r| r.last_seen_at) {
        return Some(Banner::Dropped(Missing {
            profile,
            source,
            snapshot: newest.last_seen.clone(),
            kept_at: newest.last_seen_at,
            names: shown.iter().map(|r| row_name(r)).collect(),
            generation: 0,
            modes: Vec::new(),
        }));
    }
    if let Some(loss) = v.probable_loss() {
        return Some(Banner::Lost(Missing {
            profile,
            source,
            snapshot: loss.snapshot.clone().unwrap_or_default(),
            kept_at: loss.at,
            names: loss.names(),
            generation: loss.generation(),
            modes: loss.plain.modes(),
        }));
    }
    stall::offers(root)
        .into_iter()
        .find(|o| o.source == source)
        .map(|o| {
            Banner::Stalled(Stalled {
                profile,
                source,
                sha256: o.sha256,
                snapshot: o.snapshot,
                snapshot_at: o.snapshot_at,
                sessions: o.sessions,
            })
        })
}
