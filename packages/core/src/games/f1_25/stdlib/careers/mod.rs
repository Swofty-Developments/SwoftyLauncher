//! The career index (docs/std/dependencies.md "Careers remember what they
//! were made with"). The core gives each career an id and a dependency
//! record, saved in its save entry and mirrored to
//! `$DATA/careers/index.json`, the launcher's data folder, which the launcher
//! names to the core in `<game>/swofty/state.json`, with the launcher profile
//! that started it. Before 1.6.0 the core wrote `<game>/swofty/careers/index.json`;
//! the launcher merges that file into the data folder's by career id and
//! removes it, so the list survives a restore of the game folder, and checks
//! each career's addons against a profile.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod index_file;
mod list;
mod play;
mod record;
mod resume;
mod rows;

pub use index_file::{
    data_path, forget, keep, kept_ids, migrate, parse, read, rename_profile, replace_profile, sync,
};
pub use list::{
    ListSave, READY as SEVERITY_READY, ResumeRow, RowAddon, VERSION as SEVERITY_VERSION, date_text,
    list_save, resume_list_for, resume_rows, save_file_careers, wanted as resume_list_wanted,
};
pub use play::{in_save_of, missing, needing, play_line, stop_line, unowned_line};
pub use record::{lost_cars, record_cars, record_session, refresh_versions};
pub use resume::{
    CareerNeeds, MissingRows, ResumeNeed, RowSet, mark_missing_modes, mode_owners, mode_suffix, of_profile,
    resume_needs, session_needs,
};
pub use rows::{Played, settle_rows, stock_max_for};

pub const GAME_PATH: &str = "swofty/careers/index.json";
pub const DATA_PATH: &str = "careers/index.json";

/// The `why` older launchers gave an addon whose database rows were on in a session that
/// saved the career. It says the addon was on, never that the save holds its rows.
pub const ROWS_WHY: &str = "database rows in the save";

/// The `why` older launchers gave an addon whose track any session loaded before the career
/// was saved. It says the track was driven in some mode, never that the career races it.
pub const TRACK_WHY: &str = "a track its sessions loaded";

/// The `why` of an addon that owns the team, car or driver row the career's save slot names.
pub const OWN_WHY: &str = "the career's team, car or driver";

/// The `why` of an addon that owns the vehicle row of the career's My Team car, which the core found
/// missing when it loaded the save.
pub const CAR_WHY: &str = "the career's My Team car";

fn is_guess(a: &CareerAddon) -> bool {
    matches!(a.why.as_deref(), Some(ROWS_WHY | TRACK_WHY))
}

fn is_proven(a: &CareerAddon) -> bool {
    matches!(a.why.as_deref(), Some(OWN_WHY | CAR_WHY))
}

fn launcher_wrote(a: &CareerAddon) -> bool {
    is_guess(a) || is_proven(a)
}

/// Whether addon version `now` matches the version `made_with` a career records with no range: the same
/// version, or a patch update of it (the same major and minor, and newer).
fn same_or_patch(made_with: &str, now: &semver::Version) -> bool {
    made_with == now.to_string()
        || crate::stdlib::range::version(made_with)
            .is_some_and(|was| (was.major, was.minor) == (now.major, now.minor) && *now >= was)
}

fn label_of(c: &Career) -> &str {
    if c.label.is_empty() { &c.career } else { &c.label }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CareerAddon {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Career {
    pub career: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub addons: Vec<CareerAddon>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Career {
    /// Whether the career is gone from the save: missing from the last profile load or deleted in game.
    pub fn is_gone(&self) -> bool {
        matches!(self.status(), "missing" | "deleted")
    }

    /// Whether a save slot backs the record: the core found the career in the game's save slots.
    /// A record with no slot is one an older core kept under another id, or a career never saved.
    pub fn in_slots(&self) -> bool {
        self.field("slot").is_some()
    }

    /// Whether the career is in the save and needs an addon: one recorded only because it was on
    /// doesn't count.
    pub fn uses_addons(&self) -> bool {
        !self.is_gone() && self.in_slots() && self.addons.iter().any(|a| !is_guess(a))
    }

    /// `present` unless the core marked the career missing from the last profile load.
    pub fn status(&self) -> &str {
        self.extra
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("present")
    }

    /// The text field the core wrote beside the dependency record, such as `mode` or `driver`.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.extra
            .get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    }

    /// The guid of the career's save slot, from its `slot` field or the end of its id.
    pub fn guid(&self) -> Option<&str> {
        self.field("slot")
            .and_then(guid_of)
            .or_else(|| guid_of(&self.career))
    }

    /// The ids the core's record had before the loaded career's id replaced them.
    pub fn renamed_from(&self) -> impl Iterator<Item = &str> {
        self.extra
            .get("renamed_from")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }

    /// Whether the core listed the record from the save slots and has not loaded the career since.
    pub fn from_slots(&self) -> bool {
        self.field("source") == Some("slots")
    }

    /// Whether `other` is this career under another id: one save slot guid in one profile, or an id
    /// the core renamed one of them from. The core renames a career when it first loads it.
    pub fn same_career(&self, other: &Career) -> bool {
        if self.career == other.career {
            return true;
        }
        let guid = matches!((self.guid(), other.guid()), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b));
        (guid && same_profile(self.profile.as_deref(), other.profile.as_deref()))
            || self.renamed_from().any(|id| id == other.career)
            || other.renamed_from().any(|id| id == self.career)
    }
}

/// The save slot guid a career id or slot key ends in: 16 hex digits, not all zero. A slot the game
/// gave no guid is keyed by its slot number, which another mode's slot can share.
pub fn guid_of(key: &str) -> Option<&str> {
    let tail = key.rsplit(':').next().unwrap_or(key);
    (tail.len() == 16 && tail.bytes().all(|b| b.is_ascii_hexdigit()) && tail.bytes().any(|b| b != b'0'))
        .then_some(tail)
}

/// Whether two records can be one career's: the same profile, or one has none yet.
pub fn same_profile(a: Option<&str>, b: Option<&str>) -> bool {
    a.is_none() || b.is_none() || a == b
}

#[cfg(test)]
mod tests;
