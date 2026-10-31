//! The vault's career table: `careers.json` in the vault folder, one row per
//! career id, updated from the career index each snapshot stores. A career in
//! the index of one snapshot and missing from the next one is
//! `missing-since:<snapshot>`. Nothing here reads F1 25's save: the index comes
//! from the core, so a launcher with no index yet (first run, EA before the
//! core ran, core off) has no rows and never claims a career is gone.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::stdlib::careers::{Career, CareerAddon};

pub const FILE: &str = "careers.json";
pub const INDEX_DIR: &str = "index";
const MISSING: &str = "missing-since:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Present,
    MissingSince(String),
    DeletedInGame,
    Archived,
}

impl Status {
    pub fn slug(&self) -> String {
        match self {
            Self::Present => "present".to_owned(),
            Self::MissingSince(s) => format!("{MISSING}{s}"),
            Self::DeletedInGame => "deleted-in-game".to_owned(),
            Self::Archived => "archived".to_owned(),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "deleted-in-game" => Self::DeletedInGame,
            "archived" => Self::Archived,
            _ => match s.strip_prefix(MISSING) {
                Some(id) => Self::MissingSince(id.to_owned()),
                None => Self::Present,
            },
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, Self::MissingSince(_))
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.slug())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(d)?))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub career: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub addons: Vec<CareerAddon>,
    pub first_seen: String,
    pub first_seen_at: i64,
    /// The newest snapshot whose index held the career: the one to restore it from.
    pub last_seen: String,
    pub last_seen_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<i64>,
    pub status: Status,
    /// When a check found the career missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found_at: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dismissed: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Row {
    pub fn name(&self) -> &str {
        match self.label.is_empty() {
            true => &self.career,
            false => &self.label,
        }
    }

    pub fn needs(&self) -> impl Iterator<Item = &str> {
        self.addons.iter().map(|a| a.id.as_str())
    }

    pub fn place(&self) -> Option<String> {
        match (self.season, self.round) {
            (Some(s), Some(r)) => Some(format!("season {s}, round {r}")),
            (Some(s), None) => Some(format!("season {s}")),
            _ => None,
        }
    }
}

fn text(c: &Career, key: &str) -> Option<String> {
    c.field(key).map(str::to_owned)
}

fn number(c: &Career, key: &str) -> Option<i64> {
    c.extra.get(key).and_then(|v| v.as_i64())
}

fn deleted(c: &Career) -> bool {
    c.status() == "deleted"
}

/// Careers an index says are in the save.
pub fn in_save(index: &[Career]) -> impl Iterator<Item = &Career> {
    index
        .iter()
        .filter(|c| !matches!(c.status(), "missing" | "deleted"))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Careers {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub careers: Vec<Row>,
}

fn one() -> u32 {
    1
}

impl Careers {
    pub fn read(vault: &std::path::Path) -> Self {
        std::fs::read(vault.join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn write(&self, vault: &std::path::Path) -> crate::Result<()> {
        crate::write_json_shared(&vault.join(FILE), self)
    }

    pub fn find(&self, id: &str) -> Option<&Row> {
        self.careers.iter().find(|r| r.career == id)
    }

    pub fn missing(&self) -> impl Iterator<Item = &Row> {
        self.careers.iter().filter(|r| r.status.is_missing())
    }

    pub fn drop_to_show(&self) -> Vec<&Row> {
        self.missing().filter(|r| !r.dismissed).collect()
    }

    pub fn dismiss(&mut self) -> bool {
        let mut any = false;
        for r in self
            .careers
            .iter_mut()
            .filter(|r| r.status.is_missing() && !r.dismissed)
        {
            r.dismissed = true;
            any = true;
        }
        any
    }

    pub fn archive(&mut self, id: &str) -> bool {
        match self.careers.iter_mut().find(|r| r.career == id) {
            Some(r) => {
                r.status = Status::Archived;
                true
            }
            None => false,
        }
    }

    /// Fold the index stored with snapshot `snapshot` (taken at `at`) into the
    /// rows. Returns the careers newly found missing, as `(career, the snapshot
    /// that last held it)`.
    pub fn observe(&mut self, snapshot: &str, at: i64, index: &[Career]) -> Vec<(String, String)> {
        let mut dropped = Vec::new();
        let here: BTreeSet<&str> = in_save(index).map(|c| c.career.as_str()).collect();
        let gone: BTreeSet<&str> = index
            .iter()
            .filter(|c| deleted(c))
            .map(|c| c.career.as_str())
            .collect();
        for c in in_save(index) {
            let fresh = |r: &mut Row| {
                r.mode = text(c, "mode").or(r.mode.take());
                if !c.label.is_empty() {
                    r.label = c.label.clone();
                }
                r.driver = text(c, "driver").or(r.driver.take());
                r.team = text(c, "team").or(r.team.take());
                r.season = number(c, "season").or(r.season);
                r.round = number(c, "round").or(r.round);
                r.profile = c.profile.clone().or(r.profile.take());
                r.last_played = number(c, "last_saved").map(|ms| ms / 1000).or(r.last_played);
                r.addons = c.addons.clone();
                r.last_seen = snapshot.to_owned();
                r.last_seen_at = at;
            };
            match self.careers.iter_mut().find(|r| r.career == c.career) {
                Some(r) => {
                    fresh(r);
                    if r.status != Status::Archived {
                        if r.status != Status::Present {
                            r.found_at = None;
                            r.dismissed = false;
                        }
                        r.status = Status::Present;
                    }
                }
                None => {
                    let mut r = Row {
                        career: c.career.clone(),
                        mode: None,
                        label: String::new(),
                        driver: None,
                        team: None,
                        season: None,
                        round: None,
                        profile: None,
                        addons: Vec::new(),
                        first_seen: snapshot.to_owned(),
                        first_seen_at: at,
                        last_seen: snapshot.to_owned(),
                        last_seen_at: at,
                        last_played: None,
                        status: Status::Present,
                        found_at: None,
                        dismissed: false,
                    };
                    fresh(&mut r);
                    self.careers.push(r);
                }
            }
        }
        for r in self.careers.iter_mut() {
            if here.contains(r.career.as_str()) {
                continue;
            }
            match (&r.status, gone.contains(r.career.as_str())) {
                (Status::Archived | Status::DeletedInGame, _) => {}
                (Status::Present, true) => r.status = Status::DeletedInGame,
                (Status::Present, false) => {
                    r.status = Status::MissingSince(snapshot.to_owned());
                    r.found_at = Some(at);
                    r.dismissed = false;
                    dropped.push((r.career.clone(), r.last_seen.clone()));
                }
                (Status::MissingSince(_), true) => r.status = Status::DeletedInGame,
                (Status::MissingSince(_), false) => {}
            }
        }
        dropped
    }
}

/// A career as the restore panel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    pub career: String,
    pub name: String,
    pub place: Option<String>,
}

fn brief(c: &Career) -> Brief {
    let place = match (number(c, "season"), number(c, "round")) {
        (Some(s), Some(r)) => Some(format!("season {s}, round {r}")),
        (Some(s), None) => Some(format!("season {s}")),
        _ => None,
    };
    Brief {
        career: c.career.clone(),
        name: match c.label.is_empty() {
            true => c.career.clone(),
            false => c.label.clone(),
        },
        place,
    }
}

/// What restoring a version does to the careers, from two indexes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// In the version, not in the save now.
    pub get_back: Vec<Brief>,
    /// In the save now, not in the version.
    pub lose: Vec<Brief>,
    /// In both, with further progress now than in the version: `(now, in the version)`.
    pub progress: Vec<(Brief, Brief)>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.get_back.is_empty() && self.lose.is_empty() && self.progress.is_empty()
    }
}

fn pos(c: &Career) -> (i64, i64) {
    (number(c, "season").unwrap_or(0), number(c, "round").unwrap_or(0))
}

/// `now` is the index of the save as it is; `version` the index stored with
/// the snapshot to restore.
pub fn diff(now: &[Career], version: &[Career]) -> Diff {
    let have: BTreeMap<&str, &Career> = in_save(now).map(|c| (c.career.as_str(), c)).collect();
    let then: BTreeMap<&str, &Career> = in_save(version).map(|c| (c.career.as_str(), c)).collect();
    let mut out = Diff::default();
    for (id, c) in &then {
        match have.get(id) {
            None => out.get_back.push(brief(c)),
            Some(n) if pos(n) > pos(c) => out.progress.push((brief(n), brief(c))),
            Some(_) => {}
        }
    }
    for (id, c) in &have {
        if !then.contains_key(id) {
            out.lose.push(brief(c));
        }
    }
    out
}
