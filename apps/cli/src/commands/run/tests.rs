use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use swofty_core::GAME_EXE;
use swofty_core::diagnostics::quarantine::{self, Stage};
use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::install::{Game, Installer, PROXY};
use swofty_core::session::{Config, LaunchRequest};
use swofty_core::store::Store;

use super::RunArgs;
use super::follow::Follower;
use super::launch::{refused, settle};
use super::wrapper::loader::{check_loader, loader_missed};
use super::wrapper::{Session, finish, run_loop};
use super::wrapper_process::{
    DIRECT_START_TIMEOUT, STEAM_START_TIMEOUT, WRAPPER_LOG, WrapperProcess, spawn_logged, which,
};

pub(super) fn temp_log(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("swofty-run-{}-{name}", std::process::id()))
        .join(WRAPPER_LOG)
}

pub(super) const STALE: &str = "2026-10-17 21:44:08.228 [  416] swofty loader (version.dll proxy) attached, pid 412\n\
                         2026-10-17 21:45:23.672 [  620] [game] (error) from the previous run\n";

pub(super) fn session_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-run-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("game/swofty")).unwrap();
    dir
}

#[test]
fn opting_out_is_a_flag() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        run: RunArgs,
    }
    assert!(!Cli::parse_from(["swofty"]).run.no_start_store);
    assert!(Cli::parse_from(["swofty", "--no-start-steam"]).run.no_start_store);
    assert!(Cli::parse_from(["swofty", "--no-start-store"]).run.no_start_store);
}

struct Modded {
    root: PathBuf,
    store: Store,
    game: Game,
}

impl Modded {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-run-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        std::fs::create_dir_all(dir.join("game_package")).unwrap();
        std::fs::write(dir.join(GAME_EXE), "exe").unwrap();
        std::fs::write(dir.join("game_package/db.bin"), "db").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/db"), "modded").unwrap();
        std::fs::write(root.join("src/core"), "core").unwrap();
        let game = Game {
            identity: swofty_core::game_folder::baseline::Identity::detect(&dir, Some(1)).unwrap(),
            dir,
            user_reg: None,
        };
        let store = Store::new(root.join("data"));
        let plan = swofty_core::install::Plan {
            profile: "MODDED".into(),
            data: [("game_package/db.bin".to_owned(), root.join("src/db"))].into(),
            loader: [(
                "swofty/addons/core/swofty_core.dll".to_owned(),
                root.join("src/core"),
            )]
            .into(),
            ..Default::default()
        };
        Installer {
            session: true,
            ..Installer::new(&store.root, &game)
        }
        .apply(&plan)
        .unwrap();
        std::fs::write(game.dir.join("swofty/loader.log"), "attached\n").unwrap();
        Self { root, store, game }
    }

    fn vanilla(&self) -> bool {
        std::fs::read_to_string(self.game.dir.join("game_package/db.bin")).unwrap() == "db"
            && !self.game.dir.join("swofty").exists()
            && swofty_core::game_folder::journal::Journal::open(&self.store.root)
                .unwrap()
                .is_empty()
            && Installer::new(&self.store.root, &self.game)
                .verify()
                .unwrap()
                .is_empty()
    }
}

impl Drop for Modded {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn refuses_to_start_the_game_when_an_antivirus_took_a_placed_dll() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        run: RunArgs,
    }
    let m = Modded::new("refused");
    let started = m.root.join("game-started");
    let exe = m.game.dir.join(GAME_EXE);
    std::fs::write(&exe, format!("#!/bin/sh\ntouch '{}'\n", started.display())).unwrap();
    std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let written = Some(Instant::now() - quarantine::RECHECK_AFTER);
    assert_eq!(check_loader(&m.store, &m.game.dir, written), Ok(false));
    assert_eq!(quarantine::Report::read(&m.store.root), None);
    let core = "swofty/addons/core/swofty_core.dll";
    std::fs::remove_file(m.game.dir.join(core)).unwrap();
    let session = Session {
        store: &m.store,
        game: &m.game,
        request_id: "refused".into(),
        _lock: SessionLock::try_acquire(&m.store.root).unwrap().unwrap(),
    };
    let args = Cli::parse_from(["swofty"]).run;
    let request = LaunchRequest::default();
    let result = run_loop(
        &m.store,
        &args,
        &Config::default(),
        &request,
        "MODDED",
        &m.game,
        written,
    );
    assert_eq!(
        finish(session, result).unwrap(),
        ExitCode::from(quarantine::REFUSED_EXIT)
    );
    assert!(!started.exists(), "the game was started");
    assert!(m.store.latest_session().is_none());
    assert!(m.vanilla());
    let report = quarantine::Report::read(&m.store.root).unwrap();
    assert_eq!(report.stage, Stage::BeforeLaunch);
    assert_eq!(report.files, [core]);
    assert_eq!(report.game, m.game.dir);
    assert_eq!(refused(&m.store, report.at - 1.0), Some(report.clone()));
    assert_eq!(refused(&m.store, report.at + 1.0), None);
    loader_missed(&m.store, &m.game.dir, Some(quarantine::Cause::Defender));
    let report = quarantine::Report::read(&m.store.root).unwrap();
    assert!(report.stage == Stage::InGame && report.removed);
    assert_eq!(report.cause, None);
    assert_eq!(refused(&m.store, 0.0), None);
    std::fs::write(m.game.dir.join(PROXY), "proxy").unwrap();
    loader_missed(&m.store, &m.game.dir, Some(quarantine::Cause::Defender));
    let report = quarantine::Report::read(&m.store.root).unwrap();
    assert!(!report.removed && report.cause.as_deref() == Some(&quarantine::Cause::Defender));
    assert!(
        report
            .text()
            .starts_with("F1 25 is running without mods: Microsoft Defender blocked version.dll."),
        "{}",
        report.text()
    );
}

#[test]
fn a_game_windows_starts_only_as_administrator_rolls_back_and_frees_the_lock_at_once() {
    if !swofty_core::platform::process::game_processes().is_empty() {
        return;
    }
    let m = Modded::new("ea-740");
    let installer = m.game.dir.join(swofty_core::platform::ea_app::MARKER);
    std::fs::create_dir_all(installer.parent().unwrap()).unwrap();
    std::fs::write(
        &installer,
        "<contentIDs><contentID>16425845</contentID></contentIDs>",
    )
    .unwrap();
    let log = m.game.dir.join(swofty_core::session::LOADER_LOG);
    let mut config = m.store.config();
    config.launch.ea_launch_command = Some(format!(
        ": {{url}}; printf '%s\\n' '10:06:01.380 [stub] [offline] CreateProcessW failed, error 740 \
         (ERROR_ELEVATION_REQUIRED)' >> '{}'",
        log.display()
    ));
    m.store.save_config(&config).unwrap();
    let session_dir = m.root.join("session");
    std::fs::create_dir_all(&session_dir).unwrap();
    let session = Session {
        store: &m.store,
        game: &m.game,
        request_id: "ea-740".into(),
        _lock: SessionLock::try_acquire(&m.store.root).unwrap().unwrap(),
    };
    let since = Instant::now();
    let result =
        crate::stores::ea::start(&m.store, &m.game.dir, &[], &session_dir, &[]).map(|_| ExitCode::SUCCESS);
    let err = finish(session, result).unwrap_err();
    assert!(since.elapsed() < Duration::from_secs(15), "{:?}", since.elapsed());
    let refused = swofty_core::platform::run_as_admin::game_refused_in(&[], &m.game.dir);
    assert_eq!(err.to_string(), refused.text());
    assert_eq!(
        swofty_core::diagnostics::launch::Failure::read(&m.store.root).map(|f| f.cause),
        Some(swofty_core::diagnostics::launch::Cause::RunAsAdmin(refused))
    );
    assert!(
        SessionLock::try_acquire(&m.store.root).unwrap().is_some(),
        "the session lock is still held"
    );
    assert!(m.vanilla());
    assert!(
        !m.store
            .root
            .join(swofty_core::platform::steam::client::STAGE_FILE)
            .exists()
    );
}

#[test]
fn rolls_back_a_session_whose_wrapper_was_killed_after_the_game_exited() {
    let m = Modded::new("killed");
    assert!(!m.vanilla());
    let log = m.store.root.join(WRAPPER_LOG);
    let child = spawn_logged(
        Command::new("sh").args(["-c", "echo '[info] in game: nothing changed'; kill -9 $$"]),
        &log,
    )
    .unwrap();
    let mut w = WrapperProcess {
        child: Some(child),
        log: log.clone(),
        timeout: DIRECT_START_TIMEOUT,
    };
    let mut relay = Follower::new(log);
    settle(&m.store, &m.game, &mut w, Some(&mut relay), true);
    assert!(m.vanilla());
    assert_eq!(
        std::fs::read_to_string(m.store.root.join("sessions/orphaned-logs/loader.log")).unwrap(),
        "attached\n"
    );
    let notice = swofty_core::game_folder::recovery::Notice::read(&m.store.root).unwrap();
    assert!(notice.lines[0].ends_with("(6 change(s) for profile MODDED) that a session left open"));
}

#[test]
fn waits_for_a_steam_wrapper_to_finish_rolling_back_before_recovering() {
    if !which("flock") {
        return;
    }
    let m = Modded::new("steam");
    let lock = swofty_core::game_folder::recovery::lock_path(&m.store.root);
    let mut holder = Command::new("flock")
        .arg(&lock)
        .args(["sleep", "1"])
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !SessionLock::held_elsewhere(&m.store.root) {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut w = WrapperProcess {
        child: None,
        log: PathBuf::new(),
        timeout: STEAM_START_TIMEOUT,
    };
    settle(&m.store, &m.game, &mut w, None, true);
    assert!(
        holder.try_wait().unwrap().is_some(),
        "settle ran under a live session"
    );
    holder.wait().unwrap();
    assert!(m.vanilla());
}

#[test]
fn a_roll_back_that_left_locked_files_never_says_the_files_are_back() {
    use super::launch::{UNFINISHED, settled};
    use super::wrapper::rolled_back;
    use swofty_core::game_folder::recovery::Recovered;
    let locked = Recovered {
        problems: vec!["offline play staging: offline-play restore could not finish cleanly".into()],
        ..Recovered::default()
    };
    assert_eq!(settled(&locked, true), Some(UNFINISHED));
    assert_eq!(settled(&locked, false), Some(UNFINISHED));
    let partly = Recovered {
        swept: vec!["removed swofty/addons/core/swofty_core.dll".into()],
        ..locked.clone()
    };
    assert!(partly.recovered());
    assert_eq!(settled(&partly, true), Some(UNFINISHED));

    assert_eq!(
        settled(&Recovered::default(), true),
        Some("session over: its game files are rolled back")
    );
    assert_eq!(settled(&Recovered::default(), false), None);
    let recovered = Recovered {
        offline: true,
        ..Recovered::default()
    };
    assert_eq!(settled(&recovered, true), None);

    assert_eq!(rolled_back(false), "rolled the profile's game files back");
    assert_eq!(
        rolled_back(true),
        "rolled the profile's game files back, but not the offline play files above"
    );
}
