use std::path::Path;

use serde::{Deserialize, Serialize};

/// A shader config an addon ships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShaderConfigDecl {
    /// Path of the `.swfx` file.
    pub config: String,
    /// Whether the config is on, and locked, while the addon is enabled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
}

impl ShaderConfigDecl {
    /// The file name without `.swfx`: the config's id within its addon.
    pub fn stem(&self) -> &str {
        Path::new(&self.config)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".swfx"))
            .unwrap_or(&self.config)
    }
}

/// A colour look: a `.cube` 3D LUT graded in display colour, as a screenshot is, applied to
/// the game's picture after its tonemap and before the HUD.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shader {
    /// The `.cube` file inside the addon. `{{id}}` names a `db` option, so a choice can pick
    /// between looks.
    pub lut: String,
    /// How much of the look applies, from 0 to 100, or `{{id}}` naming a `db` slider over that
    /// range. Defaults to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<Strength>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Strength {
    Percent(f64),
    Option(String),
}
