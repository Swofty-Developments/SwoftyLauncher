use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::view::event::Button;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
    #[serde(default)]
    pub action: Option<Button>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Neutral,
    Muted,
    Accent,
    Good,
    Warn,
    Bad,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Primary,
    #[default]
    Outline,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Art {
    #[default]
    None,
    Monogram(String),
    Image(PathBuf),
}

impl Art {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Icon {
    Activity,
    ArrowRightLeft,
    ChevronDown,
    ChevronUp,
    CircleAlert,
    CircleCheck,
    Copy,
    Eraser,
    FolderOpen,
    Gamepad,
    Layers,
    Lock,
    Map,
    Package,
    Pencil,
    Play,
    Plus,
    Puzzle,
    Settings,
    ShieldCheck,
    Trash,
    Upload,
    Calendar,
    Car,
    Clock,
    CloudRain,
    Flag,
    Gauge,
    History,
    Info,
    Refresh,
    Save,
    Search,
    Timer,
    TrendingDown,
    TrendingUp,
    TriangleAlert,
    Trophy,
    Undo,
    User,
    Users,
    Wallet,
    Wrench,
    Zap,
}
