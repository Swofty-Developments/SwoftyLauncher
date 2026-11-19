use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    Load,
    Save,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Game,
    Kept,
    Injected,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub slot: u32,
    #[serde(default)]
    pub guid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub season: Option<u32>,
    #[serde(default)]
    pub round: Option<u32>,
    #[serde(default)]
    pub team: Option<u32>,
    #[serde(default)]
    pub driver: Option<u32>,
    #[serde(default)]
    pub last_played: i64,
    #[serde(default)]
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    pub name: String,
    #[serde(default)]
    pub fnv64: String,
    #[serde(default)]
    pub type_id: u32,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub registered: bool,
    pub source: Source,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub pair: Option<String>,
}

pub const SLOT_SUFFIX: &str = "_SaveSlotGameModeComponent";

/// The mode a container belongs to: `gm_<mode>` and `<mode>_SaveSlotGameModeComponent` are its two halves.
pub fn mode_of(name: &str) -> String {
    name.strip_prefix("gm_")
        .or_else(|| name.strip_suffix(SLOT_SUFFIX))
        .unwrap_or(name)
        .to_owned()
}

impl Container {
    pub fn mode(&self) -> String {
        self.pair.clone().unwrap_or_else(|| mode_of(&self.name))
    }

    pub fn file(&self) -> String {
        format!("{}.bag", self.fnv64)
    }

    pub fn same(&self, other: &Self) -> bool {
        match (self.fnv64.is_empty(), other.fnv64.is_empty()) {
            (false, false) => self.fnv64 == other.fnv64,
            _ => self.name == other.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub generation: u64,
    pub event: Event,
    #[serde(default)]
    pub written_at: i64,
    #[serde(default)]
    pub core_version: String,
    #[serde(default)]
    pub exe_sha256: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub loaded_addons: Vec<String>,
    #[serde(default)]
    pub containers: Vec<Container>,
}

impl Manifest {
    pub fn halves<'a>(&'a self, mode: &'a str) -> impl Iterator<Item = &'a Container> + 'a {
        self.containers.iter().filter(move |c| c.mode() == mode)
    }

    pub fn modes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.containers {
            let m = c.mode();
            if !out.contains(&m) {
                out.push(m);
            }
        }
        out
    }

    pub fn container(&self, name: &str) -> Option<&Container> {
        self.containers.iter().find(|c| c.name == name)
    }

    pub fn is_session_save(&self) -> bool {
        self.event == Event::Save && self.session.is_some()
    }
}
