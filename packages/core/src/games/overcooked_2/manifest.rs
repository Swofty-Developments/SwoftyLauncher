//! An Overcooked 2 addon's `swofty.json`: who it is, the game it is for and the assemblies the core loads.

use std::path::{Component, Path};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use swofty_addon::game::{GameId, OVERCOOKED_2};

use crate::manifest::{AddonManifest, MANIFEST_FILE, Requirement};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compat {
    pub game: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub std: Option<VersionReq>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub compat: Compat,
    #[serde(default)]
    pub assemblies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<serde_json::Value>,
}

/// A path inside the addon folder: relative, and never above it.
fn inside(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.is_empty()
        && !rel.contains('\\')
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// An id the addon's folder can be named after.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

impl Manifest {
    /// Reads and checks the `swofty.json` in `dir`.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(MANIFEST_FILE);
        if let Some(why) = super::super::other_game_refusal(dir, GameId::Overcooked2) {
            return Err(Error::Manifest(why));
        }
        let m: Self = crate::read_json(&path)?;
        m.check()?;
        Ok(m)
    }

    pub fn check(&self) -> Result<()> {
        if self.compat.game != OVERCOOKED_2.id {
            return Err(Error::Manifest(format!(
                "compat.game {} is not {}",
                self.compat.game, OVERCOOKED_2.id
            )));
        }
        if !valid_id(&self.id) {
            return Err(Error::Manifest(format!(
                "{:?} is not an addon id: use letters, digits, '.', '-' and '_'",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(Error::Manifest(format!("{} has no name", self.id)));
        }
        if let Some(bad) = self
            .assemblies
            .iter()
            .chain(self.icon.iter())
            .find(|a| !inside(a))
        {
            return Err(Error::Manifest(format!(
                "{bad} is not a path inside the addon's folder"
            )));
        }
        Ok(())
    }

    /// The assemblies `dir` lacks.
    pub fn missing(&self, dir: &Path) -> Vec<&str> {
        self.assemblies
            .iter()
            .filter(|a| !dir.join(a).is_file())
            .map(String::as_str)
            .collect()
    }

    /// The addon as the catalog, the Addon Browser and the requirement checks read an addon.
    pub fn listing(&self) -> AddonManifest {
        let value = serde_json::json!({
            "schema_version": 1,
            "id": self.id,
            "name": self.name,
            "version": self.version.to_string(),
            "author": self.author,
            "description": self.description,
            "icon": self.icon,
            "compat": { "game": self.compat.game },
            "requires": self.requires,
        });
        serde_json::from_value(value).expect("an Overcooked 2 manifest reads as an addon listing")
    }
}
