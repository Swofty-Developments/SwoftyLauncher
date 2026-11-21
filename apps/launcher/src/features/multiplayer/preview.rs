//! The room page drawn from a saved `session.state` answer, to look at the page without a room. Debug builds
//! only, like the local module overrides: a release build never reads either variable.

use std::cmp::Ordering;
use std::ffi::OsStr;

use serde_json::{Value, json};
use swofty_core::online::multiplayer::client::state_of;
use swofty_core::online::multiplayer::types::{RoomSummary, SessionState};

use super::tracker::Here;

/// Names a saved `session.state` answer, or one of [`FIXTURES`] by name. A `steward_url` beside its state stands
/// for League Stewarding's page, and a `launcher` object for what the launcher itself knows ([`Here`]).
pub(super) const ROOM_ENV: &str = "SWOFTY_ROOM_PREVIEW";
/// Names a saved room list, as the room server lists rooms, for the room browser.
pub(super) const ROOMS_ENV: &str = "SWOFTY_ROOMS_PREVIEW";

/// What a previewed room shows beyond its state.
pub(crate) struct RoomPreview {
    pub(super) steward: Option<String>,
    pub(super) here: Option<Here>,
}

/// The room the answer in `file` holds, or the fixture it names, in a `debug` build.
pub(super) fn room(debug: bool, file: Option<&OsStr>) -> Option<(SessionState, RoomPreview)> {
    if !debug {
        return None;
    }
    let file = file?;
    let doc = match file.to_str().and_then(fixture) {
        Some(doc) => doc,
        None => {
            let Some(doc) = std::fs::read_to_string(file)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
            else {
                eprintln!(
                    "swofty-launcher: {ROOM_ENV} names no answer file or fixture; the fixtures are {}",
                    FIXTURES.join(", ")
                );
                return None;
            };
            doc
        }
    };
    let steward = doc.get("steward_url").and_then(Value::as_str).map(str::to_owned);
    let here = doc.get("launcher").map(here_of);
    Some((state_of(doc)?, RoomPreview { steward, here }))
}

fn here_of(v: &Value) -> Here {
    let flag = |key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    Here {
        leaving: flag("leaving"),
        running: flag("running"),
        closing: flag("closing"),
        building: flag("building"),
        launching: flag("launching"),
        held: flag("held"),
        taken: v
            .get("taken")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
    }
}

/// The room states a debug build can show by name, as `SWOFTY_ROOM_PREVIEW=<name>`.
pub(super) const FIXTURES: &[&str] = &[
    "launching",
    "restart-needed",
    "restarting",
    "lobby",
    "lobby-host",
    "racing",
    "addons-fixable",
    "addons-installing",
    "game-crashed",
    "host-closed",
    "relay-lost",
    "finished",
];

const IDS: [&str; 8] = [
    "account", "room", "addons", "build", "game", "network", "lobby", "race",
];

fn at(id: &str) -> usize {
    IDS.iter().position(|i| *i == id).unwrap_or(0)
}

/// Every step before `id` done, `id` in `state` with `detail`, the steps after it pending.
fn steps(id: &str, state: &str, detail: Option<&str>) -> Value {
    let n = at(id);
    Value::Array(
        IDS.iter()
            .enumerate()
            .map(|(i, step)| {
                let state = match i.cmp(&n) {
                    Ordering::Less => "done",
                    Ordering::Equal => state,
                    Ordering::Greater => "pending",
                };
                let mut v = json!({"id": step, "state": state});
                if let (Ordering::Equal, Some(detail)) = (i.cmp(&n), detail) {
                    v["detail"] = json!(detail);
                }
                v
            })
            .collect(),
    )
}

fn member(id: &str, name: &str, number: u8, host: bool, me: bool, stage: &str) -> Value {
    json!({
        "id": id, "name": name, "avatar": null, "number": number, "ip": format!("10.77.0.{number}"),
        "host": host, "me": me, "connected": true, "fingerprint": "matches", "relay": "ready",
        "stage": stage, "traffic": null,
    })
}

fn answer(phase: &str, reason: Option<&str>, host: bool, steps: Value, stage: &str) -> Value {
    let phase = match reason {
        Some(r) => json!({"phase": phase, "reason": r}),
        None => json!({"phase": phase}),
    };
    json!({
        "state": {
            "phase": phase, "host": host,
            "room": {"code": "F1-7KQ2MX", "name": "Sunday league", "public": true, "category": "f1_2025",
                     "max_members": 20, "started": false, "host": "u1"},
            "self_member": (if host { 1 } else { 2 }),
            "members": [
                member("u1", if host { "You" } else { "Ana" }, 1, true, host, stage),
                member("u2", if host { "Max" } else { "You" }, 2, false, !host, stage),
            ],
            "mismatches": [], "lobby_connected": true, "relay": null, "base": null, "bridge": null,
            "game": null, "last_error": null, "launch": null,
            "content": {"host": null, "owns_f1_2026": {}},
        },
        "progress": null, "problem": null, "fix_plan": [], "finished": false,
        "steps": steps,
    })
}

fn launch(seq: u32, restart: bool) -> Value {
    json!({"seq": seq, "row_ids": null, "id_addons": 0, "setting_addons": 0, "restart": restart,
           "why": "for the room", "direct_start": ""})
}

fn problem(text: &str, retry: bool) -> Value {
    json!({"text": text, "retry": retry})
}

/// The answer a fixture names. Each one is a state the module sends, with what the launcher knows beside it.
pub(super) fn fixture(name: &str) -> Option<Value> {
    let mut doc = match name {
        "launching" => {
            let mut d = answer(
                "waiting_for_game",
                Some("F1 25 is starting"),
                false,
                steps("game", "active", Some("F1 25 is starting")),
                "starting",
            );
            d["state"]["launch"] = launch(1, false);
            d["launcher"] = json!({"running": true, "launching": true, "taken": 1});
            d
        }
        "restart-needed" | "restarting" => {
            let mut d = answer(
                "waiting_for_game",
                Some("restart F1 25 for the room"),
                false,
                steps("build", "active", Some("restart F1 25 for the room")),
                "loading",
            );
            d["state"]["launch"] = launch(2, true);
            d["problem"] = problem("F1 25 has to restart for this session.", true);
            d["launcher"] = json!({"running": true, "closing": (name == "restarting"), "taken": 1});
            d
        }
        "lobby" | "lobby-host" => answer(
            "in_lobby",
            None,
            name == "lobby-host",
            steps("race", "active", Some("waiting for the race to start")),
            "in_lobby",
        ),
        "racing" => {
            let mut d = answer("racing", None, true, steps("race", "done", None), "in_lobby");
            d["state"]["room"]["started"] = json!(true);
            d
        }
        "addons-fixable" => {
            let mut d = answer(
                "waiting_for_game",
                Some("starting F1 25 for the room"),
                false,
                steps("build", "active", Some("starting F1 25 for the room")),
                "starting",
            );
            d["steps"][at("addons")] = json!({
                "id": "addons", "state": "done", "warning": true,
                "detail": "Timing HUD (t.hud) is 1.0.0; the host has 1.2.0",
                "problem": {"code": "addons_differ", "text": "Timing HUD doesn't match the host's.",
                            "retry": false, "detail": "Timing HUD (t.hud) is 1.0.0; the host has 1.2.0.",
                            "fix_plan": [{"action": "install", "id": "t.hud", "name": "Timing HUD",
                                          "version": "1.2.0", "sha256": null}]},
            });
            d["problem"] = problem("Timing HUD doesn't match the host's.", false);
            d["state"]["launch"] = launch(1, false);
            d["launcher"] = json!({"building": true, "launching": true, "taken": 1});
            d
        }
        "addons-installing" => answer(
            "waiting_for_game",
            Some("waiting for the host's addon list"),
            false,
            steps(
                "addons",
                "active",
                Some("Installing 2026 Cars: 1 of 3 addons, 120 of 450 MB"),
            ),
            "starting",
        ),
        "game-crashed" => {
            let mut d = answer(
                "waiting_for_game",
                Some("F1 25 crashed"),
                false,
                steps(
                    "game",
                    "failed",
                    Some("F1 25 crashed while it loaded the room's lobby"),
                ),
                "loading",
            );
            d["steps"][at("game")]["problem"] =
                json!({"code": "game_crashed", "text": "F1 25 crashed.", "retry": true});
            d["problem"] = problem("F1 25 crashed.", true);
            d
        }
        "host-closed" => {
            let mut d = answer(
                "ended",
                Some("the host closed the room"),
                false,
                steps("room", "failed", Some("the host closed the room")),
                "in_lobby",
            );
            d["steps"][at("room")]["problem"] =
                json!({"code": "room_closed", "text": "The host closed the room.", "retry": false});
            d["finished"] = json!(true);
            d
        }
        "relay-lost" => {
            let mut d = answer(
                "connecting",
                Some("connecting to the other players"),
                false,
                steps("network", "failed", Some("the relay stopped answering")),
                "in_lobby",
            );
            d["steps"][at("network")]["problem"] = json!({"code": "relay_lost",
                "text": "The connection to the relay dropped.", "retry": true});
            d["problem"] = problem("The connection to the relay dropped.", true);
            d["launcher"] = json!({"running": true, "taken": 1});
            d
        }
        "finished" => {
            let mut d = answer(
                "ended",
                Some("the host ended the session"),
                false,
                steps("race", "done", None),
                "in_lobby",
            );
            d["finished"] = json!(true);
            d
        }
        _ => return None,
    };
    if doc.get("launcher").is_none() {
        doc["launcher"] = json!({});
    }
    Some(doc)
}

/// The rooms the list in `file` holds, in a `debug` build.
pub(super) fn rooms(debug: bool, file: Option<&OsStr>) -> Option<Vec<RoomSummary>> {
    if !debug {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(file?).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use swofty_core::online::multiplayer::types::{Phase, StepState};

    use super::{room, rooms};

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
}
