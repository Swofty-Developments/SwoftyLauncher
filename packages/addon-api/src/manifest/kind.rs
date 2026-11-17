use serde::{Deserialize, Serialize};

/// What an addon, or one of its options, is to a multiplayer session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Multiplayer {
    /// Every player needs it at the host's version, with the host's settings.
    Gameplay,
    /// Players may differ on it.
    Cosmetic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Seasons,
    Tracks,
    #[serde(alias = "liveries")]
    Cars,
    Helmets,
    Shaders,
    Career,
    Ai,
    Performance,
    Utility,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Self::Seasons,
        Self::Tracks,
        Self::Cars,
        Self::Helmets,
        Self::Shaders,
        Self::Career,
        Self::Ai,
        Self::Performance,
        Self::Utility,
    ];

    /// Categories from earlier launchers that read as none.
    pub const RETIRED: [&str; 7] = [
        "drivers-teams",
        "database",
        "gameplay",
        "ui",
        "audio",
        "tools",
        "other",
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Seasons => "Seasons",
            Self::Tracks => "Tracks",
            Self::Cars => "Cars",
            Self::Helmets => "Helmets",
            Self::Shaders => "Shaders",
            Self::Career => "Career",
            Self::Ai => "AI",
            Self::Performance => "Performance",
            Self::Utility => "Utility",
        }
    }

    /// Reads an optional category, taking `liveries` as cars and a
    /// [retired](Category::RETIRED) or unknown one as none.
    pub fn read<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<Self>, D::Error> {
        use serde::de::IntoDeserializer;
        Ok(Option::<String>::deserialize(d)?.and_then(|name| {
            Self::deserialize(IntoDeserializer::<serde::de::value::Error>::into_deserializer(
                name,
            ))
            .ok()
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    /// `db-editor`, or the id of the addon that generated it (`teamsimplified.mods`).
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    /// The file name the addon was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Database,
    Erp,
    Text,
    Ui,
    Files,
    Native,
    /// Merged into the game's shader packages, as the Simplified Suite loads `.serps` mods.
    Simplified,
}

/// An experimental engine of the in-game core, named as its standard library page's
/// Feature row names it, that an addon turns on by listing it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ExperimentalFeature {
    /// Drawing over the game with `swofty/overlay.hpp`.
    Overlay,
    /// Grading the game's picture with a [`Shader`] look.
    Shaders,
    /// Running [`PostFx`] passes on the game's picture.
    Postfx,
    /// Reading live car and session state with `swofty/telemetry.hpp`.
    Telemetry,
}

impl ExperimentalFeature {
    /// Its name in `swofty/assets.json`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Overlay => "overlay",
            Self::Shaders => "shaders",
            Self::Postfx => "postfx",
            Self::Telemetry => "telemetry",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overlay => "Overlay",
            Self::Shaders => "Shaders",
            Self::Postfx => "Post-processing",
            Self::Telemetry => "Telemetry",
        }
    }

    /// What it does in game, in the player's words.
    pub fn effect(self) -> &'static str {
        match self {
            Self::Overlay => "Draws over the game",
            Self::Shaders => "Changes the picture",
            Self::Postfx => "Changes the picture",
            Self::Telemetry => "Reads live race data",
        }
    }
}
