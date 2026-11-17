use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub database: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub erp: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native: Vec<NativePlugin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screens: Vec<ScreenOp>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<String>,
    /// Files of `hud.element` records the build composes into the HUD's dial scenes
    /// (`docs/reference/ops/hud.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hud: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NativePlugin {
    pub dll: String,
    // Accepted for compatibility; declaring a capability has no effect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub early: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScreenOp {
    pub name: String,
    /// The screen's `.ui`; empty when `page` has the launcher generate it.
    #[serde(default)]
    pub file: String,
    /// `options`: the launcher builds this addon's options page, as ADDON SETTINGS shows it, as
    /// this screen, opened from `anchor` by the route `open_<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default = "default_flow")]
    pub flow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default)]
    pub view_model: bool,
}

fn default_flow() -> String {
    "flow_frontend".into()
}
