use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::asset::{AssetOp, Features};
use crate::launcher::{ProfileInfo, TaskList};
use crate::view::Event;

pub use crate::run_helper::{claim_word, run_helper};

pub const API_VERSION: u32 = 1;
/// Optional support for the data-only native Chart view block.
pub const VIEW_CHART: &str = "view.chart";
/// Listed from launcher 1.10: the launcher may answer a `build` with an earlier reply that lists its `reads`.
pub const BUILD_READS: &str = "build.reads";
pub const RESTART: &str = "swofty.restart";
// The host calls a launcher lists in hello's host_calls when it answers them.
pub const ADDON_CALLS: [&str; 4] = ["addons", "install_addon", "set_addon_enabled", "remove_addon"];
// Listed from launcher 1.3.0, beside ADDON_CALLS.
pub const MOVE_ADDON: &str = "move_addon";
// Listed from launcher 1.3.0; a helper sends tasks only when hello lists it.
pub const TASKS: &str = "tasks";
// Listed from launcher 1.5.12 when the host knows the game.
pub const PROFILE_HEADS: &str = "profile_heads";
// Listed from launcher 1.9.29, after the other host calls.
pub const SAVE_DIR: &str = "save_dir";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request<C> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub call: C,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    Hello(Hello),
    Profile(ProfileInfo),
    View {
        section: String,
    },
    Event {
        section: String,
        event: Event,
    },
    Import {
        paths: Vec<PathBuf>,
    },
    Build {
        options: BTreeMap<String, Value>,
        features: Features,
    },
    Command {
        path: Vec<String>,
        args: Vec<String>,
    },
    Summary,
    Tick {
        section: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub api: u32,
    pub addon: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub profile: ProfileInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_calls: Vec<String>,
    /// Optional view/protocol features; absent on older hosts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum HostCall {
    Progress { done: f32, text: String },
    Tasks(TaskList),
    GetState,
    SetState(Value),
    GameBuildKey,
    GameExeVersion,
    GameIsStock { rel: String },
    GameOriginal { rel: String },
    GameOriginalLang { code: String },
    GameDir,
    AddonGameDir,
    RegistryDir,
    GameStatus,
    SteamSaveDir,
    SaveDir,
    ProfileTracks,
    ProfileHeads,
    Addons,
    InstallAddon { path: PathBuf },
    SetAddonEnabled { id: String, on: bool },
    RemoveAddon { id: String },
    MoveAddon { id: String, delta: isize },
    Invalidate(#[serde(default)] Invalidate),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invalidate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Ok { id: u64, result: Value },
    Err { id: u64, error: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Reply(Reply),
    Call(Request<HostCall>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub api: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Built {
    #[serde(default)]
    pub ops: Vec<AssetOp>,
    // The item each op comes from, by position in ops, such as one mod of the
    // addon's library. Core builds one runtime overlay archive per item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Option<String>>,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    // Field names for add-row, keyed `<table>.<row>` as in an op file's `fields`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<String>>,
    // The absolute paths of every file and folder the build read; a launcher that lists build.reads
    // answers the next build with this reply while none of them changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<Vec<PathBuf>>,
}

#[cfg(test)]
mod tests;
