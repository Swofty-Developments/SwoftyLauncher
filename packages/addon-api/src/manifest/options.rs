use serde::{Deserialize, Serialize};

use crate::manifest::compat::AddonNeed;
use crate::manifest::kind::Multiplayer;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptionType {
    Toggle,
    Slider,
    Choice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Binds {
    Db,
    Native,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ChoiceRepr", into = "ChoiceRepr")]
pub struct Choice {
    pub value: String,
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ChoiceRepr {
    Plain(String),
    Labelled { value: String, label: String },
}

impl From<ChoiceRepr> for Choice {
    fn from(r: ChoiceRepr) -> Self {
        match r {
            ChoiceRepr::Plain(value) => Self { value, label: None },
            ChoiceRepr::Labelled { value, label } => Self {
                value,
                label: Some(label),
            },
        }
    }
}

impl From<Choice> for ChoiceRepr {
    fn from(c: Choice) -> Self {
        match c.label {
            None => Self::Plain(c.value),
            Some(label) => Self::Labelled {
                value: c.value,
                label,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddonOption {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: OptionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub default: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    pub binds: Binds,
    /// Kept off the addon's ADDON SETTINGS page in game, such as a developer option or one only a career setup
    /// page or hub tab shows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Addons a toggle needs on in the profile: while one is off, the toggle is off and locked, and its
    /// stored value is kept for when the addon is back on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<AddonNeed>,
    /// `cosmetic` leaves the option out of the multiplayer match of a gameplay addon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplayer: Option<Multiplayer>,
}
