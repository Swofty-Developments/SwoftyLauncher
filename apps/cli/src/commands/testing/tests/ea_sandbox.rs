use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use swofty_core::saves::vault::Layout;
use swofty_core::store::Store;
use swofty_core::testsuite::fixture::{self, Meta};
use swofty_core::testsuite::plan::{self, Batch};
use swofty_core::testsuite::report::Status;
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::suite::{self, Selector};

use crate::commands::testing::fixture_command::capture_command;
use crate::commands::testing::harness::{BatchResults, Harness};
use crate::commands::testing::preflight::{batch_fixture, check_sandbox};

use crate::commands::testing::fakes::*;
use crate::commands::testing::*;

const EA_SUITE: &str = r#"{
  "schema": 1,
  "suite": "my.career",
  "tests": [
    { "id": "hq", "env": { "kind": "career", "fixture": "fixtures/ea-r3", "at": "team_hq" }, "fn": "hq" }
  ]
}"#;

const SHA: &str = "ea145";

/// An EA app copy of the game, its Wine prefix and the player's profile save in it.
fn fake_ea_copy(root: &Path) -> (swofty_core::install::Game, PathBuf) {
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("__Installer")).unwrap();
    std::fs::write(dir.join(swofty_core::GAME_EXE), b"exe").unwrap();
    std::fs::write(dir.join("__Installer/installerdata.xml"), b"<game/>").unwrap();
    let pfx = root.join("pfx");
    let profile = pfx.join("drive_c/users/steamuser/AppData/Local/F1 25/savegame/ea/profile");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(pfx.join("user.reg"), b"").unwrap();
    std::fs::write(profile.join("NXDSMWW.BWW"), [1u8; 900]).unwrap();
    std::fs::write(profile.join("QKRHMYXE"), [1u8; 416]).unwrap();
    let game = swofty_core::install::Game {
        dir,
        user_reg: Some(pfx.join("user.reg")),
        identity: swofty_core::game_folder::baseline::Identity {
            build_id: "1".into(),
            exe_sha256: SHA.into(),
        },
    };
    (game, profile)
}

/// The core's binding set for the EA build, as bindgen writes it.
fn ea_bindings(loader: &Path, ids: &[&str]) {
    let core = loader.join("swofty/addons/core");
    std::fs::create_dir_all(&core).unwrap();
    let b: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| serde_json::json!({"id": id, "kind": "func", "status": "experimental"}))
        .collect();
    let doc =
        serde_json::json!({"schema": 1, "build": {"sha256": SHA, "distribution": "EA App"}, "bindings": b});
    std::fs::write(
        core.join("bindings.ea-1-0-145.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
}

/// A fixture captured from an EA run's title folder.
fn ea_fixture_at(tests_dir: &Path, name: &str) {
    let title = tests_dir.join("captured-ea");
    std::fs::create_dir_all(title.join("profile")).unwrap();
    std::fs::write(title.join("profile/NXDSMWW.BWW"), [7u8; 900]).unwrap();
    std::fs::write(title.join("profile/QKRHMYXE"), [8u8; 416]).unwrap();
    let meta = Meta {
        exe: fixture::Exe {
            sha256: SHA.into(),
            version: None,
        },
        created: "now".into(),
        ..Default::default()
    };
    fixture::capture_ea(name, &tests_dir.join("fixtures"), &title, &[], meta).unwrap();
}

fn armed(run: &sandbox::RunDir) {
    std::fs::write(run.root.join(sandbox::STATUS_FILE), r#"{"state":"armed"}"#).unwrap();
}

#[test]
fn an_ea_career_batch_runs_in_the_ea_title_folder_and_checks_the_prefix_save_folder() {
    let root = temp("sandbox-ea");
    let (data, out) = (root.join("data"), root.join("out"));
    let (game, live_profile) = fake_ea_copy(&root);
    assert!(sandbox::ea_copy(&game.dir));
    let live = sandbox::live_saves(&game).unwrap();
    assert_eq!(
        live, live_profile,
        "an EA copy's live save folder is its prefix's profile folder"
    );

    let tests_dir = root.join("addon/tests");
    std::fs::create_dir_all(tests_dir.join("native")).unwrap();
    std::fs::write(tests_dir.join("native/tests.dll"), b"MZ").unwrap();
    ea_fixture_at(&tests_dir, "ea-r3");
    let s = suite::parse(EA_SUITE, &tests_dir.join("c.test.json"), None).unwrap();
    let suites = vec![s];
    let plan = plan::plan(&suites, &Selector::default());
    let batches: Vec<&Batch> = plan.batches.iter().collect();
    let fixtures = fixture::store(&data);
    let loader = root.join("loader");
    ea_bindings(&loader, sandbox::BINDINGS);
    let steam_set = check_sandbox(Some(&loader), SHA, &batches, &bases(&suites), &fixtures).unwrap_err();
    assert!(steam_set.contains("lack vt_Ne_SaveSystem2Win"), "{steam_set}");
    ea_bindings(&loader, sandbox::EA_BINDINGS);
    let waits = check_sandbox(Some(&loader), SHA, &batches, &bases(&suites), &fixtures).unwrap_err();
    assert!(
        waits.contains("has had no in-game run yet"),
        "until the in-game run, the preflight refuses EA career runs: {waits}"
    );

    let store = Store::new(&data);
    let mut fake = Fake {
        scripts: Vec::new(),
        root: root.clone(),
        game: game.dir.clone(),
        launches: Vec::new(),
        envs: Vec::new(),
    };
    let h = Harness {
        store: &store,
        game: &game.dir,
        out: &out,
        profile: "test-ea9",
        launcher: &mut fake,
        times: quick(),
        run_id: "ea9",
        bases: bases(&suites),
        exe_sha: Some(SHA.into()),
        live_saves: Some(live.clone()),
    };
    let b = &plan.batches[0];

    let a = h.open_run(b, 1, &out, true).unwrap();
    let f = batch_fixture(b, &h.bases, &fixtures).unwrap();
    let sb = h.open_sandbox(f, b, "test-ea9-1", &a.run).unwrap();
    assert_eq!(
        std::fs::read(a.run.ea_profile().join("NXDSMWW.BWW")).unwrap(),
        [7u8; 900],
        "the fixture's saves are in the run's EA title folder"
    );
    assert_eq!(
        std::fs::read(a.run.ea_profile().join("QKRHMYXE")).unwrap(),
        [8u8; 416]
    );
    assert!(
        std::fs::read_dir(a.run.remote()).unwrap().next().is_none(),
        "nothing goes into the Steam sandbox on an EA copy"
    );
    armed(&a.run);
    std::fs::write(a.run.ea_profile().join("NXDSMWW.BWW"), [9u8; 900]).unwrap();
    std::fs::create_dir_all(a.run.ea_title().join("midsessions0")).unwrap();
    std::fs::write(a.run.ea_title().join("midsessions0/QKRHMYXE"), [3u8; 32]).unwrap();
    let mut br = Harness::blank(b, 1).0;
    br.started_game = true;
    let mut reports = Harness::blank(b, 1).1;
    reports[0].status = Status::Pass;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-ea9-1",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    h.close_run(Some(a), &mut br);
    assert_eq!(
        br.error, None,
        "an armed sandbox and an unchanged EA save folder pass"
    );
    assert_eq!(reports[0].status, Status::Pass);
    assert_eq!(std::fs::read(live.join("NXDSMWW.BWW")).unwrap(), [1u8; 900]);

    let captured = root.join("captured");
    let cli = crate::Cli::try_parse_from([
        "swofty",
        "test",
        "fixture",
        "capture",
        "ea-hq",
        "--run",
        "ea9-1",
        "--store",
        captured.to_str().unwrap(),
    ])
    .unwrap();
    let crate::Command::Test(TestCommand::Fixture(FixtureCommand::Capture(c))) = cli.command else {
        panic!("not a capture");
    };
    assert_eq!(capture_command(&store, c).unwrap(), ExitCode::SUCCESS);
    let f = fixture::load(&captured.join("ea-hq")).unwrap();
    assert_eq!(f.manifest.layout, Layout::Ea);
    assert_eq!(f.manifest.exe.sha256, SHA);
    assert_eq!(f.manifest.source, "test run ea9-1");
    let files: Vec<&str> = f.manifest.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        [
            "save/savegame@midsessions0#QKRHMYXE",
            "save/savegame@profile#NXDSMWW.BWW",
            "save/savegame@profile#QKRHMYXE",
        ],
        "a capture from an EA run takes the saves the game wrote into its title folder"
    );
    assert_eq!(
        std::fs::read(f.dir.join("save/savegame@profile#NXDSMWW.BWW")).unwrap(),
        [9u8; 900]
    );

    let a = h.open_run(b, 2, &out, true).unwrap();
    let sb = h.open_sandbox(f, b, "test-ea9-2", &a.run).unwrap();
    assert_eq!(
        std::fs::read(a.run.ea_profile().join("NXDSMWW.BWW")).unwrap(),
        [9u8; 900],
        "the captured fixture goes back into the next EA run"
    );
    assert!(a.run.ea_title().join("midsessions0/QKRHMYXE").is_file());
    armed(&a.run);
    std::fs::write(live.join("QKRHMYXE"), [2u8; 417]).unwrap();
    let mut br = Harness::blank(b, 2).0;
    br.started_game = true;
    let mut reports = Harness::blank(b, 2).1;
    reports[0].status = Status::Pass;
    h.close_sandbox(
        sb,
        Some(&a.run),
        b,
        "test-ea9-2",
        BatchResults {
            br: &mut br,
            reports: &mut reports,
            runnable: &[0],
        },
    );
    h.close_run(Some(a), &mut br);
    let why = br.error.unwrap();
    assert!(
        why.starts_with("THE PLAYER'S EA SAVE FOLDER CHANGED")
            && why.contains(&live.display().to_string())
            && why.contains("QKRHMYXE changed"),
        "{why}"
    );
    assert_eq!(reports[0].status, Status::Error);

    fixture_at(&tests_dir, "steam-r3", SHA);
    let steam = fixture::load(&tests_dir.join("fixtures/steam-r3")).unwrap();
    let a = h.open_run(b, 3, &out, true).unwrap();
    let Err(refused) = h.open_sandbox(steam, b, "test-ea9-3", &a.run) else {
        panic!("a Steam fixture went into an EA run");
    };
    assert!(
        refused.contains("holds Steam saves") && refused.contains("takes EA app saves"),
        "{refused}"
    );
    let mut br = Harness::blank(b, 3).0;
    h.close_run(Some(a), &mut br);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_capture_from_a_copy_of_an_ea_save_folder_is_an_ea_fixture() {
    let root = temp("capture-ea-from");
    let store = Store::new(root.join("data"));
    let copy = root.join("backup/savegame");
    std::fs::create_dir_all(copy.join("ea/profile")).unwrap();
    std::fs::create_dir_all(copy.join("ea/highlights000")).unwrap();
    std::fs::write(copy.join("ea/profile/NXDSMWW.BWW"), [4u8; 900]).unwrap();
    std::fs::write(copy.join("ea/profile/QKRHMYXE"), [5u8; 416]).unwrap();
    std::fs::write(copy.join("ea/highlights000/QKRHMYXE"), [6u8; 416]).unwrap();
    let fixtures = root.join("fixtures");
    let cli = crate::Cli::try_parse_from([
        "swofty",
        "test",
        "fixture",
        "capture",
        "ea-backup",
        "--from",
        copy.to_str().unwrap(),
        "--store",
        fixtures.to_str().unwrap(),
    ])
    .unwrap();
    let crate::Command::Test(TestCommand::Fixture(FixtureCommand::Capture(c))) = cli.command else {
        panic!("not a capture");
    };
    assert_eq!(capture_command(&store, c).unwrap(), ExitCode::SUCCESS);
    let f = fixture::load(&fixtures.join("ea-backup")).unwrap();
    assert_eq!(f.manifest.layout, Layout::Ea);
    let files: Vec<&str> = f.manifest.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        [
            "save/savegame@profile#NXDSMWW.BWW",
            "save/savegame@profile#QKRHMYXE"
        ]
    );
    assert_eq!(f.manifest.source, format!("folder {}", copy.display()));
    std::fs::remove_dir_all(&root).ok();
}
