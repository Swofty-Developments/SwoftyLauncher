use std::path::Path;

use serde::{Deserialize, Serialize};

/// A post-process pass: an HLSL compute shader written against `swofty/postfx.hlsli`, which
/// the launcher compiles to DXIL at build time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PostFx {
    /// The `.hlsl` file inside the addon.
    pub shader: String,
    /// The pass's name in the build output and the core's log. Defaults to the file's stem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Up to 16 values the shader reads with `option(i)`: numbers, or `{{id}}` naming a `db`
    /// option whose value is a number or a toggle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<PassValue>,
    /// Where the pass runs among every addon's passes: lower runs first, and equal values keep
    /// the load order.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub order: i32,
    /// GPU milliseconds a frame past which the core turns the pass off. Defaults to 1.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_ms: Option<f64>,
    /// The function the pass runs: a `float4 name(float2 uv, uint2 pixel)`, or with `raw` the
    /// compute entry point. Defaults to `effect`, or `main` with `raw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// The shader defines its own compute entry point and writes `swofty_output` itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raw: bool,
    /// The thread group of a `raw` pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<[u32; 2]>,
    /// Up to 8 textures the shader reads with `input(i, uv)`: an addon texture, `original` or
    /// `previous`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// The addon texture the pass writes. Omitted, the pass writes the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Where the pass runs, by the game's view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<PostFxWhen>,
    /// `{{id}}` naming a toggle option that turns the pass on and off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<String>,
}

/// The views a pass runs in and the states it stays out of.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PostFxWhen {
    /// A subset of [`POSTFX_VIEWS`]. Empty means every view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<String>,
    /// A subset of [`POSTFX_SKIPS`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<String>,
}

/// A texture a pass reads or writes: sized from the picture or fixed, or an image from the addon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PostFxTexture {
    pub name: String,
    /// The size as a multiple of the picture, above 0 and at most 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// One of [`POSTFX_FORMATS`]. Defaults to `rgba16f`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Kept from one frame to the next instead of cleared.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub persistent: bool,
    /// A read-only `.png` inside the addon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

pub const POSTFX_FORMATS: [&str; 7] = [
    "rgba8",
    "rgba16f",
    "rg16f",
    "r16f",
    "r32f",
    "rgb10a2",
    "r11g11b10f",
];

pub const POSTFX_VIEWS: [&str; 11] = [
    "cockpit",
    "tcam",
    "tcam_offset",
    "chase_near",
    "chase_far",
    "other",
    "trackside",
    "reverse_pod",
    "nose",
    "nose_alt",
    "wing",
];

pub const POSTFX_SKIPS: [&str; 6] = ["paused", "replay", "photo", "menu", "cutscene", "spectating"];

pub const POSTFX_INPUTS: usize = 8;

/// Names an addon's textures cannot take.
pub const POSTFX_RESERVED: [&str; 4] = ["scene", "original", "previous", "depth"];

/// The values a `postfx_toggle` choice may list, besides `A`-`Z`, `0`-`9` and `F1`-`F24`.
pub const POSTFX_KEYS: [&str; 6] = ["Insert", "Home", "End", "Page Up", "Page Down", "None"];

pub(crate) fn texture_name_ok(n: &str) -> bool {
    (1..=32).contains(&n.len())
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

pub(crate) fn is_identifier(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|f| f.is_ascii_alphanumeric() || f == '_')
}

/// Whether `v` names a key `postfx_toggle` accepts.
pub fn postfx_key_ok(v: &str) -> bool {
    let mut c = v.chars();
    let single =
        matches!((c.next(), c.next()), (Some(f), None) if f.is_ascii_uppercase() || f.is_ascii_digit());
    let f_key = v
        .strip_prefix('F')
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && n.to_string() == v[1..]);
    single || f_key || POSTFX_KEYS.contains(&v)
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

impl PostFx {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            Path::new(&self.shader)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.shader.clone())
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PassValue {
    Number(f64),
    Option(String),
}

/// Most values a pass reads with `option(i)`.
pub const POSTFX_OPTIONS: usize = 16;

/// The option ids a `{{id}}` template names, in order.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        out.push(rest[start + 2..start + end].trim());
        rest = &rest[start + end + 2..];
    }
    out
}
