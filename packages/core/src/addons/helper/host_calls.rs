//! The calls a helper program makes back into the launcher.

use std::path::PathBuf;

use serde_json::Value;
use swofty_addon::helper::{ADDON_CALLS, HostCall, MOVE_ADDON, PROFILE_HEADS, SAVE_DIR, TASKS};
use swofty_addon::launcher::Cx;

use super::protocol::to_value;

// Tasks is listed always, since a helper keeps its hello across calls; lists sent
// where nothing shows them are dropped.
pub(super) fn host_calls(cx: &Cx) -> Vec<String> {
    let mut calls: Vec<&str> = match cx.addons() {
        Some(_) => ADDON_CALLS.into_iter().chain([MOVE_ADDON]).collect(),
        None => Vec::new(),
    };
    calls.push(TASKS);
    if cx.game.is_some() {
        calls.push(PROFILE_HEADS);
    }
    calls.push(SAVE_DIR);
    calls.into_iter().map(String::from).collect()
}

pub(super) fn host_call(cx: &Cx, call: HostCall) -> std::result::Result<Value, String> {
    let game = || {
        cx.game
            .ok_or_else(|| swofty_addon::Error::Other("the game is not available".into()))
    };
    let addons = || {
        cx.addons()
            .ok_or_else(|| swofty_addon::Error::Other("addons can't be managed from here".into()))
    };
    match call {
        HostCall::GetState => cx.state::<Option<Value>>().map(|v| v.unwrap_or(Value::Null)),
        HostCall::SetState(value) => cx.set_state(&value).map(|()| Value::Null),
        HostCall::GameBuildKey => Ok(cx.game.map_or(Value::Null, |g| Value::String(g.build_key()))),
        HostCall::GameExeVersion => Ok(cx
            .game
            .and_then(|g| g.exe_version())
            .map_or(Value::Null, Value::String)),
        HostCall::GameIsStock { rel } => Ok(Value::Bool(cx.game.is_some_and(|g| g.is_stock(&rel)))),
        HostCall::GameOriginal { rel } => game()
            .and_then(|g| g.original(&rel))
            .map(|p| Value::String(p.to_string_lossy().into_owned())),
        HostCall::GameOriginalLang { code } => game()
            .and_then(|g| g.original_lang(&code))
            .map(|p| Value::String(p.to_string_lossy().into_owned())),
        HostCall::GameDir => Ok(path_value(cx.game.and_then(|g| g.dir()))),
        HostCall::AddonGameDir => Ok(path_value(cx.game.and_then(|g| g.addon_dir(cx.addon)))),
        HostCall::RegistryDir => Ok(path_value(Some(crate::stdlib::reports::dir(cx.data_dir)))),
        HostCall::GameStatus => {
            let status = cx
                .game
                .map_or_else(|| crate::platform::process::game_status(None), |g| g.status());
            serde_json::to_value(status).map_err(|e| swofty_addon::Error::Other(e.to_string()))
        }
        HostCall::SteamSaveDir => Ok(path_value(cx.game.and_then(|g| g.steam_save_dir()))),
        HostCall::SaveDir => Ok(path_value(cx.game.and_then(|g| g.save_dir(&cx.profile.name)))),
        HostCall::ProfileTracks => match cx.game {
            Some(g) => g
                .profile_tracks(&cx.profile.name)
                .and_then(|t| serde_json::to_value(t).map_err(|e| swofty_addon::Error::Other(e.to_string()))),
            None => Ok(Value::Null),
        },
        HostCall::ProfileHeads => match cx.game {
            Some(g) => g.profile_heads(&cx.profile.name).and_then(to_value),
            None => Ok(Value::Null),
        },
        HostCall::Addons => addons().and_then(|a| a.list()).and_then(to_value),
        HostCall::InstallAddon { path } => addons().and_then(|a| a.install(&path)).and_then(to_value),
        HostCall::SetAddonEnabled { id, on } => addons()
            .and_then(|a| a.set_enabled(&id, on))
            .map(|()| Value::Null),
        HostCall::RemoveAddon { id } if id == cx.addon => {
            Err(swofty_addon::Error::Other(format!("{id} can't remove itself")))
        }
        HostCall::RemoveAddon { id } => addons().and_then(|a| a.remove(&id)).map(|()| Value::Null),
        HostCall::MoveAddon { id, delta } => addons()
            .and_then(|a| a.move_addon(&id, delta))
            .map(|()| Value::Null),
        HostCall::Progress { .. } => Err(swofty_addon::Error::Other("progress takes no id".into())),
        HostCall::Tasks(_) => Err(swofty_addon::Error::Other("tasks takes no id".into())),
        HostCall::Invalidate(_) => Err(swofty_addon::Error::Other("invalidate takes no id".into())),
    }
    .map_err(|e| e.to_string())
}

pub(super) fn path_value(path: Option<PathBuf>) -> Value {
    path.map_or(Value::Null, |p| Value::String(p.to_string_lossy().into_owned()))
}
