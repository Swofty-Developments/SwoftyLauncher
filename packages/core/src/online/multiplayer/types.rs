//! The data the session module reports, as the pages render it. Field names are the module's JSON.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::row_ids::Overrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    #[serde(rename = "f1-2026")]
    F12026,
    #[serde(rename = "f1-2025")]
    F12025,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::F12026 => "f1-2026",
            Category::F12025 => "f1-2025",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    MainMenu,
    LanLobby,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    None,
    Host,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cover {
    pub step: String,
    pub percent: u8,
}

/// What the game last reported about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameStatus {
    pub screen: Screen,
    pub in_session: bool,
    pub role: Role,
    #[serde(default)]
    pub configured: bool,
    #[serde(default, rename = "self")]
    pub self_member: u8,
    #[serde(default)]
    pub base: u16,
    #[serde(default)]
    pub cover: Option<Cover>,
    #[serde(default)]
    pub room_failed: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// The content name of the F1 2026 cars in [`RoomContent::host`].
pub const F1_2026: &str = "f1_2026";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub public: bool,
    pub max_members: u8,
    pub category: Category,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            name: None,
            public: true,
            max_members: 20,
            category: Category::F12026,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSummary {
    pub code: String,
    pub name: String,
    pub host: User,
    pub members: u32,
    pub max_members: u32,
    pub category: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub addons: Option<RoomAddons>,
    #[serde(default)]
    pub created_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomAddons {
    pub count: u32,
    #[serde(default)]
    pub tracks: Vec<String>,
    #[serde(default)]
    pub list: Vec<RoomAddon>,
    /// The host's launcher version, from a server that lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<String>,
    /// The SHA-256 of the host's game executable, from a server that lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// The host's F1 25 FileVersion, from a server that lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dlc: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomAddon {
    pub id: String,
    pub name: String,
    /// The version, Addon Browser download, load order place and built-in flag of the host's copy, from a
    /// server that lists them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
}

/// The host's addons as the host's fingerprint lists them, which the session module reports beside its state
/// once the host's fingerprint arrived.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostPrint {
    pub digest: String,
    #[serde(default)]
    pub launcher: String,
    #[serde(default)]
    pub exe_sha256: String,
    /// The host's F1 25 FileVersion, from a module and a host launcher that know it.
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub dlc: Option<bool>,
    #[serde(default)]
    pub addons: Vec<HostAddon>,
    /// Addon id to option id to value.
    #[serde(default)]
    pub options: BTreeMap<String, BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostAddon {
    pub id: String,
    pub name: String,
    pub version: semver::Version,
    /// The SHA-256 of the Addon Browser download the host's copy came from.
    #[serde(default)]
    pub registry: Option<String>,
    /// Its place in the host's load order among the addons that change game data.
    #[serde(default)]
    pub order: Option<usize>,
    #[serde(default)]
    pub builtin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStage {
    Starting,
    Loading,
    InLobby,
}

impl MemberStage {
    pub fn word(self) -> &'static str {
        match self {
            Self::Starting => "Starting the game",
            Self::Loading => "Loading",
            Self::InLobby => "In the lobby",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    SigningIn,
    Joining,
    Connecting,
    StartingGame,
    Loading,
    InLobby,
}

/// The one step a session is at, with a percentage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub step: Step,
    pub label: String,
    pub percent: u8,
}

/// Something the player has to act on, as one plain sentence and whether trying again can help.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub text: String,
    pub retry: bool,
    /// One more sentence on what is wrong, shown under the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Where one step of a session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Active,
    Done,
    Failed,
    Skipped,
    /// A state a newer module names that this launcher doesn't know.
    #[serde(other)]
    Unknown,
}

/// What stopped a step: a stable code for logs, the sentence to show, and what can fix it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepProblem {
    pub code: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Whether trying again can help; without it the session's [`Problem`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Vec::is_empty")]
    pub fix_plan: Vec<FixStep>,
    /// See [`SessionStep::warns`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warning: bool,
}

/// One step of a session, in the order the module lists them. The module names the step by `id` (`account`,
/// `room`, `addons`, `build`, `game`, `network`, `lobby`, `race`); the launcher gives it its title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStep {
    pub id: String,
    pub state: StepState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<StepProblem>,
    /// See [`SessionStep::warns`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warning: bool,
}

impl SessionStep {
    /// A done step whose problem is a warning: something differs that the session goes on with, such as an
    /// addon difference the room launch can't fix while the game still starts. The module marks it
    /// `warning: true` on the step or on its problem.
    pub fn warns(&self) -> bool {
        self.state == StepState::Done && self.problem.as_ref().is_some_and(|p| self.warning || p.warning)
    }
}

/// A value a module may send in a shape this launcher doesn't read, read as its default instead of failing the
/// document around it.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", content = "reason", rename_all = "snake_case")]
pub enum Phase {
    Connecting(String),
    WaitingForGame(String),
    InLobby,
    Racing,
    Ended(String),
    Failed(String),
}

impl Phase {
    pub fn is_over(&self) -> bool {
        matches!(self, Phase::Ended(_) | Phase::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomInfo {
    pub code: String,
    pub name: String,
    pub public: bool,
    pub category: String,
    pub max_members: u32,
    pub started: bool,
    pub host: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintMatch {
    Unknown,
    Matches,
    Differs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayStatus {
    None,
    Pending,
    Ready,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberStats {
    pub packets_out: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub bytes_in: u64,
    /// Round-trip asks this player's bridge sent to the member's; 0 from a module before the asks.
    #[serde(default)]
    pub probes_out: u64,
    /// The answered asks' round trips; `None` until one comes back, and from a module before the asks.
    #[serde(default)]
    pub rtt: Option<Rtt>,
}

/// Round trips from this player's bridge to a member's and back through the relay, in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rtt {
    pub last_us: u32,
    pub min_us: u32,
    /// The first round trip, then 7/8 of the value before plus 1/8 of each new one.
    pub smoothed_us: u32,
    pub samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberState {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub number: u8,
    pub ip: String,
    pub host: bool,
    pub me: bool,
    pub connected: bool,
    pub fingerprint: PrintMatch,
    pub relay: RelayStatus,
    pub stage: Option<MemberStage>,
    pub traffic: Option<MemberStats>,
}

/// The module asks the launcher to start the game for the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchAsk {
    /// Changes with each ask, so the launcher acts once on each.
    pub seq: u32,
    /// What the build borrows from the host for this session.
    #[serde(default)]
    pub row_ids: Option<Overrides>,
    pub id_addons: usize,
    pub setting_addons: usize,
    /// The game is running but isn't what the session needs: the launcher closes it first.
    pub restart: bool,
    pub why: String,
    /// The direct start request the game reads at boot.
    #[serde(default)]
    pub direct_start: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomContent {
    pub host: Option<String>,
    pub owns_f1_2026: BTreeMap<String, bool>,
}

impl RoomContent {
    pub fn needs_f1_2026(&self) -> bool {
        self.host.as_deref() == Some(F1_2026)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub phase: Phase,
    pub host: bool,
    pub room: Option<RoomInfo>,
    pub self_member: Option<u8>,
    pub members: Vec<MemberState>,
    /// How this player's content differs from the host's, as the module reports it.
    pub mismatches: Vec<Value>,
    pub lobby_connected: bool,
    pub relay: Option<String>,
    pub base: Option<u16>,
    /// Traffic counters, as the module reports them.
    pub bridge: Option<Value>,
    pub game: Option<GameStatus>,
    pub last_error: Option<String>,
    pub launch: Option<LaunchAsk>,
    pub content: RoomContent,
    /// The step and the problem the module derives from this state; not part of the state's JSON.
    #[serde(skip)]
    pub progress: Option<Progress>,
    #[serde(skip)]
    pub problem: Option<Problem>,
    /// The steps the module reports beside this state, in order; empty from a module older than them.
    #[serde(skip)]
    pub steps: Vec<SessionStep>,
}

impl SessionState {
    pub fn new(host: bool) -> Self {
        Self {
            phase: Phase::Connecting("starting".into()),
            host,
            room: None,
            self_member: None,
            members: Vec::new(),
            mismatches: Vec::new(),
            lobby_connected: false,
            relay: None,
            base: None,
            bridge: None,
            game: None,
            last_error: None,
            launch: None,
            content: RoomContent::default(),
            steps: Vec::new(),
            progress: None,
            problem: None,
        }
    }

    /// The step the module reported with this state.
    pub fn step(&self) -> Progress {
        self.progress.clone().unwrap_or(Progress {
            step: Step::Connecting,
            label: "Connecting".into(),
            percent: 0,
        })
    }
}

/// One step of bringing this player's content in line with the host's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FixStep {
    Install {
        id: String,
        name: String,
        version: semver::Version,
        sha256: Option<String>,
    },
    Disable {
        id: String,
        name: String,
    },
    SetOption {
        addon: String,
        option: String,
        value: Option<Value>,
    },
    Reorder {
        order: Vec<String>,
    },
    Manual {
        line: String,
    },
}
