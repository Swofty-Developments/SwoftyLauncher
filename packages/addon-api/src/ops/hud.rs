use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A file named by `operations.hud`: `hud.element` records keyed by record id, in the shape of a
/// registry data file. Other registries in the same file are left to the registry loader.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HudFile {
    #[serde(default, rename = "hud.element")]
    pub elements: BTreeMap<String, serde_json::Value>,
    /// `hud.slot` records: an addon's own entries in the game's HUD editor, keyed by slot id.
    #[serde(default, rename = "hud.slot", skip_serializing_if = "BTreeMap::is_empty")]
    pub slots: BTreeMap<String, serde_json::Value>,
    /// The option values and `addon` the file's records are built for, as an operations file's `when`;
    /// with none they always are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<serde_json::Value>,
}

impl HudFile {
    pub fn parse(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| Error::Other(format!("hud file: {e}")))
    }
}
