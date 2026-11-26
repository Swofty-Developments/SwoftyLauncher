mod compat;
mod hud;
mod kind;
mod launcher;
mod operations;
mod options;
mod postfx;
mod shader;
mod validate;

#[cfg(test)]
mod tests;

use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub use compat::{AddonNeed, Compat, Conflict, Platform, Requirement};
pub use hud::{Hud, HudMfd, MFD_ROWS_MAX, MFD_TEXT_MAX};
pub use kind::{Category, ExperimentalFeature, Kind, Multiplayer, Origin};
pub use launcher::{
    CAREER_SETUP_BACK, CAREER_SETUP_NEXT, CAREER_SETUP_ROUTE_LEN, CareerSetup, HUB_TAB_OPTIONS, HubTab,
    Launcher, Saves,
};
pub use operations::{FileEntry, NativePlugin, Operations, RouteOp, ScreenOp};
pub use options::{AddonOption, Binds, Choice, OptionType};
pub use postfx::{
    POSTFX_FORMATS, POSTFX_INPUTS, POSTFX_KEYS, POSTFX_OPTIONS, POSTFX_RESERVED, POSTFX_SKIPS, POSTFX_VIEWS,
    PassValue, PostFx, PostFxTexture, PostFxWhen, placeholders, postfx_key_ok,
};
pub use shader::{Shader, ShaderConfigDecl, Strength};
pub use validate::{https_url, relative_inside, valid_id, valid_screen_name};

pub const MANIFEST_FILE: &str = "swofty.json";

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddonManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: Version,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Where the addon's source code is published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind: Vec<Kind>,
    /// Experimental engines of the core the addon uses; the build turns them on while it is on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub experimental_features: Vec<ExperimentalFeature>,
    #[serde(default)]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Conflict>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_after: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AddonOption>,
    #[serde(default)]
    pub operations: Operations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<Launcher>,
    #[serde(default, skip_serializing_if = "Saves::is_unset")]
    pub saves: Saves,
    /// What the addon is for, for grouping and filtering; `kind` says what it touches.
    #[serde(
        default,
        deserialize_with = "Category::read",
        skip_serializing_if = "Option::is_none"
    )]
    pub category: Option<Category>,
    /// The tool that generated the addon and the file it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    /// A colour look applied to the game's picture, before the HUD draws.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<Shader>,
    /// Post-process passes run on the game's picture after its tonemap and before the HUD.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postfx: Vec<PostFx>,
    /// Render targets and images the [`PostFx`] passes read and write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postfx_textures: Vec<PostFxTexture>,
    /// A native `choice` option whose value is the key that turns the addon's passes on and off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postfx_toggle: Option<String>,
    /// Shader configs (`.swfx`) the addon ships; they are listed in Shaders whether the addon is on
    /// or off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shaders: Vec<ShaderConfigDecl>,
    /// Shader effects (`<id>.effect.json`) the addon adds to the effect library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    /// A settings page in game between a Driver Career tile and the new-career wizard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub career_setup: Option<CareerSetup>,
    /// A settings tab in the career hub's laptop (Team HQ) for careers of one data group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_tab: Option<HubTab>,
    /// Whether players in a multiplayer session must match on the addon; see
    /// [`AddonManifest::multiplayer`] for the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplayer: Option<Multiplayer>,
    /// What the addon adds to the race HUD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hud: Option<Hud>,
}

impl AddonManifest {
    /// The link to the addon's source code, when it names one the launcher may open.
    pub fn source_url(&self) -> Option<&str> {
        self.repository.as_deref().and_then(https_url)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(MANIFEST_FILE);
        let data = std::fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let m: Self = serde_json::from_slice(&data).map_err(|source| Error::Json { path, source })?;
        m.validate()?;
        Ok(m)
    }

    pub fn parse(json: &str) -> Result<Self> {
        let m: Self = serde_json::from_str(json).map_err(|e| Error::Manifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn is_builtin(&self) -> bool {
        self.id == crate::CORE_ID
    }

    /// Whether the addon ships shader configs or effects and nothing else, so it has no switch.
    pub fn is_shader_only(&self) -> bool {
        (!self.shaders.is_empty() || !self.effects.is_empty())
            && self.operations == Operations::default()
            && self.options.is_empty()
            && self.launcher.is_none()
            && self.hud.is_none()
            && self.hub_tab.is_none()
            && self.career_setup.is_none()
            && self.multiplayer.is_none()
            && self.shader.is_none()
            && self.postfx.is_empty()
            && self.postfx_textures.is_empty()
            && self.postfx_toggle.is_none()
    }

    /// `multiplayer` when set; otherwise cosmetic for a `helmets`, `shaders` or `performance`
    /// addon and for one that ships only a `shader` look or `postfx` passes, and gameplay for the
    /// rest.
    pub fn multiplayer(&self) -> Multiplayer {
        if let Some(m) = self.multiplayer {
            return m;
        }
        let look_only = (self.shader.is_some() || !self.postfx.is_empty())
            && self.operations == Operations::default()
            && !self.launcher.as_ref().is_some_and(|l| l.build);
        let cosmetic = matches!(
            self.category,
            Some(Category::Helmets | Category::Shaders | Category::Performance)
        );
        if look_only || cosmetic {
            Multiplayer::Cosmetic
        } else {
            Multiplayer::Gameplay
        }
    }
}
