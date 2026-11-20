//! The `swofty/assets.json` document the in-game core reads.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Assets {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub early_stage: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asset_groups: Vec<Group>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resource_erps: Vec<ResErp>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub db_edits: Vec<DbEdit>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub lang: BTreeMap<String, LangTable>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overlay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shader: Option<ShaderLook>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub postfx: Vec<PostFxPass>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub postfx_textures: Vec<PostFxTexture>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub postfx_toggles: Vec<PostFxToggle>,
}

/// A post-process pass the in-game core runs, in the order listed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostFxPass {
    /// The DXIL container, relative to `swofty/`.
    pub shader: String,
    pub name: String,
    pub addon: String,
    /// The values the shader reads with `option(i)`.
    pub options: Vec<f64>,
    pub budget_ms: f64,
    pub addon_id: String,
    /// The option slots the core reads from the addon's settings every frame, instead of `options`.
    pub live: Vec<PostFxLive>,
    /// Texture names, namespaced `addon_id/name`, or `original` and `previous`.
    pub inputs: Vec<String>,
    /// The texture the pass writes, namespaced; empty for the picture.
    pub output: String,
    pub threads: [u32; 2],
    pub raw: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<PostFxWhen>,
    /// The id of a live toggle option that turns the pass on and off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostFxLive {
    pub slot: usize,
    pub option: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostFxWhen {
    pub views: Vec<String>,
    pub skip: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostFxTexture {
    /// Namespaced `addon_id/name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    /// The `.tex` image, relative to `swofty/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostFxToggle {
    pub addon_id: String,
    pub option: String,
}

/// The look the in-game core grades the picture with.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShaderLook {
    /// The packed LUT, relative to `swofty/`.
    pub lut: String,
    /// From 0 (the game's own colours) to 1 (the look in full).
    pub strength: f64,
    /// The addon's name, for the core's log.
    pub addon: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub group: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub erps: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResErp {
    pub file: String,
    pub erps: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DbEdit {
    pub hash: String,
    #[serde(rename = "type")]
    pub tag: u8,
    pub value: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub retype: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LangTable {
    pub table: String,
}
