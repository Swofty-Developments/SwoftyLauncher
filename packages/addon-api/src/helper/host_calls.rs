use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::launcher::TaskList;

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
