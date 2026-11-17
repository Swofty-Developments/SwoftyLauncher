#[cfg(unix)]
use std::collections::BTreeMap;
use std::process::ExitCode;

use clap::Parser;
use swofty_core::store::Store;
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::sandbox::{RunDir, RunOptions};

use crate::commands::testing::fixture_command::capture_command;
#[cfg(unix)]
use crate::commands::testing::harness::Harness;

use crate::commands::testing::fakes::*;
use crate::commands::testing::*;

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
