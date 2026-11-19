use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use swofty_addon::helper::{API_VERSION, HostCall, PROFILE_HEADS, RESTART, SAVE_DIR};
use swofty_addon::launcher::{Addon, BuildCx, Claim, Cx, Importer, ProfileInfo, Progress};
use swofty_addon::manifest::AddonManifest;
use swofty_addon::view::{Body, Effect, Event};

use super::host_calls::*;
use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;

use swofty_addon::asset::{AssetBatch, AssetOp, Features};
use swofty_addon::launcher::{GameFiles, GameStatus, StateStore};

use crate::addons::import::{Route, route};
use crate::profile::Profile;
use crate::store::Store;

const MANIFEST: &str = r#"{"schema_version":1,"id":"t.helper","name":"Helper","version":"1.0.0","author":"t",
        "compat":{"game":"f1_25"},
        "launcher":{"program":{"linux":"bin/helper","macos":"bin/helper"},
            "sections":[{"id":"items","title":"Items","icon":"package"}],"imports":["exmod"],"build":true}}"#;

#[derive(Default)]
struct Memory(Mutex<BTreeMap<String, Value>>);

impl StateStore for Memory {
    fn get(&self, addon: &str) -> Result<Option<Value>> {
        Ok(self.0.lock().unwrap().get(addon).cloned())
    }

    fn set(&self, addon: &str, value: Value) -> Result<()> {
        self.0.lock().unwrap().insert(addon.into(), value);
        Ok(())
    }
}

#[derive(Default)]
struct Seen(Mutex<Vec<(f32, String)>>);

impl Progress for Seen {
    fn report(&self, done: f32, text: &str) {
        self.0.lock().unwrap().push((done, text.into()));
    }
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-helper-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    dir
}

// The script is written by a child process: a write fd open in this
// process leaks into whatever another test spawns at that moment, and
// exec then fails with "Text file busy" until that child has exec'd.
fn addon(dir: &Path, script: &str) {
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("swofty.json"), MANIFEST).unwrap();
    let mut cat = Command::new("sh")
        .args(["-c", r#"cat > "$0""#])
        .arg(dir.join("bin/helper"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = cat.stdin.take().unwrap();
    write!(stdin, "#!/bin/sh\n{script}").unwrap();
    drop(stdin);
    assert!(cat.wait().unwrap().success());
}

// Long enough that only a hung helper reaches it, however loaded the
// machine running the tests is.
const PATIENT: Duration = Duration::from_secs(60);
const HUNG: Duration = Duration::from_millis(500);

fn helper(dir: &Path) -> HelperAddon {
    helper_waiting(dir, PATIENT)
}

fn helper_waiting(dir: &Path, wait: Duration) -> HelperAddon {
    let manifest = AddonManifest::load(dir).unwrap();
    HelperAddon::new(manifest, dir).unwrap().with_timeouts(Timeouts {
        quick: wait,
        long: wait,
        start: wait,
    })
}

fn profile(name: &str) -> ProfileInfo {
    ProfileInfo {
        name: name.into(),
        vanilla: false,
        enabled: true,
    }
}

const ROUND_TRIP: &str = r#"
answer() { read -r l; echo "$l" >> received; echo "$1"; }
answer '{"id":0,"result":{"api":1}}'
answer '{"id":9,"method":"get_state"}'
answer '{"id":10,"method":"set_state","params":["b"]}'
answer '{"id":1,"result":{"subtitle":"s","body":{"blocks":[]}}}'
answer '{"method":"progress","params":{"done":0.5,"text":"half"}}'
echo '{"id":2,"result":["imported a.exmod"]}'
read -r l; echo "$l" >> received
answer '{"id":3,"result":{"ops":[{"op":"file.put","source":"a.dds","target":"b.dds"},{"op":"file.put","source":"c.dds","target":"d.dds"}],"items":["mod.a"],"lines":["one"],"fields":{"t.r":["a"]}}}'
answer '{"id":4,"error":"nope"}'
read -r l
"#;

#[test]
fn talks_json_lines_and_answers_host_calls() {
    let root = tmp("round-trip");
    let dir = root.join("addon");
    addon(&dir, ROUND_TRIP);
    let h = helper(&dir);
    let state = Memory::default();
    state.set("t.helper", serde_json::json!(["a"])).unwrap();
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let first = profile("default");
    let cx = Cx::new("t.helper", &first, &data, &cache, None, &state);
    assert!(h.summary(&cx).is_none() && !h.running());
    let view = h.view(&cx, "items").unwrap();
    assert_eq!(view.subtitle, "s");
    assert!(h.running());
    assert_eq!(state.get("t.helper").unwrap(), Some(serde_json::json!(["b"])));
    let file = root.join("a.exmod");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(h.importer().unwrap().claims(&file), Claim::Yes);
    assert_eq!(h.claims(&root.join("a.txt")), Claim::No);
    let seen = Seen::default();
    let lines = h.import(&cx, std::slice::from_ref(&file), &seen).unwrap();
    assert_eq!(lines, ["imported a.exmod"]);
    assert_eq!(*seen.0.lock().unwrap(), [(0.5, "half".to_owned())]);
    let second = profile("other");
    let cx = Cx::new("t.helper", &second, &data, &cache, None, &state);
    let options = BTreeMap::new();
    let batch = h
        .build(&BuildCx {
            cx: Cx::new("t.helper", &second, &data, &cache, None, &state),
            options: &options,
            features: &Features::default(),
        })
        .unwrap();
    assert!(matches!(batch.ops[0].op, AssetOp::File(_)));
    assert_eq!(batch.ops[0].origin.addon, "t.helper");
    assert_eq!(batch.ops[0].origin.item.as_deref(), Some("mod.a"));
    assert_eq!(batch.ops[1].origin.item, None);
    assert_eq!(batch.lines, ["one"]);
    assert_eq!(batch.fields["t.r"], ["a"]);
    let e = h.command(&cx, &["x".into()], &[]).unwrap_err();
    assert_eq!(e.to_string(), "Helper: nope");
    assert!(h.running());
    let received = std::fs::read_to_string(dir.join("received")).unwrap();
    let received: Vec<Value> = received
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let methods: Vec<&str> = received
        .iter()
        .map(|m| m["method"].as_str().unwrap_or("reply"))
        .collect();
    assert_eq!(
        methods,
        [
            "hello", "view", "reply", "reply", "import", "profile", "build", "command"
        ]
    );
    assert_eq!(received[0]["params"]["api"], API_VERSION);
    assert_eq!(received[0]["params"]["addon"], "t.helper");
    assert_eq!(received[2], serde_json::json!({"id": 9, "result": ["a"]}));
    assert_eq!(received[5]["params"]["name"], "other");
    h.stop();
    assert!(!h.running());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn pausing_stops_helpers_and_keeps_them_from_starting_until_dropped() {
    let root = tmp("pause");
    let store = Store::new(root.join("data"));
    let dir = store.addons_dir().join("t.helper");
    addon(
        &dir,
        r#"read -r l; echo '{"id":0,"result":{"api":1}}'
while read -r l; do echo '{"id":1,"result":{"subtitle":"s","body":{"blocks":[]}}}'; done
"#,
    );
    let h = store
        .helpers()
        .get(&AddonManifest::load(&dir).unwrap(), &dir)
        .unwrap();
    let state = Memory::default();
    let (data, cache) = (root.join("state"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let p = profile("default");
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
    h.view(&cx, "items").unwrap();
    assert!(h.running());
    let pause = store.helpers().pause();
    assert!(!h.running());
    assert!(h.view(&cx, "items").is_err());
    assert!(h.try_view(&cx, "items").is_none());
    assert!(!h.running());
    assert!(h.failure().is_none());
    drop(pause);
    h.view(&cx, "items").unwrap();
    assert!(h.running());
    h.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn removing_an_addon_stops_its_helper_before_deleting_it() {
    let root = tmp("remove");
    let store = Store::new(root.join("data"));
    let dir = store.addons_dir().join("t.helper");
    addon(
        &dir,
        r#"echo $$ > pid
read -r l; echo '{"id":0,"result":{"api":1}}'
read -r l; echo '{"id":1,"result":{"subtitle":"s","body":{"blocks":[]}}}'
exec sleep 60
"#,
    );
    let h = store
        .helpers()
        .get(&AddonManifest::load(&dir).unwrap(), &dir)
        .unwrap();
    let state = Memory::default();
    let (data, cache) = (root.join("state"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let p = profile("default");
    h.view(&Cx::new("t.helper", &p, &data, &cache, None, &state), "items")
        .unwrap();
    assert!(h.running());
    let pid = std::fs::read_to_string(dir.join("pid")).unwrap();
    store
        .remove_addon_with("t.helper", |d| {
            assert!(!h.running());
            let alive = Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(!alive.success(), "the helper still runs");
            std::fs::remove_dir_all(d)
        })
        .unwrap();
    assert!(!dir.exists());
    std::fs::remove_dir_all(&root).ok();
}

struct FakeGame(PathBuf);

impl GameFiles for FakeGame {
    fn build_key(&self) -> String {
        "25251167-abc".into()
    }

    fn exe_version(&self) -> Option<String> {
        None
    }

    fn is_stock(&self, _: &str) -> bool {
        false
    }

    fn original(&self, rel: &str) -> Result<PathBuf> {
        Err(swofty_addon::Error::Other(rel.into()))
    }

    fn original_lang(&self, code: &str) -> Result<PathBuf> {
        Err(swofty_addon::Error::Other(code.into()))
    }

    fn dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }

    fn status(&self) -> GameStatus {
        GameStatus {
            running: true,
            pid: Some(7),
            since: Some(100.5),
            session: Some("s1".into()),
        }
    }

    fn steam_save_dir(&self) -> Option<PathBuf> {
        Some(self.0.join("remote"))
    }

    fn save_dir(&self, profile: &str) -> Option<PathBuf> {
        Some(self.0.join("saves").join(profile))
    }
}

const LIVE: &str = r#"
answer() { read -r l; echo "$l" >> received; echo "$1"; }
answer '{"id":0,"result":{"api":1}}'
answer '{"id":0,"method":"game_dir"}'
answer '{"id":1,"method":"addon_game_dir"}'
answer '{"id":2,"method":"game_status"}'
answer '{"id":3,"method":"steam_save_dir"}'
answer '{"id":4,"method":"save_dir"}'
read -r l; echo "$l" >> received
echo '{"method":"invalidate","params":{"section":"items"}}'
echo '{"id":1,"result":{"subtitle":"s","body":{"blocks":[]},"tick":true}}'
read -r l; echo "$l" >> received
echo '{"method":"invalidate","params":{}}'
echo '{"id":2,"result":null}'
answer '{"id":3,"error":"nope"}'
echo '{"method":"invalidate","params":{"section":"late"}}'
answer '{"id":4,"result":null}'
read -r l
"#;

#[test]
fn ticks_invalidates_and_answers_the_game_host_calls() {
    let root = tmp("live");
    let dir = root.join("addon");
    addon(&dir, LIVE);
    let h = helper(&dir);
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let state = Memory::default();
    let p = profile("default");
    let game = FakeGame(root.join("F1 25"));
    let cx = Cx::new("t.helper", &p, &data, &cache, Some(&game), &state);
    assert!(
        h.try_tick(&cx, "items").is_none(),
        "a tick never starts the plugin"
    );
    let view = h.view(&cx, "items").unwrap();
    assert!(view.tick);
    assert_eq!(
        h.take_stale(),
        Some(Stale::Sections(BTreeSet::from(["items".to_owned()])))
    );
    assert_eq!(h.take_stale(), None);
    assert!(!h.busy());
    h.try_tick(&cx, "items").unwrap().unwrap();
    assert_eq!(h.take_stale(), Some(Stale::All));
    let e = h.try_tick(&cx, "items").unwrap().unwrap_err();
    assert_eq!(e.to_string(), "Helper: nope");
    assert!(h.running());
    h.try_tick(&cx, "items").unwrap().unwrap();
    assert_eq!(
        h.take_stale(),
        Some(Stale::Sections(BTreeSet::from(["late".to_owned()]))),
        "an invalidate between replies is taken before the next reply"
    );
    let received: Vec<Value> = std::fs::read_to_string(dir.join("received"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(received[0]["params"]["launcher"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        received[0]["params"]["host_calls"],
        serde_json::json!(["tasks", PROFILE_HEADS, SAVE_DIR])
    );
    let game_dir = root.join("F1 25");
    assert_eq!(received[2]["result"], game_dir.to_string_lossy().as_ref());
    assert_eq!(
        received[3]["result"],
        game_dir.join("swofty/addons/t.helper").to_string_lossy().as_ref()
    );
    assert_eq!(
        received[4]["result"],
        serde_json::json!({"running": true, "pid": 7, "since": 100.5, "session": "s1"})
    );
    assert_eq!(
        received[5]["result"],
        game_dir.join("remote").to_string_lossy().as_ref()
    );
    assert_eq!(
        received[6]["result"],
        game_dir.join("saves/default").to_string_lossy().as_ref()
    );
    assert_eq!(received[7]["method"], "tick");
    assert_eq!(received[7]["params"]["section"], "items");
    h.stop();
    let none = Cx::new("t.helper", &p, &data, &cache, None, &state);
    assert_eq!(host_call(&none, HostCall::GameDir).unwrap(), Value::Null);
    assert_eq!(host_call(&none, HostCall::AddonGameDir).unwrap(), Value::Null);
    assert_eq!(host_call(&none, HostCall::ProfileTracks).unwrap(), Value::Null);
    assert_eq!(host_call(&none, HostCall::SaveDir).unwrap(), Value::Null);
    assert!(host_call(&none, HostCall::Invalidate(Default::default())).is_err());
    let e = host_call(&none, HostCall::Addons).unwrap_err();
    assert!(e.contains("can't be managed"), "{e}");
    let e = host_call(
        &none,
        HostCall::RemoveAddon {
            id: "t.helper".into(),
        },
    )
    .unwrap_err();
    assert!(e.contains("itself"), "{e}");
    std::fs::remove_dir_all(&root).ok();
}

#[derive(Default)]
struct Moves(Mutex<Vec<(String, isize)>>);

impl swofty_addon::launcher::AddonStore for Moves {
    fn list(&self) -> Result<Vec<swofty_addon::launcher::AddonInfo>> {
        Ok(Vec::new())
    }

    fn install(&self, _: &std::path::Path) -> Result<swofty_addon::launcher::AddonInfo> {
        Err(swofty_addon::Error::Other("no".into()))
    }

    fn set_enabled(&self, _: &str, _: bool) -> Result<()> {
        Ok(())
    }

    fn remove(&self, _: &str) -> Result<()> {
        Ok(())
    }

    fn move_addon(&self, id: &str, delta: isize) -> Result<()> {
        self.0.lock().unwrap().push((id.into(), delta));
        Ok(())
    }
}

#[test]
fn lists_and_answers_move_addon_beside_the_addon_calls() {
    let root = tmp("move");
    let (data, cache) = (root.join("data"), root.join("cache"));
    let state = Memory::default();
    let p = profile("default");
    let moves = Moves::default();
    let none = Cx::new("t.helper", &p, &data, &cache, None, &state);
    assert_eq!(host_calls(&none), ["tasks", SAVE_DIR]);
    let e = host_call(
        &none,
        HostCall::MoveAddon {
            id: "t.a".into(),
            delta: 1,
        },
    )
    .unwrap_err();
    assert!(e.contains("can't be managed"), "{e}");
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state).with_addons(&moves);
    assert_eq!(
        host_calls(&cx),
        [
            "addons",
            "install_addon",
            "set_addon_enabled",
            "remove_addon",
            "move_addon",
            "tasks",
            SAVE_DIR
        ]
    );
    let call: HostCall = serde_json::from_value(
        serde_json::json!({"method": "move_addon", "params": {"id": "t.a", "delta": -3}}),
    )
    .unwrap();
    assert_eq!(host_call(&cx, call).unwrap(), Value::Null);
    assert_eq!(*moves.0.lock().unwrap(), [("t.a".to_owned(), -3)]);
}

#[derive(Default)]
struct Shown(Mutex<Vec<swofty_addon::launcher::TaskList>>);

impl swofty_addon::launcher::Tasks for Shown {
    fn show(&self, list: &swofty_addon::launcher::TaskList) {
        self.0.lock().unwrap().push(list.clone());
    }
}

const TASK_LISTS: &str = r#"
answer() { read -r l; echo "$l" >> received; echo "$1"; }
answer '{"id":0,"result":{"api":1}}'
read -r l
echo '{"method":"tasks","params":{"title":"T","items":[{"id":"a","label":"A","state":"running"}]}}'
echo '{"method":"tasks","params":{"title":"T","items":[{"id":"a","label":"A","state":"failed","note":"bad"}]}}'
echo '{"id":1,"result":{"notice":"done"}}'
read -r l
echo '{"method":"tasks","params":{"title":"T","items":[]}}'
echo '{"id":2,"result":"none"}'
read -r l
"#;

#[test]
fn forwards_task_lists_to_the_event_that_shows_them() {
    let root = tmp("tasks");
    let dir = root.join("addon");
    addon(&dir, TASK_LISTS);
    let h = helper(&dir);
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let (state, shown, p) = (Memory::default(), Shown::default(), profile("default"));
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state).with_tasks(&shown);
    let effect = h.event(&cx, "items", Event::Startup {}).unwrap();
    assert_eq!(effect, Effect::Notice("done".into()));
    let states: Vec<_> = shown.0.lock().unwrap().iter().map(|l| l.items[0].state).collect();
    assert_eq!(
        states,
        [
            swofty_addon::launcher::TaskState::Running,
            swofty_addon::launcher::TaskState::Failed
        ]
    );
    let plain = Cx::new("t.helper", &p, &data, &cache, None, &state);
    assert_eq!(h.event(&plain, "items", Event::Startup {}).unwrap(), Effect::None);
    assert_eq!(shown.0.lock().unwrap().len(), 2);
    assert!(h.running());
    h.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn stale_sections_merge_until_taken() {
    let mut slot = None;
    Stale::add(&mut slot, Some("a".into()));
    Stale::add(&mut slot, Some("b".into()));
    let both = slot.clone().unwrap();
    assert!(both.covers("a") && both.covers("b") && !both.covers("c"));
    Stale::add(&mut slot, None);
    Stale::add(&mut slot, Some("c".into()));
    assert_eq!(slot, Some(Stale::All));
    assert_eq!(
        invalidation(r#"{"method":"invalidate","params":{"section":"x"}}"#),
        Some(Some("x".into()))
    );
    assert_eq!(invalidation(r#"{"method":"invalidate"}"#), Some(None));
    assert_eq!(invalidation(r#"{"id":4,"method":"invalidate"}"#), None);
    assert_eq!(invalidation(r#"{"id":4,"result":"invalidate"}"#), None);
    assert_eq!(invalidation("not json, invalidate"), None);
}

#[test]
fn a_busy_helper_keeps_its_page_and_summary_without_waiting() {
    let root = tmp("busy");
    let dir = root.join("addon");
    addon(
        &dir,
        r#"
read -r l; echo '{"id":0,"result":{"api":1}}'
read -r l; echo '{"id":1,"result":{"subtitle":"s","body":{"blocks":[]}}}'
read -r l; echo '{"id":2,"result":{"label":"MODS","items":["A"]}}'
read -r l; sleep 1; echo '{"id":3,"result":"none"}'
read -r l; echo '{"id":4,"result":{"subtitle":"t","body":{"blocks":[]}}}'
read -r l
"#,
    );
    let manifest = AddonManifest::load(&dir).unwrap();
    let h = Arc::new(HelperAddon::new(manifest, &dir).unwrap().with_timeouts(Timeouts {
        quick: Duration::from_secs(2),
        long: Duration::from_secs(5),
        start: PATIENT,
    }));
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let state = Memory::default();
    let p = profile("default");
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
    assert_eq!(h.try_view(&cx, "items").unwrap().unwrap().subtitle, "s");
    let summary = h.summary(&cx).unwrap();
    assert_eq!(summary.items, ["A"]);
    let busy = {
        let (h, data, cache) = (h.clone(), data.clone(), cache.clone());
        std::thread::spawn(move || {
            let state = Memory::default();
            let p = profile("default");
            let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
            h.event(&cx, "items", Event::Click { id: "x".into() }).unwrap()
        })
    };
    while !h.busy() && !busy.is_finished() {
        std::thread::yield_now();
    }
    let asked = std::time::Instant::now();
    assert!(h.try_view(&cx, "items").is_none());
    assert_eq!(h.summary(&cx), Some(summary));
    assert!(asked.elapsed() < Duration::from_millis(200));
    assert_eq!(busy.join().unwrap(), Effect::None);
    assert_eq!(h.try_view(&cx, "items").unwrap().unwrap().subtitle, "t");
    h.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_page_with_toneless_text_keeps_the_helper_running_for_the_build() {
    let root = tmp("toneless");
    let dir = root.join("addon");
    addon(
        &dir,
        r#"
read -r l; echo '{"id":0,"result":{"api":1}}'
read -r l; echo '{"id":1,"result":{"body":{"blocks":[{"label":"LEGACY CAREER"},{"text":{"text":"Start-year tiles are under Settings."}}]},"subtitle":"s"}}'
read -r l; echo '{"id":2,"result":{"ops":[],"lines":["built"]}}'
read -r l
"#,
    );
    let h = helper(&dir);
    let state = Memory::default();
    let p = profile("default");
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
    let view = h.view(&cx, "items").unwrap();
    assert!(view.notices.is_empty(), "{:?}", view.notices);
    let Body::Blocks(blocks) = &view.body else {
        panic!("blocks");
    };
    assert!(matches!(
        blocks[1],
        swofty_addon::view::Block::Text {
            tone: Tone::Neutral,
            ..
        }
    ));
    assert!(h.failure().is_none());
    let options = BTreeMap::new();
    let batch = h
        .build(&BuildCx {
            cx: Cx::new("t.helper", &p, &data, &cache, None, &state),
            options: &options,
            features: &Features::default(),
        })
        .unwrap();
    assert_eq!(batch.lines, ["built"]);
    h.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn contains_hangs_crashes_and_bad_helpers() {
    let root = tmp("failures");
    let hello = r#"read -r l; echo '{"id":0,"result":{"api":1}}'"#;
    let cases = [
        (
            "hang",
            format!("{hello}\nread -r l; exec sleep 5"),
            "did not answer within 0.5 s",
        ),
        (
            "crash",
            format!("{hello}\nread -r l; exit 3"),
            "stopped (exit status: 3)",
        ),
        (
            "junk",
            format!("{hello}\nread -r l; echo hello there; read -r l"),
            "sent a bad message",
        ),
        (
            "shape",
            format!("{hello}\nread -r l; echo '{{\"id\":1,\"result\":7}}'; read -r l"),
            "sent a bad result",
        ),
        (
            "old",
            r#"read -r l; echo '{"id":0,"result":{"api":0}}'; read -r l"#.into(),
            "speaks launcher API 0",
        ),
    ];
    for (name, script, _) in &cases {
        addon(&root.join(name), script);
    }
    let state = Memory::default();
    let p = profile("default");
    let cache = root.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let cx = Cx::new("t.helper", &p, &root, &cache, None, &state);
    for (name, _, error) in cases {
        let wait = if name == "hang" { HUNG } else { PATIENT };
        let h = helper_waiting(&root.join(name), wait);
        let view = h.view(&cx, "items").unwrap();
        let notice = &view.notices[0];
        assert!(
            notice.text.starts_with(&format!("Helper {error}")),
            "{name}: {}",
            notice.text
        );
        assert_eq!(notice.action.as_ref().unwrap().id, RESTART);
        assert!(matches!(view.body, Body::Empty { .. }));
        assert!(!h.running() && h.failure().is_some());
        let click = Event::Click { id: "x".into() };
        assert!(h.event(&cx, "items", click).is_err(), "{name}");
        let restart = Event::Click { id: RESTART.into() };
        assert_eq!(h.event(&cx, "items", restart).unwrap(), Effect::Reload);
        assert!(h.failure().is_none());
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_program_names_the_antivirus() {
    let root = tmp("missing");
    let dir = root.join("t.helper");
    addon(&dir, "read -r l");
    std::fs::remove_file(dir.join("bin/helper")).unwrap();
    let state = Memory::default();
    let p = profile("default");
    let cache = root.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let cx = Cx::new("t.helper", &p, &root, &cache, None, &state);
    let view = helper(&dir).view(&cx, "items").unwrap();
    assert_eq!(
        view.notices[0].text,
        format!(
            "Helper could not start: its program bin/helper is missing, most likely removed by an antivirus. \
                 Allow {} in your antivirus, then reinstall Helper",
            dir.display()
        )
    );
    let manifest = AddonManifest::load(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let view = HelperAddon::new(manifest, &dir)
        .unwrap()
        .view(&cx, "items")
        .unwrap();
    assert!(
        view.notices[0].text.starts_with("Helper could not start "),
        "{}",
        view.notices[0].text
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_managed_addon_comes_back_on_play() {
    let root = tmp("managed-missing");
    let dir = root.join(crate::updates::managed::ID);
    addon(&dir, "read -r l");
    std::fs::remove_file(dir.join("bin/helper")).unwrap();
    let manifest = AddonManifest {
        id: crate::updates::managed::ID.into(),
        name: crate::updates::managed::NAME.into(),
        ..AddonManifest::load(&dir).unwrap()
    };
    let h = HelperAddon::new(manifest, &dir).unwrap();
    assert!(
        h.removed("bin/helper")
            .ends_with("then press Play again to download it again"),
        "{}",
        h.removed("bin/helper")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_helper_only_runs_for_a_drop_while_it_is_on() {
    let root = tmp("off-claims");
    let store = Store::new(root.join("data"));
    let dir = store.addons_dir().join("t.helper");
    let ran = root.join("ran");
    addon(&dir, &format!("touch '{}'\necho yes", ran.display()));
    let manifest = MANIFEST.replace(r#""imports":["exmod"]"#, r#""imports":["exmod"],"claims":true"#);
    std::fs::write(dir.join("swofty.json"), manifest).unwrap();
    let car = root.join("car.exmod");
    std::fs::write(&car, "x").unwrap();
    let reg = store.registry().unwrap();

    let mut off = Profile::new("t");
    off.set_enabled("t.helper", false).unwrap();
    for profile in [&off, &Profile::new("absent"), &Profile::vanilla()] {
        assert_eq!(route(&reg, profile, &car), None);
        assert!(!ran.exists(), "{} ran the helper", profile.name);
    }

    let mut on = Profile::new("t");
    on.set_enabled("t.helper", true).unwrap();
    assert_eq!(route(&reg, &on, &car), Some(Route::Importer("t.helper".into())));
    assert!(ran.exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn routes_drops_to_a_helper_without_starting_it() {
    let root = tmp("routes");
    let store = Store::new(root.join("data"));
    addon(&store.addons_dir().join("t.helper"), "exit 1");
    std::fs::write(root.join("car.exmod"), "x").unwrap();
    std::fs::write(root.join("car.txt"), "x").unwrap();
    let reg = store.registry().unwrap();
    let entry = reg.get("t.helper").unwrap();
    assert!(entry.helper && !entry.declared);
    assert_eq!(entry.addon.sections()[0].id, "items");
    let mut profile = Profile::new("t");
    profile.set_enabled("t.helper", true).unwrap();
    assert_eq!(
        route(&reg, &profile, &root.join("car.exmod")),
        Some(Route::Importer("t.helper".into()))
    );
    assert_eq!(route(&reg, &profile, &root.join("car.txt")), None);
    let again = store.registry().unwrap();
    assert!(std::ptr::addr_eq(
        Arc::as_ptr(&entry.addon),
        Arc::as_ptr(&again.get("t.helper").unwrap().addon)
    ));
    assert!(
        !store
            .helpers()
            .get(
                &entry.addon.manifest().clone(),
                &store.addons_dir().join("t.helper")
            )
            .unwrap()
            .running()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn asks_a_helper_that_claims_about_each_drop() {
    let root = tmp("claims");
    let dir = root.join("addon");
    let script = r#"case "$1:$2" in
  claims:*.serps|claims:*/mod) echo yes ;;
  claims:*.zip) echo maybe ;;
  claims:*.slow) exec sleep 5 ;;
  claims:*) echo no ;;
esac"#;
    addon(&dir, script);
    let manifest = MANIFEST.replace(r#""imports":["exmod"]"#, r#""imports":["exmod"],"claims":true"#);
    std::fs::write(dir.join("swofty.json"), manifest).unwrap();
    let h = helper(&dir);
    let hung = helper_waiting(&dir, HUNG);
    let file = |name: &str| {
        let p = root.join(name);
        std::fs::write(&p, "x").unwrap();
        p
    };
    assert_eq!(h.claims(&file("a.serps")), Claim::Yes);
    assert_eq!(h.claims(&file("a.zip")), Claim::Maybe);
    assert_eq!(h.claims(&file("a.txt")), Claim::No);
    std::fs::create_dir_all(root.join("mod")).unwrap();
    assert_eq!(h.claims(&root.join("mod")), Claim::Yes);
    assert_eq!(h.claims(&file("a.exmod")), Claim::No, "the helper's answer wins");
    assert_eq!(
        hung.claims(&file("b.slow")),
        Claim::No,
        "a stalled helper claims nothing"
    );
    assert!(!h.running(), "claims runs apart from the live session");
    std::fs::remove_dir_all(&root).ok();
}

// A helper that logs each start to `starts` beside its addon folder, asks `asks` once during the build and
// replies with `reply`'s fields beside one op and one line.
fn reading_script(root: &Path, asks: &str, reply: &str) -> String {
    let starts = root.join("starts");
    format!(
        r#"echo start >> "{}"
read -r l; echo '{{"id":0,"result":{{"api":1}}}}'
read -r l
echo '{{"id":9,"method":"{asks}","params":null}}'
read -r l
echo '{{"id":1,"result":{{"ops":[{{"op":"file.put","source":"a.dds","target":"b.dds"}}],"lines":["built"]{reply}}}}}'
read -r l
"#,
        starts.display()
    )
}

// Written executable, so that starting it does not change the addon's folder during the build.
fn reading_addon(dir: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    addon(dir, script);
    std::fs::set_permissions(dir.join("bin/helper"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn starts(root: &Path) -> usize {
    std::fs::read_to_string(root.join("starts")).map_or(0, |s| s.lines().count())
}

struct Reading {
    root: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    kept: PathBuf,
    state: Memory,
}

impl Reading {
    fn new(name: &str) -> Self {
        let root = tmp(name);
        let (data, cache) = (root.join("data"), root.join("cache"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        Self {
            kept: root.join("kept/t.helper.json"),
            root,
            data,
            cache,
            state: Memory::default(),
        }
    }

    fn build(
        &self,
        h: &HelperAddon,
        profile: &ProfileInfo,
        options: &BTreeMap<String, Value>,
    ) -> (AssetBatch, HookRun) {
        let cx = BuildCx {
            cx: Cx::new("t.helper", profile, &self.data, &self.cache, None, &self.state),
            options,
            features: &Features::default(),
        };
        let (batch, ran) = h.build_kept(&cx, &self.kept).unwrap();
        h.stop();
        (batch, ran.unwrap())
    }
}

#[test]
fn a_build_reply_that_lists_its_reads_answers_the_next_build_until_one_changes() {
    let r = Reading::new("reads");
    let dir = r.root.join("addon");
    let (file, folder, missing) = (
        r.root.join("read.txt"),
        r.root.join("read-dir"),
        r.root.join("absent"),
    );
    std::fs::write(&file, "a").unwrap();
    std::fs::create_dir_all(folder.join("sub")).unwrap();
    std::fs::write(folder.join("sub/x"), "x").unwrap();
    let reads = format!(
        r#","reads":["{}","{}","{}"]"#,
        file.display(),
        folder.display(),
        missing.display()
    );
    reading_addon(&dir, &reading_script(&r.root, "get_state", &reads));
    let h = helper(&dir).settled_after(Duration::ZERO);
    let (p, options) = (profile("default"), BTreeMap::new());
    let (first, ran) = r.build(&h, &p, &options);
    assert_eq!(ran, HookRun::Ran("no reply is kept from an earlier build".into()));
    assert_eq!(first.lines, ["built"]);
    assert!(first.reads.is_some());
    let (again, ran) = r.build(&h, &p, &options);
    assert_eq!(again, first);
    assert_eq!(
        ran.to_string(),
        "reused: 3 read path(s), 1 host call answer(s) and its own and data folders are unchanged"
    );
    assert_eq!(starts(&r.root), 1, "a reused reply starts no helper");

    let ran_after = |change: &dyn Fn(), options: &BTreeMap<String, Value>, p: &ProfileInfo| {
        change();
        let before = starts(&r.root);
        let (batch, ran) = r.build(&h, p, options);
        assert_eq!((batch, starts(&r.root)), (first.clone(), before + 1));
        ran.to_string()
    };
    let none = || {};
    let changed = |p: &Path| format!("ran: {} changed", p.display());
    assert_eq!(
        ran_after(&|| std::fs::write(&file, "bb").unwrap(), &options, &p),
        changed(&file)
    );
    assert_eq!(
        ran_after(
            &|| std::fs::write(folder.join("sub/y"), "y").unwrap(),
            &options,
            &p
        ),
        changed(&folder)
    );
    assert_eq!(
        ran_after(&|| std::fs::write(&missing, "now").unwrap(), &options, &p),
        changed(&missing)
    );
    assert_eq!(
        ran_after(
            &|| r.state.set("t.helper", serde_json::json!(1)).unwrap(),
            &options,
            &p
        ),
        "ran: the launcher's answer to get_state changed"
    );
    assert_eq!(
        ran_after(&|| std::fs::write(r.data.join("d"), "d").unwrap(), &options, &p),
        "ran: its data folder changed"
    );
    assert_eq!(
        ran_after(&|| std::fs::write(dir.join("extra"), "e").unwrap(), &options, &p),
        "ran: its own folder changed"
    );
    let sized = BTreeMap::from([("size".to_owned(), serde_json::json!(24))]);
    assert_eq!(
        ran_after(&none, &sized, &p),
        "ran: its options or the launcher's features changed"
    );
    assert_eq!(
        ran_after(&none, &sized, &profile("other")),
        "ran: the hello's profile changed"
    );
    let (_, ran) = r.build(&h, &profile("other"), &sized);
    assert!(matches!(ran, HookRun::Reused(_)), "{ran}");
    std::fs::remove_dir_all(&r.root).ok();
}

#[test]
fn a_build_reply_is_not_kept_without_reads_after_a_change_it_made_or_one_too_recent() {
    type Why = fn(&Path) -> String;
    let cases: [(&str, &str, Duration, Why); 4] = [
        ("get_state", "", Duration::ZERO, |_| {
            "ran: its last reply listed no reads".into()
        }),
        ("set_state", r#","reads":[]"#, Duration::ZERO, |_| {
            "ran: its last build sent set_state".into()
        }),
        ("get_state", r#","reads":["rel.txt"]"#, Duration::ZERO, |_| {
            "ran: its last reply's reads name rel.txt, which is not an absolute path".into()
        }),
        ("get_state", "fresh", Duration::from_secs(2), |root| {
            format!(
                "ran: {} changed less than 2 s before its last build ended",
                root.join("fresh.txt").display()
            )
        }),
    ];
    for (i, (asks, reply, settled, why)) in cases.into_iter().enumerate() {
        let r = Reading::new(&format!("not-kept-{i}"));
        let fresh = r.root.join("fresh.txt");
        let reply = match reply {
            "fresh" => format!(r#","reads":["{}"]"#, fresh.display()),
            other => other.to_owned(),
        };
        let dir = r.root.join("addon");
        reading_addon(&dir, &reading_script(&r.root, asks, &reply));
        if !settled.is_zero() {
            std::thread::sleep(settled + Duration::from_millis(100));
            std::fs::write(&fresh, "new").unwrap();
        }
        let h = helper(&dir).settled_after(settled);
        let (p, options) = (profile("default"), BTreeMap::new());
        r.build(&h, &p, &options);
        let (_, ran) = r.build(&h, &p, &options);
        assert_eq!(ran.to_string(), why(&r.root), "case {i}");
        assert_eq!(starts(&r.root), 2, "case {i}");
        std::fs::remove_dir_all(&r.root).ok();
    }
}

fn timed(dir: &Path, quick: Duration, start: Duration) -> HelperAddon {
    let manifest = AddonManifest::load(dir).unwrap();
    HelperAddon::new(manifest, dir).unwrap().with_timeouts(Timeouts {
        quick,
        long: PATIENT,
        start,
    })
}

fn start_notes(cache: &Path) -> Vec<String> {
    std::fs::read_to_string(cache.join(LOG_FILE))
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once(" launcher: ").map(|(_, note)| note.to_owned()))
        .collect()
}

fn started_pid(notes: &[String]) -> String {
    assert_eq!(notes[0], "starting bin/helper", "{notes:?}");
    let (pid, _) = notes[1]
        .strip_prefix("pid ")
        .and_then(|n| n.split_once(" started in "))
        .unwrap_or_else(|| panic!("{notes:?}"));
    pid.to_owned()
}

#[test]
fn the_first_hello_waits_a_minute_and_quick_calls_five_seconds() {
    let t = Timeouts::default();
    assert_eq!(t.quick, Duration::from_secs(5));
    assert_eq!(t.start, Duration::from_secs(60));
    assert_eq!(t.long, Duration::from_secs(300));
}

#[test]
fn a_slow_first_hello_gets_the_start_deadline_and_later_calls_stay_quick() {
    let root = tmp("slow-hello");
    let dir = root.join("addon");
    addon(
        &dir,
        r#"sleep 2
read -r l; echo '{"id":0,"result":{"api":1}}'
read -r l; echo '{"id":1,"result":"none"}'
read -r l; exec sleep 30
"#,
    );
    let h = Arc::new(timed(&dir, HUNG, PATIENT));
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let state = Memory::default();
    let p = profile("default");
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
    let asked = Instant::now();
    let starting = {
        let (h, data, cache) = (h.clone(), data.clone(), cache.clone());
        std::thread::spawn(move || {
            let state = Memory::default();
            let p = profile("default");
            let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
            h.event(&cx, "items", Event::Click { id: "x".into() })
        })
    };
    while !h.busy() && !starting.is_finished() {
        std::thread::yield_now();
    }
    let peeked = Instant::now();
    assert!(
        h.try_view(&cx, "items").is_none(),
        "a view never waits for a starting helper"
    );
    assert!(!h.ready());
    assert!(peeked.elapsed() < Duration::from_millis(200));
    assert_eq!(starting.join().unwrap().unwrap(), Effect::None);
    assert!(asked.elapsed() >= Duration::from_secs(2), "{:?}", asked.elapsed());
    assert!(h.ready() && h.failure().is_none());

    let asked = Instant::now();
    let view = h.view(&cx, "items").unwrap();
    assert_eq!(view.notices[0].text, "Helper did not answer within 0.5 s");
    assert!(asked.elapsed() < Duration::from_secs(10), "{:?}", asked.elapsed());

    let notes = start_notes(&cache);
    let pid = started_pid(&notes);
    assert_eq!(
        notes[2],
        format!("pid {pid} has not said hello after 0.5 s, waiting up to 60 s")
    );
    let after: f32 = notes[3]
        .strip_prefix(&format!("pid {pid} said hello after "))
        .and_then(|n| n.strip_suffix(" s"))
        .unwrap_or_else(|| panic!("{notes:?}"))
        .parse()
        .unwrap();
    assert!(after >= 2.0, "{notes:?}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_helper_that_never_says_hello_fails_at_the_start_deadline_naming_the_antivirus() {
    let root = tmp("no-hello");
    let dir = root.join("addon");
    addon(&dir, "echo $$ > pid\nexec sleep 30\n");
    let h = timed(&dir, HUNG, Duration::from_secs(2));
    let (data, cache) = (root.join("data"), root.join("cache"));
    std::fs::create_dir_all(&cache).unwrap();
    let state = Memory::default();
    let p = profile("default");
    let cx = Cx::new("t.helper", &p, &data, &cache, None, &state);
    let asked = Instant::now();
    let view = h.view(&cx, "items").unwrap();
    assert!(asked.elapsed() >= Duration::from_secs(2), "{:?}", asked.elapsed());
    let failure = format!(
        "was still starting after 2 s: an antivirus may be scanning it. Allow {} in your antivirus, then press \
         Restart",
        dir.display()
    );
    assert_eq!(view.notices[0].text, format!("Helper {failure}"));
    assert_eq!(view.notices[0].action.as_ref().unwrap().id, RESTART);
    assert!(!h.running() && h.failure().is_some());

    let notes = start_notes(&cache);
    let pid = started_pid(&notes);
    assert_eq!(std::fs::read_to_string(dir.join("pid")).unwrap().trim(), pid);
    let alive = Command::new("kill")
        .args(["-0", &pid])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!alive.success(), "the silent helper still runs");
    assert_eq!(
        notes[2],
        format!("pid {pid} has not said hello after 0.5 s, waiting up to 2 s")
    );
    assert!(
        notes[3].starts_with(&format!("pid {pid} {failure} (")),
        "{notes:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}
