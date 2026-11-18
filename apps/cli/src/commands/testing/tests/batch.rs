use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
#[cfg(unix)]
use std::time::Duration;

use clap::Parser;
#[cfg(unix)]
use swofty_core::addons::builtin::CORE_ID;
use swofty_core::store::Store;
use swofty_core::testsuite::plan;
use swofty_core::testsuite::report::{Report, Status, exit};
use swofty_core::testsuite::suite::{self, Selector};

use crate::commands::testing::harness::Harness;
use crate::commands::testing::preflight::addons_installed;

use crate::commands::testing::fakes::*;
use crate::commands::testing::*;

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
