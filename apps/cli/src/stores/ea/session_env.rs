//! The session's environment for a game the EA app starts. The EA app starts F1 25 with an environment of its own, so
//! the session's variables go in a file in the game folder that the loader reads at attach, before any addon loads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use swofty_core::online::multiplayer::client::LAUNCH_ENV;

use super::{HANDOFF_TIMEOUT, PRESS_PLAY_TIMEOUT};
use crate::console::output::say;

/// The file, under the game folder. native/src/loader/version.c reads it from the same place.
pub const FILE: &str = "swofty/session_env.json";
/// The layout the loader reads (native/include/session_env.h, SWOFTY_SESSION_ENV_SCHEMA).
pub const SCHEMA: u32 = 1;
/// The time after the hand-off and the wait for Play that a game still takes to reach the loader.
const START_GRACE: Duration = Duration::from_secs(120);

/// How long after it is written a game may start and still take the file: the wait for the EA app to start the
/// game, the wait for Play in the EA app, and the game's own start.
pub fn life() -> Duration {
    HANDOFF_TIMEOUT + PRESS_PLAY_TIMEOUT + START_GRACE
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contents {
    pub schema: u32,
    /// The session's launch request id.
    pub request: String,
    /// Unix seconds. The loader takes the file only for a game process created between `written` and `expires`.
    pub written: u64,
    pub expires: u64,
    pub env: BTreeMap<String, String>,
}

impl Contents {
    pub fn new(
        request: &str,
        written: u64,
        game_env: &BTreeMap<String, String>,
        overrides: &BTreeMap<String, String>,
    ) -> Self {
        Self {
            schema: SCHEMA,
            request: request.to_owned(),
            written,
            expires: written + life().as_secs(),
            env: passed(game_env, overrides),
        }
    }
}

/// The variables of the game's environment that go in the file: the launcher's own `SWOFTY_` variables and the
/// player's `launch.env`, less the ones [`left_out`] names. The rest of the map is this process's environment, and
/// the EA app gives the game its own.
pub fn passed(
    game_env: &BTreeMap<String, String>,
    overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    game_env
        .iter()
        .filter(|(k, _)| k.starts_with("SWOFTY_") || overrides.contains_key(*k))
        .filter(|(k, _)| !left_out(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Variables a game the EA app starts gets no use from: Proton's and Wine's, read before the game's first
/// instruction; the host's library path and Steam's ids; the Windows compatibility layer, applied when a process is
/// created; and the ones this process reads from its own environment (the data folder, the host's row ids, and the
/// launch variables, whose pairs are in the map already).
pub fn left_out(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "SteamAppId",
        "SteamGameId",
        "SteamOverlayGameId",
        LAUNCH_ENV,
        swofty_core::data::paths::ENV,
        swofty_core::row_ids::ENV,
    ];
    const PREFIXES: &[&str] = &["PROTON_", "WINE", "LD_", "STEAM_"];
    EXACT.contains(&name)
        || PREFIXES.iter().any(|p| name.starts_with(p))
        || swofty_core::session::is_compat_layer(name)
}

/// The file for one game start. Dropping it removes the file, however the session ends.
pub struct SessionEnv {
    path: PathBuf,
}

impl SessionEnv {
    /// Writes `contents` into the game folder. A file that can't be written is reported, and the game starts
    /// without the session's variables.
    pub fn write(game: &Path, contents: &Contents) -> Option<Self> {
        let path = game.join(FILE);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(anyhow::Error::from)
            .and_then(|()| swofty_core::write_json(&path, contents).map_err(anyhow::Error::from));
        match written {
            Ok(()) => {
                say(
                    "ea app",
                    &format!(
                        "the session's {} variable(s) go to the game through {FILE}; the loader sets them at attach \
                         and removes the file",
                        contents.env.len()
                    ),
                );
                Some(Self { path })
            }
            Err(e) => {
                say(
                    "ea app",
                    &format!(
                        "could not write {}: {e:#}; F1 25 starts without the session's variables",
                        path.display()
                    ),
                );
                None
            }
        }
    }
}

impl Drop for SessionEnv {
    fn drop(&mut self) {
        remove(&self.path);
    }
}

/// Removes a file an earlier session left, such as one whose wrapper was killed, before a game starts.
pub fn remove_leftover(game: &Path) {
    let path = game.join(FILE);
    if path.exists() && remove(&path) {
        say(
            "info",
            &format!("removed {FILE}, which an earlier session left in the game folder"),
        );
    }
}

fn remove(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            say(
                "info",
                &format!(
                    "could not remove {}: {e}; the loader ignores it once it expires",
                    path.display()
                ),
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn game(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-session-env-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("swofty")).unwrap();
        dir
    }

    fn game_env() -> BTreeMap<String, String> {
        let mut env = map(&[
            ("HOME", "/home/player"),
            ("PATH", "/usr/bin"),
            ("DISPLAY", ":0"),
            ("PROTON_LOG", "1"),
            ("PROTON_LOG_DIR", "/tmp/session"),
            ("WINEDEBUG", "-all"),
            ("WINEDLLOVERRIDES", "version=n,b"),
            ("LD_LIBRARY_PATH", "/steam/runtime"),
            ("SteamAppId", "3059520"),
            ("SteamGameId", "3059520"),
            ("STEAM_COMPAT_CLIENT_INSTALL_PATH", "/steam"),
            ("__COMPAT_LAYER", "RunAsInvoker"),
            ("SWOFTY_DIRECT_START", "session"),
            ("SWOFTY_NET_TRACE", "1"),
            ("SWOFTY_GAME_LOG_CHANNELS", "default,NETWORK*:info"),
            ("SWOFTY_DATA_DIR", "/home/player/.local/share/swofty"),
            ("DXVK_HUD", "fps"),
        ]);
        env.insert(LAUNCH_ENV.to_owned(), "[[\"SWOFTY_NET_TRACE\",\"1\"]]".to_owned());
        env.insert(
            swofty_core::row_ids::ENV.to_owned(),
            "/home/player/ids.json".to_owned(),
        );
        env
    }

    #[test]
    fn passes_the_launchers_variables_and_the_players_launch_env() {
        let overrides = map(&[
            ("DXVK_HUD", "fps"),
            ("WINEDEBUG", "-all"),
            ("SteamAppId", "3059520"),
        ]);
        let env = passed(&game_env(), &overrides);
        assert_eq!(
            env,
            map(&[
                ("DXVK_HUD", "fps"),
                ("SWOFTY_GAME_LOG_CHANNELS", "default,NETWORK*:info"),
                ("SWOFTY_DIRECT_START", "session"),
                ("SWOFTY_NET_TRACE", "1"),
            ])
        );
    }

    #[test]
    fn leaves_out_what_only_proton_wine_or_the_host_reads() {
        for name in [
            "PROTON_LOG",
            "PROTON_LOG_DIR",
            "WINEDEBUG",
            "WINEDLLOVERRIDES",
            "WINEPREFIX",
            "WINEESYNC",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "SteamAppId",
            "SteamGameId",
            "SteamOverlayGameId",
            "STEAM_COMPAT_DATA_PATH",
            "__COMPAT_LAYER",
            "__compat_layer",
            LAUNCH_ENV,
            "SWOFTY_DATA_DIR",
            swofty_core::row_ids::ENV,
        ] {
            assert!(left_out(name), "{name} is left out");
        }
        for name in [
            "SWOFTY_NET_TRACE",
            "SWOFTY_DIRECT_START",
            "SWOFTY_GAME_LOG_CHANNELS",
            "DXVK_HUD",
        ] {
            assert!(!left_out(name), "{name} is passed");
        }
    }

    #[test]
    fn the_file_names_the_request_and_a_window_past_both_waits() {
        let c = Contents::new("1f2e3d", 1_800_000_000, &game_env(), &BTreeMap::new());
        assert_eq!(c.schema, 1);
        assert_eq!(c.request, "1f2e3d");
        assert_eq!(c.written, 1_800_000_000);
        assert!(c.expires - c.written >= (HANDOFF_TIMEOUT + PRESS_PLAY_TIMEOUT).as_secs());
        assert!(
            c.expires - c.written < 3600,
            "the loader refuses a window of an hour or more"
        );
        let dir = game("contents");
        let file = SessionEnv::write(&dir, &c).unwrap();
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        let read: Contents = serde_json::from_str(&text).unwrap();
        assert_eq!(read, c);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["env"]["SWOFTY_DIRECT_START"], "session");
        assert!(value["env"].get("PROTON_LOG").is_none() && value["env"].get("HOME").is_none());
        drop(file);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_file_goes_when_the_session_ends() {
        let dir = game("drop");
        let c = Contents::new("a1", 1_800_000_000, &game_env(), &BTreeMap::new());
        {
            let _file = SessionEnv::write(&dir, &c).unwrap();
            assert!(dir.join(FILE).is_file());
        }
        assert!(!dir.join(FILE).exists());
        let failed = || -> anyhow::Result<()> {
            let _file = SessionEnv::write(&dir, &c);
            anyhow::bail!("the hand-off failed")
        };
        assert!(failed().is_err());
        assert!(!dir.join(FILE).exists(), "an error on the way out removes it too");
        let _ = std::panic::catch_unwind(|| {
            let _file = SessionEnv::write(&dir, &c);
            panic!("the wait panicked");
        });
        assert!(!dir.join(FILE).exists(), "and so does a panic");
        {
            let _file = SessionEnv::write(&dir, &c).unwrap();
            std::fs::remove_file(dir.join(FILE)).unwrap();
        }
        assert!(
            !dir.join(FILE).exists(),
            "a file the loader removed already is fine"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_new_session_removes_an_earlier_sessions_file() {
        let dir = game("leftover");
        let old = Contents::new("old", 1_700_000_000, &game_env(), &BTreeMap::new());
        swofty_core::write_json(&dir.join(FILE), &old).unwrap();
        remove_leftover(&dir);
        assert!(!dir.join(FILE).exists());
        remove_leftover(&dir);
        let new = Contents::new("new", 1_800_000_000, &game_env(), &BTreeMap::new());
        let file = SessionEnv::write(&dir, &new).unwrap();
        let read: Contents = serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap();
        assert_eq!(read.request, "new");
        assert!(
            read.written > old.expires,
            "the earlier window ended before this one starts"
        );
        drop(file);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_a_folder_the_game_has_not_got_yet() {
        let dir = game("no-folder");
        std::fs::remove_dir_all(dir.join("swofty")).unwrap();
        let c = Contents::new("b2", 1_800_000_000, &game_env(), &BTreeMap::new());
        let file = SessionEnv::write(&dir, &c).unwrap();
        assert!(dir.join(FILE).is_file());
        drop(file);
        std::fs::remove_dir_all(&dir).ok();
    }
}
