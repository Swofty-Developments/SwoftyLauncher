use std::path::Path;

use swofty_core::addons::storage as addon_storage;
use swofty_core::store::Store;
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::plan;
use swofty_core::testsuite::plan::Batch;
#[cfg(unix)]
use swofty_core::testsuite::report::Report;
use swofty_core::testsuite::report::Status;
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::suite::{self, Selector};

use crate::commands::testing::harness::{BatchResults, Harness, career_mode};
use crate::commands::testing::preflight::{batch_fixture, check_sandbox};

use crate::commands::testing::fakes::*;
use crate::commands::testing::*;

const CAREER_SUITE: &str = r#"{
  "schema": 1,
  "suite": "my.career",
  "tests": [
    { "id": "hq", "env": { "kind": "career", "fixture": "fixtures/r3", "at": "team_hq" }, "fn": "hq" }
  ]
}"#;

fn sandbox_bindings(loader: &Path, sha: &str, ids: &[&str]) {
    let core = loader.join("swofty/addons/core");
    std::fs::create_dir_all(&core).unwrap();
    let b: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| serde_json::json!({"id": id, "kind": "func", "status": "experimental"}))
        .collect();
    std::fs::write(
        core.join("bindings.json"),
        serde_json::to_vec(&serde_json::json!({"build": {"sha256": sha}, "bindings": b})).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_career_run_is_refused_unless_the_sandbox_is_verified_for_the_exe() {
    let root = temp("sandbox-preflight");
    let tests_dir = root.join("addon/tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    let file = tests_dir.join("c.test.json");
    let s = suite::parse(CAREER_SUITE, &file, None).unwrap();
    let suites = vec![s];
    let plan = plan::plan(&suites, &Selector::default());
    let batches: Vec<&Batch> = plan.batches.iter().collect();
    let b = bases(&suites);
    let store = root.join("store");
    let none = check_sandbox(None, "aa", &batches, &b, &store).unwrap_err();
    assert!(none.contains("loader"), "{none}");
    let loader = root.join("loader");
    sandbox_bindings(&loader, "aa", &["Ne_SaveSystem2Win_Start"]);
    let lacking = check_sandbox(Some(&loader), "aa", &batches, &b, &store).unwrap_err();
    assert!(
        lacking.starts_with("a career environment needs the save sandbox")
            && lacking.contains("lack Ne_SaveSystem2Win_Mount"),
        "{lacking}"
    );
    sandbox_bindings(&loader, "aa", sandbox::BINDINGS);
    let other = check_sandbox(Some(&loader), "bb", &batches, &b, &store).unwrap_err();
    assert!(other.contains("not verified for this build"), "{other}");
    let missing = check_sandbox(Some(&loader), "aa", &batches, &b, &store).unwrap_err();
    assert!(missing.contains("fixture \"fixtures/r3\" not found"), "{missing}");
    fixture_at(&tests_dir, "r3", "aa");
    check_sandbox(Some(&loader), "aa", &batches, &b, &store).unwrap();
    std::fs::write(
        tests_dir.join("fixtures/r3/save").join(fixture::PROFILE_FILES[1]),
        [0u8; 416],
    )
    .unwrap();
    let edited = check_sandbox(Some(&loader), "aa", &batches, &b, &store).unwrap_err();
    assert!(edited.contains("does not match fixture.json"), "{edited}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_career_batch_runs_in_its_own_sandbox_and_fails_closed_on_any_doubt() {
    let root = temp("sandbox-batch");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let tests_dir = root.join("addon/tests");
    std::fs::create_dir_all(tests_dir.join("native")).unwrap();
    std::fs::write(tests_dir.join("native/tests.dll"), b"MZ").unwrap();
    fixture_at(&tests_dir, "r3", "aa");
    let storage = tests_dir.join("fixtures/r3/storage/my.addon");
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(storage.join("careers.json"), "{}").unwrap();
    let f = tests_dir.join("fixtures/r3/fixture.json");
    let mut m: fixture::Manifest = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    m.files.insert(
        "storage/my.addon/careers.json".into(),
        fixture::FileSum {
            size: 2,
            sha256: swofty_core::game_folder::baseline::sha256_bytes(b"{}"),
        },
    );
    std::fs::write(&f, serde_json::to_vec(&m).unwrap()).unwrap();
    let s = suite::parse(CAREER_SUITE, &tests_dir.join("c.test.json"), None).unwrap();
    let suites = vec![s];
    let plan = plan::plan(&suites, &Selector::default());
    let live = root.join("live-remote");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join(fixture::PROFILE_FILES[1]), [1u8; 416]).unwrap();
    let store = Store::new(&data);
    let mut fake = Fake {
        scripts: Vec::new(),
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let h = Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r9",
        launcher: &mut fake,
        times: quick(),
        run_id: "r9",
        bases: bases(&suites),
        exe_sha: Some("aa".into()),
        live_saves: Some(live.clone()),
    };
    let b = &plan.batches[0];
    let open = |i: usize| {
        let a = h.open_run(b, i, &out, true).unwrap();
        let f = batch_fixture(b, &h.bases, &fixture::store(&data)).unwrap();
        let sb = h.open_sandbox(f, b, &format!("test-r9-{i}"), &a.run).unwrap();
        (a, sb)
    };
    let (a, sb) = open(1);
    assert_eq!(a.run.root, data.join("test-runs/r9-1"));
    assert_eq!(
        std::fs::read(data.join("test-runs/r9-1/agent/dll/my.career.dll")).unwrap(),
        b"MZ",
        "the suite's test DLL is staged in the run's agent folder"
    );
    assert_eq!(
        std::fs::read(a.run.remote().join(fixture::PROFILE_FILES[1])).unwrap(),
        [8u8; 416],
        "the fixture's saves are in the sandbox"
    );
    assert_eq!(
        std::fs::read_to_string(
            addon_storage::profile_dir(&data, "my.addon", "test-r9-1").join("careers.json")
        )
        .unwrap(),
        "{}",
        "the fixture's storage is the batch profile's copy"
    );
    let env = a.run.env();
    assert!(
        env.iter()
            .any(|(k, v)| k == sandbox::RUN_ENV && Path::new(v) == a.run.root)
    );
    assert!(env.iter().any(|(k, v)| k == sandbox::TOKEN_ENV && v.len() == 32));
    assert!(env.contains(&(sandbox::EARLY_ENV.to_owned(), "1".to_owned())));

    let mut br = Harness::blank(b, 1).0;
    br.started_game = true;
    let mut reports = Harness::blank(b, 1).1;
    reports[0].status = Status::Pass;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-r9-1",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    h.close_run(Some(a), &mut br);
    assert_eq!(
        br.error.as_deref(),
        Some(
            "the core never reported the save sandbox (no sandbox.json): the save device may not have started"
        )
    );
    assert_eq!(reports[0].status, Status::Error);
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data.join("test-runs/r9-1/run.json")).unwrap()).unwrap();
    assert!(
        doc.get("token").is_none(),
        "the run token is retired after the batch"
    );
    assert_eq!(doc["saves"], "saves", "a retired run.json still names its saves");
    assert_eq!(doc["exe_sha256"], "aa", "and its exe, for a capture");
    assert!(
        data.join("test-runs/r9-1/saves/remote")
            .join(fixture::PROFILE_FILES[1])
            .is_file(),
        "a sandboxed run folder is kept for a capture"
    );

    let (a, sb) = open(2);
    std::fs::write(
        a.run.root.join(sandbox::STATUS_FILE),
        r#"{"state":"failed","why":"Ne_SaveSystem2Win_Start does not resolve on this build"}"#,
    )
    .unwrap();
    let mut br = Harness::blank(b, 2).0;
    let mut reports = Harness::blank(b, 2).1;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-r9-2",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    assert_eq!(
        br.error.as_deref(),
        Some("the save sandbox did not arm: Ne_SaveSystem2Win_Start does not resolve on this build")
    );

    let (a, sb) = open(3);
    std::fs::write(a.run.root.join(sandbox::STATUS_FILE), r#"{"state":"armed"}"#).unwrap();
    std::fs::write(a.run.remote().join("savegame@profile#QKRHMYXE"), [5u8; 416]).unwrap();
    let mut br = Harness::blank(b, 3).0;
    let mut reports = Harness::blank(b, 3).1;
    reports[0].status = Status::Pass;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-r9-3",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    assert_eq!(
        br.error, None,
        "an armed sandbox and an unchanged live folder pass"
    );
    assert_eq!(reports[0].status, Status::Pass);
    assert_eq!(
        std::fs::read_to_string(data.join("test-runs/r9-3/storage/my.addon/careers.json")).unwrap(),
        "{}",
        "the batch's addon storage is kept in the run folder for a capture"
    );

    let (a, sb) = open(4);
    std::fs::write(a.run.root.join(sandbox::STATUS_FILE), r#"{"state":"armed"}"#).unwrap();
    std::fs::write(live.join(fixture::PROFILE_FILES[1]), [2u8; 417]).unwrap();
    let mut br = Harness::blank(b, 4).0;
    let mut reports = Harness::blank(b, 4).1;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-r9-4",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    let why = br.error.unwrap();
    assert!(
        why.starts_with("THE PLAYER'S STEAM SAVE FOLDER CHANGED")
            && why.contains("savegame@profile#QKRHMYXE changed"),
        "{why}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
const CAREER_RUN_SUITE: &str = r#"{
  "schema": 1,
  "suite": "my.career",
  "tests": [
    { "id": "hq", "env": { "kind": "career", "fixture": "fixtures/r3" }, "assert": [ { "no_crash": true } ] }
  ]
}"#;

#[cfg(unix)]
#[test]
fn a_career_batch_loads_the_career_through_the_agent_and_keeps_its_saves() {
    let root = temp("career-batch");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let tests_dir = root.join("addon/tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    fixture_at(&tests_dir, "r3", "aa");
    let s = suite::parse(CAREER_RUN_SUITE, &tests_dir.join("c.test.json"), None).unwrap();
    let suites = vec![s];
    let plan = plan::plan(&suites, &Selector::default());
    assert_eq!(plan.game_starts(), 1, "a career test runs now");
    assert!(plan.batches[0].tests[0].needs_agent());
    let store = Store::new(&data);
    store.create_profile("mine", None).unwrap();
    let mut fake = Fake {
        scripts: vec![CAREER_GAME],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let agent_thread = fake_agent(
        game.clone(),
        AgentPlay {
            career_ok: true,
            ..AgentPlay::default()
        },
    );
    let mut report = Report::new("r7", &out.display().to_string());
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r7",
        launcher: &mut fake,
        times: quick(),
        run_id: "r7",
        bases: bases(&suites),
        exe_sha: Some("aa".into()),
        live_saves: None,
    }
    .run(&plan, &mut report);
    let seen = agent_thread.join().unwrap();
    let t = status_of(&report, "hq");
    assert_eq!(
        t.status,
        Status::Pass,
        "{:#?}\n{:#?}",
        report.tests,
        report.batches
    );
    assert!(
        same_command(
            &seen[0],
            "career",
            r#"{"at":"team_hq","op":"load","slot":"newest"}"#
        ),
        "the CLI asks the agent to continue the newest career into Team HQ"
    );
    assert_eq!(seen.last().unwrap(), "quit null");
    assert_eq!(
        fake.launches[0].0, "test-r7-1",
        "a career batch has its own profile"
    );
    let env = std::fs::read_to_string(game.join("env.txt")).unwrap();
    let run = data.join("test-runs/r7-1");
    assert!(
        env.starts_with(&format!("{}|", run.display())) && env.trim_end().ends_with("|1"),
        "the run folder, its token and the early stage for the sandbox: {env}"
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run.join(sandbox::RUN_FILE)).unwrap()).unwrap();
    assert!(doc.get("token").is_none(), "the token is retired after the batch");
    assert!(
        run.join("saves/remote").join(fixture::PROFILE_FILES[0]).is_file(),
        "the sandbox's saves are kept for a capture"
    );
    let names = store.profile_names().unwrap();
    assert!(
        !names.contains(&"test-r7-1".to_owned()),
        "the batch profile is deleted: {names:?} {:?}",
        report.batches[0].notes
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_career_load_that_fails_fails_the_batch_with_the_agents_words() {
    let root = temp("career-fail");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let tests_dir = root.join("addon/tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    fixture_at(&tests_dir, "r3", "aa");
    let s = suite::parse(CAREER_RUN_SUITE, &tests_dir.join("c.test.json"), None).unwrap();
    let suites = vec![s];
    let plan = plan::plan(&suites, &Selector::default());
    let store = Store::new(&data);
    let mut fake = Fake {
        scripts: vec![CAREER_GAME],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let agent_thread = fake_agent(game.clone(), AgentPlay::default());
    let mut report = Report::new("r8", &out.display().to_string());
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r8",
        launcher: &mut fake,
        times: quick(),
        run_id: "r8",
        bases: bases(&suites),
        exe_sha: Some("aa".into()),
        live_saves: None,
    }
    .run(&plan, &mut report);
    agent_thread.join().unwrap();
    let t = status_of(&report, "hq");
    assert_eq!(t.status, Status::Error);
    assert!(
        t.reason
            .as_deref()
            .unwrap()
            .contains("career load: the resume card's press did not happen within 30 s"),
        "{:?}",
        t.reason
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_fixtures_career_names_the_mode_to_continue() {
    let root = temp("career-mode");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    fixture_at(&tests_dir, "m", "aa");
    let mut f = fixture::load(&tests_dir.join("fixtures/m")).unwrap();
    assert_eq!(career_mode(&f, None), None, "no careers listed");
    f.manifest.holds.careers = vec![fixture::Career {
        mode: "my_team_career_26".into(),
        slot: Some(3),
        ..Default::default()
    }];
    assert_eq!(career_mode(&f, None).as_deref(), Some("my_team_career"));
    assert_eq!(career_mode(&f, Some(3)).as_deref(), Some("my_team_career"));
    assert_eq!(career_mode(&f, Some(4)), None, "no career in that slot");
    f.manifest.holds.careers.push(fixture::Career {
        mode: "driver_career".into(),
        slot: Some(4),
        ..Default::default()
    });
    assert_eq!(
        career_mode(&f, None),
        None,
        "two careers and no slot: the agent picks"
    );
    assert_eq!(career_mode(&f, Some(4)).as_deref(), Some("driver_career"));
    std::fs::remove_dir_all(&root).ok();
}
