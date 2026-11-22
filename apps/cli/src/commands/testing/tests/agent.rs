use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use swofty_core::store::Store;
use swofty_core::testsuite::plan;
use swofty_core::testsuite::report::{Report, Status};
use swofty_core::testsuite::suite::{self, Selector};

use crate::commands::testing::harness::Harness;

use crate::commands::testing::fakes::*;

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
