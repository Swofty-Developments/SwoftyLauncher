//! Time Trial boards: the launcher's side of uploading laps the core records.
//!
//! The core writes one `.swlap` file per completed lap into `<game>/swofty/laps`, or into the data folder's
//! `laps` when it can't write the game folder (an EA app copy under Program Files). [`queue`] moves them into
//! the outbox and uploads them through [`api`]; [`upload`] is the loop the wrapper and the launcher run for it.

pub mod barred;
pub mod boards;
pub mod bridge;
pub mod fingerprint;
pub mod game_replay;
pub mod queue;
pub mod swlap;
pub mod ticks;
pub mod toast;
pub mod upload;
pub mod watch;

use std::path::{Path, PathBuf};

use crate::online::server::api::Nonce;
use crate::session::Config;

pub const MAILBOX_TOAST: &str = "toast";
pub const LAPS_DIR: &str = "swofty/laps";
/// Where the wrapper hands the core its Time Trial boards switches. Not `swofty/session.json`: the core
/// treats that file's existence as a mixin manifest and refuses every mixin it does not declare.
pub const SESSION_FILE: &str = "swofty/tt.json";

/// Where the core writes laps: the game folder, where the integrity seal finds the session's `tt.json`, then the
/// data folder when the game folder refuses them (an EA app copy under Program Files).
pub fn laps_dirs(data: &Path, game: &Path) -> [PathBuf; 2] {
    [data.join("laps"), game.join(LAPS_DIR)]
}

/// The page that shows the boards.
pub fn boards_url() -> String {
    std::env::var("SWOFTY_TT_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_owned())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://tt.f1.swofty.net".to_owned())
}

/// The token of the sign-in when Time Trial boards is on: the setting is on and the player is signed in.
pub fn token(data: &Path, config: &Config) -> Option<String> {
    config
        .time_trial_boards
        .then(|| crate::online::server::sign_in::load(data))
        .flatten()
        .map(|s| s.token)
}

const KEYS: [&str; 5] = ["tt_boards", "tt_nonce", "tt_bridge", "tt_ghost", "tt_challenge"];
/// The largest challenge handed to the integrity module; a bigger one, or one that is not an object, is left out.
const CHALLENGE_MAX: usize = 16 * 1024;

/// Held while [`SESSION_FILE`] is read and written again, so the nonce retries and the barred list refresh,
/// each on a thread of its own, do not drop each other's keys.
pub(crate) static SESSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Tell the core, through [`SESSION_FILE`], that laps are to be recorded for the boards, which
/// session nonce they carry, the server's challenge for the integrity module when the session has one, and
/// whether the in-game bridge is armed. Other keys in the file stay as they are.
pub fn write_session_keys(game: &Path, session: Option<&Nonce>, bridge: bool) -> std::io::Result<()> {
    let _held = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = game.join(SESSION_FILE);
    let mut doc = read_object(&path);
    doc.insert("tt_boards".into(), true.into());
    match session {
        Some(n) => doc.insert("tt_nonce".into(), n.nonce.clone().into()),
        None => doc.remove("tt_nonce"),
    };
    match session.and_then(challenge) {
        Some(c) => doc.insert("tt_challenge".into(), c.clone()),
        None => doc.remove("tt_challenge"),
    };
    if bridge {
        doc.insert("tt_bridge".into(), true.into());
        doc.insert("tt_ghost".into(), true.into());
    } else {
        doc.remove("tt_bridge");
        doc.remove("tt_ghost");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap_or_default())
}

fn challenge(session: &Nonce) -> Option<&serde_json::Value> {
    session
        .challenge
        .as_ref()
        .filter(|c| c.is_object() && c.to_string().len() <= CHALLENGE_MAX)
}

/// Take [`write_session_keys`]'s keys back out; the file goes if nothing else is in it.
pub fn clear_session_keys(game: &Path) {
    let _held = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = game.join(SESSION_FILE);
    let mut doc = read_object(&path);
    if KEYS.iter().all(|k| !doc.contains_key(*k)) {
        return;
    }
    KEYS.iter().for_each(|k| {
        doc.remove(*k);
    });
    if doc.is_empty() {
        std::fs::remove_file(&path).ok();
    } else {
        std::fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap_or_default()).ok();
    }
}

/// Change [`SESSION_FILE`]'s keys with `edit`, under [`SESSION_LOCK`]. The file goes when nothing is left in it,
/// and is not written when `edit` changed nothing.
pub(crate) fn edit_session(
    game: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> std::io::Result<()> {
    let _held = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = game.join(SESSION_FILE);
    let before = read_object(&path);
    let mut doc = before.clone();
    edit(&mut doc);
    if doc == before {
        return Ok(());
    }
    if doc.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap_or_default())
}

fn read_object(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::online::server::api::Api;

    pub(crate) fn nonce(n: &str) -> Nonce {
        Nonce {
            nonce: n.to_owned(),
            expires: 0,
            challenge: None,
        }
    }

    #[test]
    fn session_keys_merge_into_the_file_and_come_back_out() {
        let game = std::env::temp_dir().join(format!("swofty-ttk-{}", std::process::id()));
        std::fs::remove_dir_all(&game).ok();
        write_session_keys(&game, Some(&nonce("abc")), true).unwrap();
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(game.join(SESSION_FILE)).unwrap()).unwrap();
        assert_eq!(
            doc,
            serde_json::json!({"tt_boards": true, "tt_nonce": "abc", "tt_bridge": true, "tt_ghost": true})
        );
        clear_session_keys(&game);
        assert!(!game.join(SESSION_FILE).exists());

        std::fs::write(
            game.join(SESSION_FILE),
            r#"{"game_log":{"level":"warn"},"tt_nonce":"old"}"#,
        )
        .unwrap();
        write_session_keys(&game, None, false).unwrap();
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(game.join(SESSION_FILE)).unwrap()).unwrap();
        assert_eq!(
            doc,
            serde_json::json!({"game_log":{"level":"warn"},"tt_boards": true})
        );
        clear_session_keys(&game);
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(game.join(SESSION_FILE)).unwrap()).unwrap();
        assert_eq!(doc, serde_json::json!({"game_log":{"level":"warn"}}));
        std::fs::remove_dir_all(&game).ok();
    }

    #[test]
    fn the_servers_challenge_goes_to_the_module_beside_the_nonce_and_an_old_server_sends_none() {
        let game = std::env::temp_dir().join(format!("swofty-ttc-{}", std::process::id()));
        std::fs::remove_dir_all(&game).ok();
        let read = || -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(game.join(SESSION_FILE)).unwrap()).unwrap()
        };
        let challenge = serde_json::json!({"v": 1, "salt": "c2FsdA==", "regions": [["exe", 4128768, 65536], ["rdata", 1310720, 16384]]});
        let answer = serde_json::json!({"nonce": "n1", "expires": 99, "challenge": challenge}).to_string();
        let (url, t) = crate::online::server::api::tests::serve("200 OK", Box::leak(answer.into_boxed_str()));
        let n = Api::with_server(&url, "tok").sessions(None).unwrap();
        t.join().unwrap();
        write_session_keys(&game, Some(&n), false).unwrap();
        assert_eq!(
            read(),
            serde_json::json!({"tt_boards": true, "tt_nonce": "n1", "tt_challenge": challenge})
        );

        let (url, t) = crate::online::server::api::tests::serve("200 OK", r#"{"nonce":"n2","expires":99}"#);
        let old = Api::with_server(&url, "tok").sessions(None).unwrap();
        t.join().unwrap();
        write_session_keys(&game, Some(&old), false).unwrap();
        assert_eq!(read(), serde_json::json!({"tt_boards": true, "tt_nonce": "n2"}));

        for odd in [
            serde_json::json!("not an object"),
            serde_json::json!({"salt": "x".repeat(20_000)}),
        ] {
            let n = Nonce {
                challenge: Some(odd),
                ..n.clone()
            };
            write_session_keys(&game, Some(&n), false).unwrap();
            assert_eq!(read(), serde_json::json!({"tt_boards": true, "tt_nonce": "n1"}));
        }

        write_session_keys(&game, Some(&n), false).unwrap();
        write_session_keys(&game, None, false).unwrap();
        assert_eq!(read(), serde_json::json!({"tt_boards": true}));
        write_session_keys(&game, Some(&n), false).unwrap();
        clear_session_keys(&game);
        assert!(!game.join(SESSION_FILE).exists());
        std::fs::remove_dir_all(&game).ok();
    }
}
