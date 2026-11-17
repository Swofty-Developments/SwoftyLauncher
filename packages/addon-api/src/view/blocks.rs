use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::view::Block;
use crate::view::event::Button;
use crate::view::form::number;
use crate::view::style::{Art, Fact, Icon, Tone};
use crate::view::table::yes;

/// A large drop target with one main call to action, such as the Simplified Importer's
/// Import a mod: an icon, a title, a line of text and a big button, centred in a
/// tall area. Files dropped on the window still go to the addon that claims them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropZone {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    pub button: Button,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub path: PathBuf,
    pub width: f32,
    pub height: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default)]
    pub fit: Fit,
    /// Draws the picture without the box behind it, so its transparent parts show what is
    /// under it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transparent: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    #[default]
    Cover,
    Contain,
}

/// A row of tabs. Clicking an enabled tab that isn't active sends `Event::Tab`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    pub id: String,
    pub items: Vec<TabItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TabItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Hero {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Art::is_none")]
    pub art: Art,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pills: Vec<Pill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<Fact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pill {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub tone: Tone,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
}

impl Tile {
    pub fn delta_tone(&self) -> Tone {
        if let Some(t) = self.delta_tone {
            return t;
        }
        match self.delta.as_deref().and_then(|d| d.trim_start().chars().next()) {
            Some('+') => Tone::Good,
            Some('-' | '\u{2212}') => Tone::Bad,
            _ => Tone::Muted,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    #[serde(default)]
    pub label: String,
    pub value: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
}

impl Meter {
    pub fn fraction(&self) -> f32 {
        let (min, max) = (self.min.unwrap_or(0.), self.max.unwrap_or(100.));
        if max <= min || !self.value.is_finite() {
            return 0.;
        }
        ((self.value - min) / (max - min)).clamp(0., 1.) as f32
    }

    pub fn text(&self) -> String {
        self.text
            .clone()
            .unwrap_or_else(|| format!("{} / {}", number(self.value), number(self.max.unwrap_or(100.))))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Card {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pills: Vec<Pill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Button>,
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// Draw the card with a dashed outline, for a placeholder or drop target
    /// such as the Simplified Importer's list of imported mods.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dashed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    #[serde(default)]
    pub columns: Columns,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Columns {
    Count(usize),
    Weights(Vec<f32>),
}

impl Default for Columns {
    fn default() -> Self {
        Self::Count(2)
    }
}

impl Columns {
    pub fn weights(&self) -> Vec<f32> {
        match self {
            Self::Count(n) => vec![1.; (*n).clamp(1, 4)],
            Self::Weights(w) if w.is_empty() => vec![1.],
            Self::Weights(w) => w.iter().map(|w| if *w > 0. { *w } else { 1. }).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Search {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub event: bool,
    #[serde(default = "yes")]
    pub filter: bool,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            id: None,
            placeholder: None,
            event: false,
            filter: true,
        }
    }
}

impl Search {
    pub fn matches<'a>(query: &str, texts: impl IntoIterator<Item = &'a str> + Clone) -> bool {
        query.split_whitespace().all(|word| {
            let word = word.to_lowercase();
            texts
                .clone()
                .into_iter()
                .any(|t| t.to_lowercase().contains(&word))
        })
    }
}
