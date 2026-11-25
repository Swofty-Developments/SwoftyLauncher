use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::view::selection::SelectionDialog;
use crate::view::style::{Icon, Style};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Switch {
        id: String,
        on: bool,
        enabled: bool,
    },
    Choice {
        id: String,
        options: Vec<String>,
        current: usize,
    },
    Button(Button),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<Icon>,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub kind: ButtonKind,
    #[serde(default)]
    pub size: ButtonSize,
}

impl Button {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            style: Style::default(),
            kind: ButtonKind::default(),
            size: ButtonSize::default(),
        }
    }
}

/// How prominently a button is drawn. `Large` is for a page's main call to
/// action, such as the Simplified Importer's Choose a mod button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonSize {
    #[default]
    Normal,
    Large,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonKind {
    #[default]
    Event,
    /// A native choice dialog; confirmation sends Selected, cancellation sends nothing.
    Selection(SelectionDialog),
    Confirm {
        title: String,
        text: String,
    },
    Pick {
        title: String,
        extensions: Vec<String>,
        folders: bool,
        multiple: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Selected {
        id: String,
        values: Vec<String>,
    },
    Click {
        id: String,
    },
    Toggle {
        id: String,
        on: bool,
    },
    Choose {
        id: String,
        index: usize,
    },
    Move {
        row: String,
        to: usize,
    },
    Picked {
        id: String,
        paths: Vec<PathBuf>,
    },
    Submit {
        form: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<String>,
        values: serde_json::Map<String, serde_json::Value>,
        #[serde(default)]
        changed: Vec<String>,
    },
    Search {
        id: String,
        query: String,
    },
    Tab {
        id: String,
        tab: String,
    },
    /// Sent once per launcher start to a section whose addon declares
    /// `launcher.startup`, for long work such as converting mods.
    Startup {},
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    None,
    Reload,
    Notice(String),
    SetAddonEnabled(bool),
    Open(PathBuf),
    /// Switches to the Addons page and highlights this addon.
    ShowAddon(Shown),
}

/// A `show_addon` target: the addon's id, or `{ "id", "notice" }` to also
/// show the notice as a notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Shown {
    Id(String),
    With {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notice: Option<String>,
    },
}

impl Shown {
    pub fn new(id: impl Into<String>, notice: impl Into<String>) -> Self {
        Self::With {
            id: id.into(),
            notice: Some(notice.into()),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Id(id) | Self::With { id, .. } => id,
        }
    }

    pub fn notice(&self) -> Option<&str> {
        match self {
            Self::Id(_) => None,
            Self::With { notice, .. } => notice.as_deref(),
        }
    }
}

impl From<String> for Shown {
    fn from(id: String) -> Self {
        Self::Id(id)
    }
}

impl From<&str> for Shown {
    fn from(id: &str) -> Self {
        Self::Id(id.into())
    }
}
