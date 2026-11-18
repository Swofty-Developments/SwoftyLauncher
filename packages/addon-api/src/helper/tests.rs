use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;

use super::*;
use crate::asset::{AssetOp, Features, FileOp, Kind, Source};
use crate::launcher::{ProfileInfo, TaskItem, TaskList, TaskState};
use crate::view::Event;

fn profile() -> ProfileInfo {
    ProfileInfo {
        name: "default".into(),
        vanilla: false,
        enabled: true,
    }
}

fn round<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(value: T, wire: Value) {
    assert_eq!(serde_json::to_value(&value).unwrap(), wire);
    let line = serde_json::to_string(&value).unwrap();
    assert!(!line.contains('\n'));
    assert_eq!(serde_json::from_str::<T>(&line).unwrap(), value);
}

#[test]
fn requests_round_trip() {
    round(
        Request {
            id: Some(0),
            call: Call::Hello(Hello {
                api: API_VERSION,
                addon: "example.addon".into(),
                data_dir: "/d".into(),
                cache_dir: "/c".into(),
                profile: profile(),
                launcher: Some("1.0.10".into()),
                host_calls: Vec::new(),
                capabilities: Vec::new(),
            }),
        },
        json!({"id": 0, "method": "hello", "params": {"api": 1, "addon": "example.addon",
                "data_dir": "/d", "cache_dir": "/c",
                "profile": {"name": "default", "vanilla": false, "enabled": true},
                "launcher": "1.0.10"}}),
    );
    let hello: Hello = serde_json::from_value(json!({"api": 1, "addon": "a.b", "data_dir": "/d",
            "cache_dir": "/c", "profile": {"name": "default", "vanilla": false, "enabled": true},
            "host_calls": ADDON_CALLS}))
    .unwrap();
    assert_eq!(hello.host_calls, ADDON_CALLS);
    assert!(hello.capabilities.is_empty());
    let mut current = hello;
    current.capabilities = vec![VIEW_CHART.into(), "future.feature".into()];
    let json = serde_json::to_value(&current).unwrap();
    assert_eq!(json["capabilities"], json!(["view.chart", "future.feature"]));
    assert_eq!(serde_json::from_value::<Hello>(json).unwrap(), current);
    round(
        Request {
            id: None,
            call: Call::Profile(profile()),
        },
        json!({"method": "profile", "params": {"name": "default", "vanilla": false, "enabled": true}}),
    );
    round(
        Request {
            id: Some(2),
            call: Call::Event {
                section: "items".into(),
                event: Event::Toggle {
                    id: "a".into(),
                    on: true,
                },
            },
        },
        json!({"id": 2, "method": "event", "params": {"section": "items",
                "event": {"toggle": {"id": "a", "on": true}}}}),
    );
    round(
        Request {
            id: Some(3),
            call: Call::Build {
                options: BTreeMap::from([("size".into(), json!(24))]),
                features: Features {
                    stable: [Kind::File].into(),
                    experimental: Default::default(),
                },
            },
        },
        json!({"id": 3, "method": "build", "params": {"options": {"size": 24},
                "features": {"stable": ["file"], "experimental": []}}}),
    );
    round(
        Request {
            id: Some(4),
            call: Call::Summary,
        },
        json!({"id": 4, "method": "summary"}),
    );
}

#[test]
fn helper_messages_round_trip() {
    round(
        Incoming::Reply(Reply::Ok {
            id: 1,
            result: Value::Null,
        }),
        json!({"id": 1, "result": null}),
    );
    round(
        Incoming::Reply(Reply::Err {
            id: 1,
            error: "no".into(),
        }),
        json!({"id": 1, "error": "no"}),
    );
    round(
        Incoming::Call(Request {
            id: None,
            call: HostCall::Progress {
                done: 0.5,
                text: "half".into(),
            },
        }),
        json!({"method": "progress", "params": {"done": 0.5, "text": "half"}}),
    );
    round(
        Incoming::Call(Request {
            id: None,
            call: HostCall::Tasks(TaskList {
                title: "Migrating your mods to Swofty addons".into(),
                subtitle: "2 mods".into(),
                items: vec![
                    TaskItem {
                        id: "okayama".into(),
                        label: "Okayama".into(),
                        state: TaskState::Done,
                        note: None,
                    },
                    TaskItem {
                        id: "liveries".into(),
                        label: "Liveries".into(),
                        state: TaskState::Failed,
                        note: Some("no archive inside".into()),
                    },
                ],
            }),
        }),
        json!({"method": "tasks", "params": {"title": "Migrating your mods to Swofty addons",
        "subtitle": "2 mods", "items": [
            {"id": "okayama", "label": "Okayama", "state": "done"},
            {"id": "liveries", "label": "Liveries", "state": "failed", "note": "no archive inside"}
        ]}}),
    );
    let bare: Incoming = serde_json::from_value(json!({"method": "tasks", "params": {
            "title": "T", "items": [{"id": "a", "label": "A", "state": "skipped"}]}}))
    .unwrap();
    let Incoming::Call(Request {
        call: HostCall::Tasks(bare),
        ..
    }) = bare
    else {
        panic!("not a task list");
    };
    assert_eq!(
        (bare.subtitle.as_str(), bare.items[0].state),
        ("", TaskState::Skipped)
    );
    for state in ["waiting", "running", "done", "failed", "skipped"] {
        assert!(serde_json::from_value::<TaskState>(json!(state)).is_ok());
    }
    assert!(serde_json::from_value::<TaskState>(json!("paused")).is_err());
    round(
        Incoming::Call(Request {
            id: Some(7),
            call: HostCall::GetState,
        }),
        json!({"id": 7, "method": "get_state"}),
    );
    round(
        Incoming::Call(Request {
            id: Some(8),
            call: HostCall::SetState(json!({"on": ["a"]})),
        }),
        json!({"id": 8, "method": "set_state", "params": {"on": ["a"]}}),
    );
    round(
        Incoming::Call(Request {
            id: Some(9),
            call: HostCall::GameBuildKey,
        }),
        json!({"id": 9, "method": "game_build_key"}),
    );
    round(
        Incoming::Call(Request {
            id: Some(10),
            call: HostCall::GameOriginal {
                rel: "ui/a.erp".into(),
            },
        }),
        json!({"id": 10, "method": "game_original", "params": {"rel": "ui/a.erp"}}),
    );
    let built: Built = serde_json::from_value(json!({"ops": [
        {"op": "file.put", "source": {"root": "data", "path": "a.dds"}, "target": "b.dds"}
    ]}))
    .unwrap();
    assert!(matches!(
        &built.ops[0],
        AssetOp::File(FileOp::Put { source: Source { path, .. }, .. }) if path == "a.dds"
    ));
    assert!(built.items.is_empty());
    let built: Built = serde_json::from_value(json!({"ops": [
            {"op": "file.put", "source": {"root": "data", "path": "a.dds"}, "target": "b.dds"},
            {"op": "file.put", "source": {"root": "data", "path": "c.dds"}, "target": "d.dds"}
        ], "items": ["mod.a", null]}))
    .unwrap();
    assert_eq!(built.items, [Some("mod.a".to_owned()), None]);
    assert_eq!(
        serde_json::to_value(&built).unwrap()["items"],
        json!(["mod.a", null])
    );
    assert!(
        serde_json::to_value(Built::default())
            .unwrap()
            .get("items")
            .is_none()
    );
    assert!(built.fields.is_empty());
    let built: Built = serde_json::from_value(json!({"ops": [
            {"op": "db.add-row", "table": "t", "from": "t.r_0"}
        ], "fields": {"t.r": ["a", "b"]}}))
    .unwrap();
    assert_eq!(built.fields["t.r"], ["a", "b"]);
    assert_eq!(
        built.reads, None,
        "a reply without reads is answered on every build"
    );
    let built: Built =
        serde_json::from_value(json!({"ops": [], "reads": ["/s/addons/a/db", "/s/gone"]})).unwrap();
    assert_eq!(
        built.reads,
        Some(vec![PathBuf::from("/s/addons/a/db"), PathBuf::from("/s/gone")])
    );
    let empty: Built = serde_json::from_value(json!({"ops": [], "reads": []})).unwrap();
    assert_eq!(
        empty.reads,
        Some(Vec::new()),
        "an empty list still lets the reply be reused"
    );
    assert!(
        serde_json::to_value(Built::default())
            .unwrap()
            .get("reads")
            .is_none()
    );
    assert!(
        serde_json::to_value(Built::default())
            .unwrap()
            .get("fields")
            .is_none()
    );
    assert_eq!(
        serde_json::from_str::<Incoming>(r#"{"id": 7, "method": "get_state", "params": null}"#).unwrap(),
        Incoming::Call(Request {
            id: Some(7),
            call: HostCall::GetState,
        })
    );
    assert!(serde_json::from_str::<Incoming>(r#"{"result": 1}"#).is_err());
    round(
        Request {
            id: Some(5),
            call: Call::Tick {
                section: "overview".into(),
            },
        },
        json!({"id": 5, "method": "tick", "params": {"section": "overview"}}),
    );
    for (method, call) in [
        ("game_dir", HostCall::GameDir),
        ("addon_game_dir", HostCall::AddonGameDir),
        ("registry_dir", HostCall::RegistryDir),
        ("game_status", HostCall::GameStatus),
        ("steam_save_dir", HostCall::SteamSaveDir),
        (SAVE_DIR, HostCall::SaveDir),
        ("profile_tracks", HostCall::ProfileTracks),
        (PROFILE_HEADS, HostCall::ProfileHeads),
        ("addons", HostCall::Addons),
    ] {
        round(
            Incoming::Call(Request { id: Some(11), call }),
            json!({"id": 11, "method": method}),
        );
    }
    for (call, params) in [
        (
            HostCall::InstallAddon {
                path: "/home/you/okayama.swoftyaddon".into(),
            },
            json!({"path": "/home/you/okayama.swoftyaddon"}),
        ),
        (
            HostCall::SetAddonEnabled {
                id: "you.okayama".into(),
                on: true,
            },
            json!({"id": "you.okayama", "on": true}),
        ),
        (
            HostCall::RemoveAddon {
                id: "you.okayama".into(),
            },
            json!({"id": "you.okayama"}),
        ),
        (
            HostCall::MoveAddon {
                id: "you.okayama".into(),
                delta: -2,
            },
            json!({"id": "you.okayama", "delta": -2}),
        ),
    ] {
        let method = serde_json::to_value(&call).unwrap()["method"].clone();
        let method_name = method.as_str().unwrap();
        assert!(ADDON_CALLS.contains(&method_name) || method_name == MOVE_ADDON);
        round(
            Incoming::Call(Request { id: Some(12), call }),
            json!({"id": 12, "method": method, "params": params}),
        );
    }
    round(
        Incoming::Call(Request {
            id: None,
            call: HostCall::Invalidate(Invalidate {
                section: Some("overview".into()),
            }),
        }),
        json!({"method": "invalidate", "params": {"section": "overview"}}),
    );
    assert_eq!(
        serde_json::from_str::<Incoming>(r#"{"method": "invalidate", "params": {}}"#).unwrap(),
        Incoming::Call(Request {
            id: None,
            call: HostCall::Invalidate(Invalidate::default()),
        })
    );
    let status: crate::launcher::GameStatus = serde_json::from_str(
        r#"{"running": true, "pid": 48211, "since": 1760941234.5, "session": "2026-10-20T14-22-33"}"#,
    )
    .unwrap();
    assert_eq!(status.pid, Some(48211));
    assert_eq!(status.session.as_deref(), Some("2026-10-20T14-22-33"));
    assert!(serde_json::from_str::<Incoming>(r#"{"id": 1, "method": "launch"}"#).is_err());
}
