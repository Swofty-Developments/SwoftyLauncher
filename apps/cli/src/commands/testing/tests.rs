use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use swofty_core::store::Store;
use swofty_core::testsuite::plan;
use swofty_core::testsuite::report::{Report, Status, exit};
use swofty_core::testsuite::suite::{self, Selector};

use super::fixture_command::capture_command;
use super::game_lock::{hh_mm, take_lock};
use super::harness::Harness;
use super::preflight::{addons_installed, batch_fixture, check_sandbox, terms};
use super::run::Stop;
use super::shell::career_mode;
use swofty_core::addon_storage;
#[cfg(unix)]
use swofty_core::builtin::CORE_ID;
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::lock::GameLock;
use swofty_core::testsuite::plan::Batch;
use swofty_core::testsuite::sandbox::{self, RunDir, RunOptions};

use super::fakes::*;
use super::*;

#[cfg(unix)]
const SUITE: &str = r#"{
  "schema": 1,
  "suite": "me.fake",
  "tests": [
    { "id": "boot_ok", "env": { "kind": "boot" },
      "steps": [ { "wait": { "event": "game.ready" }, "timeout_s": 10 } ],
      "assert": [ { "log": "\\[my\\] hello" }, { "event": "game.ready", "count": 1 }, { "no_crash": true },
                  { "perf": { "p99_ms": { "max": 40 } } } ] },
    { "id": "boot_fail", "env": { "kind": "boot" }, "assert": [ { "log": "\\[my\\] goodbye" } ] },
    { "id": "needs_setup", "env": { "kind": "time_trial" }, "steps": [ { "route": "contracts" } ] },
    { "id": "crashes", "fresh": true, "env": { "kind": "boot" },
      "steps": [ { "wait": { "log": "never" }, "timeout_s": 10 } ], "assert": [ { "no_crash": true } ] },
    { "id": "race", "env": { "kind": "benchmark", "track": "bahrain", "laps": 1, "cars": 4 },
      "steps": [ { "wait": { "event": "race.start" }, "timeout_s": 10 } ],
      "assert": [ { "event": "race.start" }, { "no_crash": true }, { "perf": { "p99_ms": { "max": 20 } } } ] },
    { "id": "gp", "env": { "kind": "grand_prix" }, "assert": [ { "no_crash": true } ] }
  ]
}"#;

#[cfg(unix)]
#[test]
fn a_fake_game_runs_every_batch_and_puts_the_folder_back() {
    let root = temp("fake-run");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(SUITE, Path::new("tests/fake.test.json"), None).unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    assert_eq!(plan.game_starts(), 3);
    let mut fake = Fake {
        scripts: vec![BOOT, CRASH, BENCH],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let mut report = Report::new("r1", &out.display().to_string());
    report.game_starts = plan.game_starts();
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r1",
        launcher: &mut fake,
        times: quick(),
        run_id: "r1",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    report.exit_code = report.compute_exit();

    let summary = report.summary();
    assert_eq!(
        status_of(&report, "boot_ok").status,
        Status::Pass,
        "{summary}\n{:#?}",
        report.tests
    );
    let fail = status_of(&report, "boot_fail");
    assert_eq!(fail.status, Status::Fail);
    assert_eq!(
        fail.failures[0].text(),
        "expected log /\\[my\\] goodbye/ at least 1 time(s), got 0 times"
    );
    assert_eq!(fail.failures[0].evidence.as_deref(), Some("batches/1/loader.log"));
    assert_eq!(status_of(&report, "needs_setup").status, Status::Skipped);
    assert_eq!(
        status_of(&report, "needs_setup").reason.as_deref(),
        Some(suite::NEEDS_SESSION_SETUP)
    );
    assert_eq!(
        status_of(&report, "gp").reason.as_deref(),
        Some(suite::NEEDS_SESSION_SETUP)
    );

    let crash = status_of(&report, "crashes");
    assert_eq!(crash.status, Status::Fail);
    assert_eq!(
        crash.failures[0].text(),
        "expected a log line /never/, got none before the game exited"
    );
    assert_eq!(
        crash.failures[1].text(),
        "expected no crash, got the game crashed (exception 0xc0000005 in F1_25.exe)"
    );
    let race = status_of(&report, "race");
    assert_eq!(race.status, Status::Fail);
    assert_eq!(race.failures.len(), 1, "{:?}", race.failures);
    assert_eq!(
        race.failures[0].text(),
        "expected p99 frame time at most 20 ms, got 22.5 ms"
    );

    assert_eq!(fake.launches.len(), 3, "the session setup batches start no game");
    assert_eq!(fake.launches[0], ("test-r1".to_owned(), vec![]));
    assert_eq!(fake.launches[2].1, ["-benchmark", "swofty_test.xml"]);
    assert_eq!(
        std::fs::read_to_string(game.join("args.txt")).unwrap().trim(),
        "-benchmark swofty_test.xml"
    );
    let seen = std::fs::read_to_string(game.join("seen.xml")).unwrap();
    assert!(
        seen.contains("<track name=\"bahrain\" laps=\"1\" weather=\"clear\" num_cars=\"4\""),
        "{seen}"
    );
    assert!(
        !game.join("benchmark").exists(),
        "the staged XML and its folder are gone"
    );

    let batches: Vec<(&str, bool, Option<bool>)> = report
        .batches
        .iter()
        .map(|b| (b.env.as_str(), b.started_game, b.clean))
        .collect();
    assert_eq!(
        batches,
        [
            ("boot", true, Some(true)),
            ("boot", true, Some(true)),
            ("benchmark bahrain 1 lap 4 cars", true, Some(true)),
            ("grand_prix", false, None),
            ("time_trial", false, None),
        ]
    );
    assert_eq!(
        report.batches[0].end.as_deref(),
        Some("stopped: the test run closed F1 25")
    );
    assert!(
        report.batches[1]
            .notes
            .iter()
            .any(|n| n.contains("the game crashed"))
    );
    assert!(out.join("batches/1/loader.log").is_file());
    assert!(out.join("batches/3/perf.log").is_file());
    assert!(out.join("batches/1/launcher.log").is_file());
    let profile = store.profile("test-r1").unwrap();
    assert_eq!(
        profile.enabled().collect::<Vec<_>>(),
        [CORE_ID],
        "only the suite's addons, none here"
    );
    assert_eq!(report.exit_code, exit::FAIL);
    assert!(
        summary.contains("2 passed") || summary.contains("1 passed"),
        "{summary}"
    );
    assert!(summary.contains("boot_ok"), "{summary}");
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_game_that_never_reaches_the_menu_is_an_error_and_is_closed() {
    let root = temp("fake-stuck");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"s","tests":[{"id":"menu","env":{"kind":"boot"},"assert":[{"no_crash":true}]}]}"#,
        Path::new("s.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    let mut fake = Fake {
        scripts: vec![STUCK],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let mut report = Report::new("r2", "out");
    let mut times = quick();
    times.start = Duration::from_millis(400);
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r2",
        launcher: &mut fake,
        times,
        run_id: "r2",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    let t = &report.tests[0];
    assert_eq!(t.status, Status::Error);
    assert_eq!(
        t.reason.as_deref(),
        Some("the batch did not start: the game did not reach the main menu within 0 s")
    );
    assert_eq!(
        report.batches[0].end.as_deref(),
        Some("stopped: the test run closed F1 25")
    );
    assert_eq!(report.batches[0].clean, Some(true));
    assert_eq!(report.compute_exit(), exit::INFRA);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_addon_stops_the_batch_before_the_game_starts() {
    let root = temp("fake-addon");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"s","defaults":{"addons":["not.here"]},
            "tests":[{"id":"menu","env":{"kind":"benchmark","track":"monza"},"assert":[{"no_crash":true}]}]}"#,
        Path::new("s.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    assert!(
        addons_installed(&store, &plan)
            .unwrap_err()
            .contains("not.here is not installed")
    );
    let mut fake = Fake {
        scripts: vec![],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let mut report = Report::new("r3", "out");
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r3",
        launcher: &mut fake,
        times: quick(),
        run_id: "r1",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    assert!(fake.launches.is_empty());
    assert_eq!(report.tests[0].status, Status::Error);
    assert!(
        report.batches[0]
            .error
            .as_deref()
            .unwrap()
            .contains("not.here is not installed"),
        "{:?}",
        report.batches[0].error
    );
    assert!(!game.join("benchmark").exists(), "nothing staged");
    assert_eq!(report.compute_exit(), exit::INFRA);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_held_lock_is_busy_and_unaccepted_terms_are_infra() {
    let root = temp("lock-busy");
    let dir = root.join("game.lock.d");
    let held = GameLock::try_acquire(&dir, "someone else 10:00")
        .unwrap()
        .unwrap();
    match take_lock(&dir, "swofty-test r s 10:01", Duration::ZERO) {
        Err(Stop::Busy(why)) => assert!(why.contains("held by \"someone else 10:00\""), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(dir.is_dir());
    drop(held);
    let lock = take_lock(&dir, "swofty-test r s 10:01", Duration::ZERO).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("owner")).unwrap(),
        "swofty-test r s 10:01\n"
    );
    drop(lock);
    match take_lock(&root.join("missing/x.lock.d"), "x", Duration::ZERO) {
        Err(Stop::Infra(why)) => assert!(why.contains("SWOFTY_GAME_LOCK"), "{why}"),
        other => panic!("{other:?}"),
    }
    let store = Store::new(root.join("data"));
    let why = terms(&store).unwrap_err();
    assert!(why.contains("a test run never accepts them for you"), "{why}");
    swofty_core::offline::record_accept(&store.root).unwrap();
    assert!(terms(&store).is_ok());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn waits_parse_as_seconds_minutes_or_hours() {
    assert_eq!(parse_wait("0"), Ok(Duration::ZERO));
    assert_eq!(parse_wait("90s"), Ok(Duration::from_secs(90)));
    assert_eq!(parse_wait("30m"), Ok(Duration::from_secs(1800)));
    assert_eq!(parse_wait("2h"), Ok(Duration::from_secs(7200)));
    assert!(parse_wait("soon").is_err());
    assert!(parse_wait("5d").is_err());
}

#[test]
fn the_cli_takes_the_documented_forms() {
    let parse = |args: &[&str]| crate::Cli::try_parse_from(args).map(|c| c.command);
    let Ok(crate::Command::Test(TestCommand::Run(a))) = parse(&[
        "swofty",
        "test",
        "run",
        "./my-addon",
        "-t",
        "smoke",
        "--take-over",
        "--wait",
        "0",
    ]) else {
        panic!("test run");
    };
    assert_eq!(a.select.paths, [PathBuf::from("./my-addon")]);
    assert_eq!(a.select.tags, ["smoke"]);
    assert!(a.take_over);
    assert_eq!(a.wait, Duration::ZERO);
    let Ok(crate::Command::Test(TestCommand::Run(a))) = parse(&["swofty", "test", "run", "x"]) else {
        panic!("defaults");
    };
    assert_eq!(
        a.wait,
        Duration::from_secs(1800),
        "a run waits for the game by default"
    );
    let Ok(crate::Command::Test(TestCommand::Plan(p))) =
        parse(&["swofty", "test", "plan", "./my-addon", "--id", "a", "--id", "s/b"])
    else {
        panic!("test plan");
    };
    assert_eq!(p.select.ids, ["a", "s/b"]);
    assert!(parse(&["swofty", "test", "plan"]).is_err(), "a path is required");
    assert!(
        parse(&["swofty", "run", "--game-arg=-benchmark"]).is_err(),
        "game arguments need a direct launch"
    );
    assert!(
        parse(&[
            "swofty",
            "run",
            "--direct",
            "--game-arg=-benchmark",
            "--game-arg",
            "x.xml"
        ])
        .is_ok()
    );
    crate::Cli::command().debug_assert();
}

#[test]
fn a_run_of_only_skipped_tests_starts_nothing_and_passes() {
    let root = temp("skipped-only");
    let addon = root.join("addon");
    std::fs::create_dir_all(addon.join("tests")).unwrap();
    std::fs::write(
        addon.join("tests/a.test.json"),
        r#"{"schema":1,"suite":"s","tests":[{"id":"c","env":{"kind":"grand_prix"},"assert":[{"no_crash":true}]},
            {"id":"r","env":{"kind":"time_trial"},"assert":[{"no_crash":true}]}]}"#,
    )
    .unwrap();
    let out = root.join("out");
    let store = Store::new(root.join("data"));
    let cli = crate::Cli::try_parse_from([
        "swofty",
        "test",
        "run",
        addon.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--lock",
        root.join("never/used.lock.d").to_str().unwrap(),
    ])
    .unwrap();
    let crate::Command::Test(cmd) = cli.command else {
        panic!("test");
    };
    assert_eq!(
        command(&store, None, "default", cmd).unwrap(),
        ExitCode::from(exit::PASS)
    );
    let r: Report = serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(r.game_starts, 0);
    assert!(r.tests.iter().all(|t| t.status == Status::Skipped));
    assert!(out.join("junit.xml").is_file() && out.join("summary.txt").is_file());
    assert!(
        !root.join("never").exists(),
        "no lock is taken when no game starts"
    );

    let none = crate::Cli::try_parse_from(["swofty", "test", "plan", addon.to_str().unwrap(), "-t", "nope"])
        .unwrap();
    let crate::Command::Test(cmd) = none.command else {
        panic!("test");
    };
    assert_eq!(
        command(&store, None, "default", cmd).unwrap(),
        ExitCode::from(exit::INFRA)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_owner_line_has_a_clock_time() {
    let t = hh_mm();
    assert_eq!(t.len(), 5, "{t}");
    assert_eq!(&t[2..3], ":");
}

#[cfg(unix)]
#[test]
fn steps_run_through_a_fake_agent_and_checks_use_what_it_read() {
    let root = temp("fake-agent");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(root.join("tests/native")).unwrap();
    std::fs::write(root.join("tests/native/tests.dll"), b"MZ").unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"me.agent","tests":[
          {"id":"routes","env":{"kind":"route","routes":["career"]},
           "steps":[{"read":"career.money","as":"before"},{"wait":{"event":"career.hub"}},{"read":"career.money","as":"after"}],
           "assert":[{"value":"after","greater_than":"before"},{"value":"car[0].chassis.mass","between":[790,830]},
                     {"value":"before","equals":1500},{"no_crash":true}]},
          {"id":"unbound","env":{"kind":"route","routes":["career"]},"steps":[{"set":{"car[0].chassis.mass":900}}]},
          {"id":"too_light","env":{"kind":"route","routes":["career"]},"assert":[{"value":"car[0].chassis.mass","less_than":700}]},
          {"id":"code","env":{"kind":"route","routes":["career"]},"fn":"rnd_tab_hidden"}
        ]}"#,
        &root.join("tests/agent.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    assert_eq!(plan.game_starts(), 1);
    let mut fake = Fake {
        scripts: vec![AGENT_GAME],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let agent_thread = fake_agent(game.clone(), AgentPlay::default());
    let mut report = Report::new("r4", &out.display().to_string());
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r4",
        launcher: &mut fake,
        times: quick(),
        run_id: "r4",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    let seen = agent_thread.join().unwrap();
    let summary = report.summary();

    assert!(
        same_command(&seen[0], "title", "null"),
        "the title screen is passed without input"
    );
    assert!(
        same_command(&seen[1], "route", r#"{"route":"career"}"#),
        "the route environment's route"
    );
    assert!(
        seen.iter()
            .any(|l| same_command(l, "wait", r#"{"event":"career.hub"}"#)),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|l| same_command(l, "run_test", r#"{"fn":"rnd_tab_hidden"}"#)),
        "{seen:?}"
    );
    assert_eq!(
        seen.last().unwrap(),
        "quit null",
        "the batch ends with the agent's quit"
    );
    assert_eq!(
        status_of(&report, "routes").status,
        Status::Pass,
        "{summary}\n{:#?}",
        report.tests
    );
    let unbound = status_of(&report, "unbound");
    assert_eq!(unbound.status, Status::Fail);
    assert_eq!(
        unbound.failures[0].actual,
        "unavailable: the car struct is not bound in this build"
    );
    assert_eq!(
        status_of(&report, "too_light").failures[0].text(),
        "expected less than 700, got 800.5"
    );
    let code = status_of(&report, "code");
    assert_eq!(code.status, Status::Fail);
    assert_eq!(code.failures.len(), 1);
    assert_eq!(code.failures[0].expected, "the R&D tab is hidden");
    assert_eq!(fake.launches.len(), 1);
    assert_eq!(report.batches[0].end.as_deref(), Some("quit: the game quit"));
    let env = std::fs::read_to_string(game.join("env.txt")).unwrap();
    let run = data.join("test-runs/r4-1");
    assert_eq!(
        env.trim_end(),
        format!("{}|{}|", run.display(), fake.envs[0][1].1),
        "the run folder and its token, no early stage without the sandbox"
    );
    let kept = std::fs::read_to_string(out.join("batches/1/agent.jsonl")).unwrap();
    assert!(
        kept.contains(r#""event":"hello""#) && kept.contains(r#"dll/me.agent.dll"#),
        "{kept}"
    );
    assert!(
        !run.exists(),
        "a run folder without the sandbox is removed after the batch"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_game_without_the_agent_fails_the_batch_at_hello() {
    let root = temp("no-agent");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"me.x","tests":[{"id":"a","env":{"kind":"benchmark","track":"bahrain"},"steps":[{"read":"x"}]}]}"#,
        Path::new("tests/x.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    let mut fake = Fake {
        scripts: vec![STUCK],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let mut report = Report::new("r5", &out.display().to_string());
    let mut times = quick();
    times.start = Duration::from_millis(600);
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r5",
        launcher: &mut fake,
        times,
        run_id: "r5",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    let t = status_of(&report, "a");
    assert_eq!(t.status, Status::Error);
    assert!(
        t.reason
            .as_deref()
            .unwrap()
            .contains("the test agent did not say hello"),
        "{:?}",
        t.reason
    );
    assert!(
        !data.join("test-runs/r5-1").exists(),
        "the run folder is removed after a failed batch too"
    );
    std::fs::remove_dir_all(&root).ok();
}

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
            sha256: swofty_core::baseline::sha256_bytes(b"{}"),
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
    h.close_sandbox(sb, Some(&a.run), b, "test-r9-1", &mut br, &mut reports, &[0]);
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
    h.close_sandbox(sb, Some(&a.run), b, "test-r9-2", &mut br, &mut reports, &[0]);
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
    h.close_sandbox(sb, Some(&a.run), b, "test-r9-3", &mut br, &mut reports, &[0]);
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
    h.close_sandbox(sb, Some(&a.run), b, "test-r9-4", &mut br, &mut reports, &[0]);
    let why = br.error.unwrap();
    assert!(
        why.starts_with("THE PLAYER'S STEAM SAVE FOLDER CHANGED")
            && why.contains("savegame@profile#QKRHMYXE changed"),
        "{why}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn shell_is_a_test_command() {
    let cli = crate::Cli::try_parse_from(["swofty", "test", "shell", "--addon", "me.x", "--wait", "0"]);
    assert!(cli.is_ok(), "{:?}", cli.err().map(|e| e.to_string()));
    let Ok(crate::Command::Test(TestCommand::Shell(a))) =
        crate::Cli::try_parse_from(["swofty", "test", "shell", "--fixture", "my-team-r3"]).map(|c| c.command)
    else {
        panic!("shell --fixture");
    };
    assert_eq!(a.fixture.as_deref(), Some("my-team-r3"));
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

#[cfg(unix)]
#[test]
fn the_title_falls_back_to_the_pad_only_when_the_agent_cannot_pass_it() {
    let root = temp("title-pad");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"me.t","tests":[{"id":"menu","env":{"kind":"route","routes":["career"]},"assert":[{"no_crash":true}]}]}"#,
        Path::new("tests/t.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    let mut fake = Fake {
        scripts: vec![AGENT_GAME],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let agent_thread = fake_agent(
        game.clone(),
        AgentPlay {
            title_unbound: true,
            ..AgentPlay::default()
        },
    );
    let mut report = Report::new("r6", &out.display().to_string());
    Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-r6",
        launcher: &mut fake,
        times: quick(),
        run_id: "r6",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    let seen = agent_thread.join().unwrap();
    assert_eq!(
        status_of(&report, "menu").status,
        Status::Pass,
        "{:#?}",
        report.tests
    );
    assert!(
        same_command(&seen[0], "title", "null"),
        "the title command comes first"
    );
    assert!(
        same_command(&seen[1], "pad", r#"{"ms":250,"press":"a"}"#),
        "the pad only after the title answered unavailable"
    );

    assert_eq!(
        seen.iter().filter(|c| c.starts_with("title")).count(),
        1,
        "title is sent once"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_boot_batch_waits_for_the_home_screen() {
    let root = temp("boot-home");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let s = suite::parse(
        r#"{"schema":1,"suite":"me.t","tests":[{"id":"menu","env":{"kind":"boot"},"assert":[{"no_crash":true}]}]}"#,
        Path::new("tests/t.test.json"),
        None,
    )
    .unwrap();
    let plan = plan::plan(&[s], &Selector::default());
    let mut fake = Fake {
        scripts: vec![AGENT_GAME],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let agent_thread = fake_agent(
        game.clone(),
        AgentPlay {
            title_unbound: true,
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
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    }
    .run(&plan, &mut report);
    let seen = agent_thread.join().unwrap();
    assert_eq!(
        status_of(&report, "menu").status,
        Status::Pass,
        "{:#?}",
        report.tests
    );
    let title = seen.iter().position(|c| c.starts_with("title"));
    let home = seen.iter().position(|c| same_command(c, "home", "null"));
    assert!(
        title.is_some() && home.is_some() && title < home,
        "a boot batch passes the title, then waits for the home screen: {seen:?}"
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

#[test]
fn a_capture_from_a_run_records_the_runs_exe_and_addons() {
    let root = temp("capture-run");
    let data = root.join("data");
    let store = Store::new(&data);
    let run = RunDir::create(
        &data,
        "r5-1",
        RunOptions {
            sandbox: true,
            exe_sha256: Some("cafe".into()),
            dlls: Vec::new(),
        },
    )
    .unwrap();
    run.retire().unwrap();
    std::fs::write(run.remote().join(fixture::PROFILE_FILES[0]), [1u8; 10]).unwrap();
    std::fs::write(run.remote().join(fixture::PROFILE_FILES[1]), [2u8; 416]).unwrap();
    std::fs::write(run.root.join("addons.json"), r#"{"swofty.progressive":"1.6.0"}"#).unwrap();
    std::fs::create_dir_all(run.storage().join("swofty.progressive")).unwrap();
    std::fs::write(run.storage().join("swofty.progressive/careers.json"), "{}").unwrap();
    let fixtures = root.join("fixtures");
    let cli = crate::Cli::try_parse_from([
        "swofty",
        "test",
        "fixture",
        "capture",
        "prog-r1",
        "--run",
        "r5-1",
        "--career",
        "driver_career_progressive:2",
        "--store",
        fixtures.to_str().unwrap(),
    ])
    .unwrap();
    let crate::Command::Test(TestCommand::Fixture(FixtureCommand::Capture(a))) = cli.command else {
        panic!("not a capture");
    };
    assert_eq!(capture_command(&store, a).unwrap(), ExitCode::SUCCESS);
    let f = fixture::load(&fixtures.join("prog-r1")).unwrap();
    assert_eq!(f.manifest.exe.sha256, "cafe");
    assert_eq!(f.manifest.addons["swofty.progressive"], "1.6.0");
    assert_eq!(f.manifest.holds.careers[0].slot, Some(2));
    assert!(
        f.manifest
            .files
            .contains_key("storage/swofty.progressive/careers.json")
    );
    assert_eq!(f.manifest.source, "test run r5-1");
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_shell_profile_is_a_copy_of_the_named_profile() {
    let root = temp("profile-copy");
    let (data, game, out) = (root.join("data"), root.join("game"), root.join("out"));
    std::fs::create_dir_all(&game).unwrap();
    let store = Store::new(&data);
    let mut base = store.profile("base").unwrap();
    base.set_enabled("a.one", true).unwrap();
    base.set_enabled("b.two", false).unwrap();
    base.options.insert(
        "a.one".to_owned(),
        BTreeMap::from([("speed".to_owned(), serde_json::json!(3))]),
    );
    store.save_profile(&base).unwrap();
    let mut fake = Fake {
        scripts: vec![],
        root: root.clone(),
        game: game.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let h = Harness {
        store: &store,
        game: &game,
        out: &out,
        profile: "test-shell-x",
        launcher: &mut fake,
        times: quick(),
        run_id: "shell-x",
        bases: BTreeMap::new(),
        exe_sha: None,
        live_saves: None,
    };
    h.profile_from("base", "test-shell-x", &[]).unwrap();
    let copy = store.profile("test-shell-x").unwrap();
    let base = store.profile("base").unwrap();
    let ids = |p: &swofty_core::profile::Profile| {
        p.addons
            .iter()
            .map(|a| (a.id.clone(), a.enabled))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&copy), ids(&base), "same addons, order and switches");
    assert_eq!(copy.options, base.options, "same settings");
    assert!(
        h.profile_from("missing", "test-shell-y", &[])
            .unwrap_err()
            .to_string()
            .contains("there is no profile missing")
    );
    assert!(
        h.profile_from("base", "test-shell-z", &["not.installed".to_owned()])
            .unwrap_err()
            .to_string()
            .contains("not.installed is not installed")
    );
    std::fs::remove_dir_all(&root).ok();
}
