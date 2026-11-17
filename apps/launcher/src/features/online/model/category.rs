//! Board rows and the category a board is filtered to: assists and input device.

use serde::{Deserialize, Serialize};

use super::overview::Driver;
use super::setup::Setup;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub rank: u32,
    pub lap_id: String,
    pub driver: Driver,
    pub time_ms: u64,
    pub sectors_ms: Vec<u64>,
    pub car: Option<String>,
    pub date: String,
    pub input: Option<InputDevice>,
    pub aids: Option<Assists>,
    pub setup: Option<Setup>,
    pub game_replay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPoint {
    pub time_ms: u64,
    pub driver: Driver,
    pub date: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistFilter {
    #[default]
    Either,
    None,
    Assists,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceFilter {
    #[default]
    All,
    Wheel,
    Controller,
    Keyboard,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub assists: AssistFilter,
    pub device: DeviceFilter,
}

impl Category {
    pub fn query(&self) -> String {
        let mut out = String::new();
        match self.assists {
            AssistFilter::Either => {}
            AssistFilter::None => out.push_str("&assists=none"),
            AssistFilter::Assists => out.push_str("&assists=any"),
        }
        match self.device {
            DeviceFilter::All => {}
            DeviceFilter::Wheel => out.push_str("&device=wheel"),
            DeviceFilter::Controller => out.push_str("&device=controller"),
            DeviceFilter::Keyboard => out.push_str("&device=keyboard"),
        }
        out
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn suffix(&self) -> String {
        let mut parts = Vec::new();
        match self.assists {
            AssistFilter::Either => {}
            AssistFilter::None => parts.push("No assists"),
            AssistFilter::Assists => parts.push("Assists"),
        }
        match self.device {
            DeviceFilter::All => {}
            DeviceFilter::Wheel => parts.push("Wheel"),
            DeviceFilter::Controller => parts.push("Controller"),
            DeviceFilter::Keyboard => parts.push("Keyboard"),
        }
        parts.join(" - ")
    }

    fn path(data: &std::path::Path) -> std::path::PathBuf {
        data.join("state").join("tt-category.json")
    }

    pub fn load(data: &std::path::Path) -> Self {
        std::fs::read(Self::path(data))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data: &std::path::Path) {
        let path = Self::path(data);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        if let Ok(json) = serde_json::to_vec(self) {
            std::fs::write(path, json).ok();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDevice {
    Wheel,
    Controller,
    Keyboard,
}

impl InputDevice {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "wheel" => Some(Self::Wheel),
            "pad" | "controller" | "gamepad" => Some(Self::Controller),
            "keyboard" | "kb" => Some(Self::Keyboard),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Wheel => "Wheel",
            Self::Controller => "Controller",
            Self::Keyboard => "Keyboard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aid {
    TractionControl,
    Abs,
    BrakingAssist,
    SteeringAssist,
    RacingLine,
    Gearbox,
    Ers,
}

impl Aid {
    pub const ALL: [Aid; 7] = [
        Aid::TractionControl,
        Aid::Abs,
        Aid::BrakingAssist,
        Aid::SteeringAssist,
        Aid::RacingLine,
        Aid::Gearbox,
        Aid::Ers,
    ];
    pub const TRIO: [Aid; 3] = [Aid::TractionControl, Aid::Abs, Aid::Gearbox];

    pub fn label(self) -> &'static str {
        match self {
            Self::TractionControl => "Traction control",
            Self::Abs => "ABS",
            Self::BrakingAssist => "Braking assist",
            Self::SteeringAssist => "Steering assist",
            Self::RacingLine => "Racing line",
            Self::Gearbox => "Gearbox",
            Self::Ers => "ERS / DRS assist",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "traction_control" | "trc" => Some(Self::TractionControl),
            "abs" => Some(Self::Abs),
            "braking_assist" | "brk" => Some(Self::BrakingAssist),
            "steering_assist" | "str" => Some(Self::SteeringAssist),
            "racing_line" | "drl" => Some(Self::RacingLine),
            "gearbox" | "gbx" => Some(Self::Gearbox),
            "ers" | "drs" => Some(Self::Ers),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assists {
    pub levels: Vec<(Aid, u32)>,
}

impl Assists {
    pub fn level(&self, aid: Aid) -> Option<u32> {
        self.levels.iter().find(|(a, _)| *a == aid).map(|(_, l)| *l)
    }

    pub fn on(&self, aid: Aid) -> bool {
        self.level(aid).is_some_and(|l| l > 0)
    }

    pub fn none(&self) -> bool {
        !self.levels.is_empty() && self.levels.iter().all(|(_, l)| *l == 0)
    }
}

pub fn shown_rank(index: usize, overall: u32, filtered: bool) -> (u32, Option<u32>) {
    let position = index as u32 + 1;
    match filtered && position != overall {
        true => (position, Some(overall)),
        false => (overall, None),
    }
}
