use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A file named by `operations.hud`: `hud.element` records keyed by record id, in the shape of a
/// registry data file. Other registries in the same file are left to the registry loader.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HudFile {
    #[serde(default, rename = "hud.element")]
    pub elements: BTreeMap<String, serde_json::Value>,
}

impl HudFile {
    pub fn parse(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| Error::Other(format!("hud file: {e}")))
    }
}
