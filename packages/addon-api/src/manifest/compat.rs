use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::manifest::validate::valid_id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Compat {
    pub game: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game_builds: Vec<VersionReq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<VersionReq>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
}

/// A store a game copy comes from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Steam,
    Ea,
}

impl Platform {
    pub const ALL: [Platform; 2] = [Platform::Steam, Platform::Ea];

    pub fn id(self) -> &'static str {
        match self {
            Platform::Steam => "steam",
            Platform::Ea => "ea",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
}

impl Default for Compat {
    fn default() -> Self {
        Self {
            game: crate::GAME_ID.to_owned(),
            game_builds: Vec::new(),
            launcher: None,
            seasons: Vec::new(),
            platforms: Vec::new(),
        }
    }
}

impl Compat {
    /// The platforms the addon lists as tested, once each; empty means not tested.
    pub fn tested_platforms(&self) -> Vec<Platform> {
        let mut out: Vec<Platform> = self.platforms.iter().filter_map(|p| Platform::parse(p)).collect();
        out.sort();
        out.dedup();
        out
    }

    pub fn accepts_build(&self, build_id: u64) -> bool {
        let v = Version::new(build_id, 0, 0);
        self.game_builds.is_empty() || self.game_builds.iter().any(|r| r.matches(&v))
    }

    pub fn accepts_launcher(&self, version: &Version) -> bool {
        self.launcher.as_ref().is_none_or(|r| r.matches(version))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    #[serde(default = "any_version")]
    pub version: VersionReq,
}

fn any_version() -> VersionReq {
    VersionReq::STAR
}

/// An addon that must be on in the profile: an option's `needs`, or the `addon` of an op's `when`.
/// Written as the addon's id, or as `{ "id", "version" }` like a requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "serde_json::Value", into = "serde_json::Value")]
pub struct AddonNeed {
    pub id: String,
    pub version: VersionReq,
}

impl AddonNeed {
    pub fn parse(v: &serde_json::Value) -> std::result::Result<Self, String> {
        use serde_json::Value;
        let (id, version) = match v {
            Value::String(id) => (id.as_str(), None),
            Value::Object(o) => {
                if let Some(k) = o.keys().find(|k| *k != "id" && *k != "version") {
                    return Err(format!("{k:?} is not a key of an addon; use id and version"));
                }
                let id = o
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("an addon needs its id")?;
                match o.get("version") {
                    None => (id, None),
                    Some(Value::String(r)) => (id, Some(r.as_str())),
                    Some(r) => return Err(format!("version {r} is not a version range like \">=1.2.0\"")),
                }
            }
            v => return Err(format!("{v} is not an addon id or {{ \"id\", \"version\" }}")),
        };
        if !valid_id(id) {
            return Err(format!("{id:?} is not an addon id"));
        }
        let version = match version {
            None => VersionReq::STAR,
            Some(r) => VersionReq::parse(r).map_err(|e| format!("version {r:?}: {e}"))?,
        };
        Ok(Self {
            id: id.to_owned(),
            version,
        })
    }

    /// One addon, or a list of addons that must all be on.
    pub fn parse_list(v: &serde_json::Value) -> std::result::Result<Vec<Self>, String> {
        match v {
            serde_json::Value::Array(a) if a.is_empty() => Err("lists no addon".into()),
            serde_json::Value::Array(a) => a
                .iter()
                .enumerate()
                .map(|(i, v)| Self::parse(v).map_err(|e| format!("[{i}]: {e}")))
                .collect(),
            v => Self::parse(v).map(|n| vec![n]),
        }
    }
}

impl TryFrom<serde_json::Value> for AddonNeed {
    type Error = String;

    fn try_from(v: serde_json::Value) -> std::result::Result<Self, String> {
        Self::parse(&v)
    }
}

impl From<AddonNeed> for serde_json::Value {
    fn from(n: AddonNeed) -> Self {
        match n.version == VersionReq::STAR {
            true => n.id.into(),
            false => serde_json::json!({ "id": n.id, "version": n.version.to_string() }),
        }
    }
}

impl std::fmt::Display for AddonNeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.version == VersionReq::STAR {
            true => f.write_str(&self.id),
            false => write!(f, "{} {}", self.id, self.version),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touches: Option<String>,
}
