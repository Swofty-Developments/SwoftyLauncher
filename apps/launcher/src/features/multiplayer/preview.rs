//! The room page drawn from a saved `session.state` answer, to look at the page without a room, and a stand-in
//! where the page's ad goes while ads are off. Debug builds only, like the local module overrides: a release
//! build never reads either variable.

use std::ffi::OsStr;

use serde_json::Value;
use swofty_core::online::multiplayer::client::state_of;
use swofty_core::online::multiplayer::types::{RoomSummary, SessionState};

/// Names a saved `session.state` answer. A `steward_url` beside its state stands for League Stewarding's page.
pub(super) const ROOM_ENV: &str = "SWOFTY_ROOM_PREVIEW";
/// Names a saved room list, as the room server lists rooms, for the room browser.
pub(super) const ROOMS_ENV: &str = "SWOFTY_ROOMS_PREVIEW";
/// Set, the page draws a stand-in where its ad goes while ads are off.
pub(super) const AD_ENV: &str = "SWOFTY_ROOM_PREVIEW_AD";

/// What a previewed room shows beyond its state.
pub(crate) struct RoomPreview {
    pub(super) steward: Option<String>,
}

/// The room the answer in `file` holds, in a `debug` build.
pub(super) fn room(debug: bool, file: Option<&OsStr>) -> Option<(SessionState, RoomPreview)> {
    if !debug {
        return None;
    }
    let text = std::fs::read_to_string(file?).ok()?;
    let doc: Value = serde_json::from_str(&text).ok()?;
    let steward = doc.get("steward_url").and_then(Value::as_str).map(str::to_owned);
    Some((state_of(doc)?, RoomPreview { steward }))
}

/// The rooms the list in `file` holds, in a `debug` build.
pub(super) fn rooms(debug: bool, file: Option<&OsStr>) -> Option<Vec<RoomSummary>> {
    if !debug {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(file?).ok()?).ok()
}

/// Whether the page draws the ad stand-in: in a `debug` build with [`AD_ENV`] `set`.
pub(super) fn ad_stand_in(debug: bool, set: Option<&OsStr>) -> bool {
    debug && set.is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use swofty_core::online::multiplayer::types::{Phase, StepState};

    use super::{ad_stand_in, room, rooms};

    const ANSWER: &str = r#"{
        "state": {"phase": {"phase": "in_lobby"}, "host": true, "members": [], "mismatches": [],
                  "lobby_connected": true, "content": {"host": null, "owns_f1_2026": {}}},
        "steps": [{"id": "account", "state": "done"}, {"id": "build", "state": "active", "detail": "addons"}],
        "steward_url": "http://127.0.0.1:51234/?t=ab"
    }"#;

    #[test]
    fn only_a_debug_build_previews_a_room() {
        let dir = std::env::temp_dir().join(format!("swofty-room-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("room.json");
        std::fs::write(&file, ANSWER).unwrap();

        assert!(room(false, Some(file.as_os_str())).is_none(), "a release build");
        let (state, preview) = room(true, Some(file.as_os_str())).expect("a debug build");
        assert_eq!(state.phase, Phase::InLobby);
        assert!(state.host);
        assert_eq!(state.steps.len(), 2);
        assert_eq!(state.steps[1].state, StepState::Active);
        assert_eq!(preview.steward.as_deref(), Some("http://127.0.0.1:51234/?t=ab"));
        assert!(room(true, None).is_none());
        assert!(room(true, Some(dir.join("missing.json").as_os_str())).is_none());
        std::fs::write(&file, "{").unwrap();
        assert!(room(true, Some(file.as_os_str())).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_a_debug_build_previews_the_room_list() {
        let dir = std::env::temp_dir().join(format!("swofty-rooms-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rooms.json");
        std::fs::write(
            &file,
            r#"[{"code":"ABC123","name":"Sunday league","host":{"id":"u1","name":"swofty"},
                "members":3,"max_members":20,"category":"f1_2025"}]"#,
        )
        .unwrap();
        assert!(rooms(false, Some(file.as_os_str())).is_none(), "a release build");
        let list = rooms(true, Some(file.as_os_str())).expect("a debug build");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].max_members, 20);
        assert!(rooms(true, None).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_a_debug_build_draws_the_ad_stand_in() {
        assert!(ad_stand_in(true, Some(OsStr::new("1"))));
        assert!(!ad_stand_in(false, Some(OsStr::new("1"))), "a release build");
        assert!(!ad_stand_in(true, None));
        assert!(!ad_stand_in(true, Some(OsStr::new(""))));
    }
}
