//! One session's options for a game the launcher can't start itself. The EA app starts an EA app copy,
//! so nothing the launcher sets in the environment reaches that game. The session writes the variables
//! to [`FILE`] in the data folder instead; the core reads the file once at start, deletes it and takes
//! the variables only before `expires` (native/src/core/session_options_logic.hpp). Each session starts
//! by removing the file, so one a game never read can't reach a later game.
//!
//! A game that took its options from the file keeps the files it trades with the launcher (the command
//! mailbox, `netplay.json`, `entitlements.json`) under [`FILES_DIR`] in the data folder: the game can't
//! create files in `<game>/swofty` of a copy under Program Files. [`files_dir`] is the folder the
//! launcher reads them in, for either kind of copy.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;

use super::bridge::{ENV, TRACE_ENV};
use crate::install::Source;

/// The options file, under the data folder.
pub const FILE: &str = "state/session_options.json";
/// Under the data folder, what stands in for the game folder for a game armed through [`FILE`]: the
/// session's files are in its `swofty`, as they are in the game folder's.
pub const FILES_DIR: &str = "session";
/// How long after it is written the core still takes the file: the EA app has 5 minutes to start the
/// game, and the game a few more to load the core.
pub const LIFE: Duration = Duration::from_secs(600);

/// The variables the core takes from the file.
pub fn names() -> impl Iterator<Item = &'static str> {
    std::iter::once(ENV).chain(TRACE_ENV.iter().map(|(name, _)| *name))
}

/// Whether the copy in `game` gets its options through the file: the EA app starts it.
pub fn through_file(game: &Path) -> bool {
    Source::detect(game) == Source::EaApp
}

/// The folder whose `swofty` holds the files the game in `game` trades with the launcher: the game
/// folder, or [`FILES_DIR`] in the data folder for a copy armed [`through_file`].
pub fn files_dir(data: &Path, game: &Path) -> PathBuf {
    files_dir_for(data, game, through_file(game))
}

pub fn files_dir_for(data: &Path, game: &Path, through_file: bool) -> PathBuf {
    if through_file {
        data.join(FILES_DIR)
    } else {
        game.to_path_buf()
    }
}

/// The file's text for the launch `id`, written at `now` (Unix seconds). `env` is the game's
/// environment; only [`names`] are passed on. `None` when the game is not armed.
pub fn text<'a>(id: &str, now: u64, env: impl Fn(&str) -> Option<&'a str>) -> Option<String> {
    let passed: serde_json::Map<String, serde_json::Value> = names()
        .filter_map(|name| Some((name.to_owned(), json!(env(name).filter(|v| !v.is_empty())?))))
        .collect();
    if !passed.contains_key(ENV) {
        return None;
    }
    let mut text = serde_json::to_string_pretty(&json!({
        "version": 1,
        "id": id,
        "expires": now + LIFE.as_secs(),
        "env": passed,
    }))
    .expect("serialisable");
    text.push('\n');
    Some(text)
}

/// Remove an options file no game read.
pub fn clear(data: &Path) {
    let path = data.join(FILE);
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(path.with_extension("json.tmp")).ok();
}

/// Write the file for the launch `id` and make the folders the game answers in. Returns the file's
/// path, or `None` when `env` does not arm the game and nothing is written.
pub fn write<'a>(
    data: &Path,
    id: &str,
    now: u64,
    env: impl Fn(&str) -> Option<&'a str>,
) -> std::io::Result<Option<PathBuf>> {
    clear(data);
    let Some(text) = text(id, now, env) else {
        return Ok(None);
    };
    let path = data.join(FILE);
    let tmp = path.with_extension("json.tmp");
    let mailbox = super::game::mailbox(&data.join(FILES_DIR));
    for dir in [mailbox.join("inbox"), mailbox.join("outbox")] {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        std::fs::remove_file(&tmp).ok();
    })?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use super::super::bridge::{ARMED_ROOM, armed_env};
    use super::*;

    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-session-options-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn armed() -> BTreeMap<String, String> {
        let mut env: BTreeMap<String, String> = armed_env(ARMED_ROOM)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        env.insert("PATH".into(), "/bin".into());
        env.insert("SWOFTY_POSTFX".into(), "0".into());
        env
    }

    #[test]
    fn the_file_carries_the_arming_and_its_expiry_and_nothing_else_from_the_environment() {
        let env = armed();
        let text = text("ab12", 1_794_658_984, |n| env.get(n).map(String::as_str)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["version"], 1);
        assert_eq!(doc["id"], "ab12");
        assert_eq!(doc["expires"], 1_794_658_984u64 + 600);
        let passed = doc["env"].as_object().unwrap();
        assert_eq!(passed[ENV], ARMED_ROOM);
        assert_eq!(passed["SWOFTY_NET_TRACE"], "1");
        assert_eq!(passed["SWOFTY_NET_TRACE_CAP_KB"], "16384");
        assert_eq!(
            passed["SWOFTY_GAME_LOG_CHANNELS"],
            "default,NETWORK*:info,SSL*:info"
        );
        assert_eq!(passed.len(), 4, "{passed:?}");
    }

    #[test]
    fn a_game_that_is_not_armed_gets_no_file() {
        let mut env = armed();
        env.remove(ENV);
        assert_eq!(text("ab12", 1, |n| env.get(n).map(String::as_str)), None);
        env.insert(ENV.into(), String::new());
        assert_eq!(text("ab12", 1, |n| env.get(n).map(String::as_str)), None);

        let data = root("unarmed");
        let stale = data.join(FILE);
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "{}").unwrap();
        assert_eq!(
            write(&data, "ab12", 1, |n| env.get(n).map(String::as_str)).unwrap(),
            None
        );
        assert!(
            !stale.exists(),
            "an earlier launch's file is removed by a launch that is not armed"
        );
        assert!(!data.join(FILES_DIR).exists());
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn writing_replaces_an_earlier_file_and_makes_the_mailbox() {
        let data = root("write");
        let env = armed();
        let get = |n: &str| env.get(n).map(String::as_str);
        let first = write(&data, "one", 100, get).unwrap().unwrap();
        assert_eq!(first, data.join(FILE));
        let second = write(&data, "two", 200, get).unwrap().unwrap();
        let doc: Value = serde_json::from_slice(&std::fs::read(&second).unwrap()).unwrap();
        assert_eq!(doc["id"], "two");
        assert_eq!(doc["expires"], 800);
        assert!(!data.join("state/session_options.json.tmp").exists());
        let mailbox = super::super::game::mailbox(&files_dir_for(&data, Path::new("/game"), true));
        assert_eq!(mailbox, data.join("session/swofty/netplay"));
        assert!(mailbox.join("inbox").is_dir() && mailbox.join("outbox").is_dir());
        clear(&data);
        assert!(!second.exists());
        clear(&data);
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn an_ea_app_copy_trades_its_files_in_the_data_folder_and_a_steam_copy_in_the_game_folder() {
        let data = root("copies");
        let (ea, steam, other) = (data.join("ea"), data.join("steam"), data.join("other"));
        std::fs::create_dir_all(ea.join("__Installer")).unwrap();
        std::fs::write(ea.join(crate::ea_app::MARKER), "").unwrap();
        std::fs::create_dir_all(&steam).unwrap();
        std::fs::write(steam.join("EAJavelinInstaller_installscript.vdf"), "").unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let store = data.join("data");

        assert!(through_file(&ea));
        assert_eq!(files_dir(&store, &ea), store.join("session"));
        assert!(!through_file(&steam));
        assert_eq!(files_dir(&store, &steam), steam);
        assert!(!through_file(&other));
        assert_eq!(files_dir(&store, &other), other);

        assert_eq!(
            super::super::game::mailbox(&files_dir(&store, &steam)),
            steam.join("swofty").join("netplay")
        );
        assert_eq!(
            files_dir(&store, &ea).join(super::super::game::ENTITLEMENTS_FILE),
            store.join("session/swofty/entitlements.json")
        );
        assert_eq!(
            files_dir(&store, &ea).join(super::super::bridge::STATUS_FILE),
            store.join("session/swofty/netplay.json")
        );
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn commands_reach_a_mailbox_in_the_data_folder_when_the_game_folder_refuses_them() {
        use super::super::game::{Command, post};
        let data = root("mailbox");
        let game = data.join("game");
        std::fs::create_dir_all(&game).unwrap();
        // A file where the folder would be: the game folder can't hold the mailbox, as under Program Files.
        std::fs::write(game.join("swofty"), "").unwrap();
        assert!(post(&files_dir_for(&data, &game, false), &Command::Status).is_err());
        let seq = post(&files_dir_for(&data, &game, true), &Command::Status).unwrap();
        assert!(
            data.join(format!("session/swofty/netplay/inbox/{seq}.json"))
                .is_file()
        );
        std::fs::remove_dir_all(&data).ok();
    }
}
