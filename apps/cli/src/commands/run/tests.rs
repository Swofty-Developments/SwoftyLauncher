use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant, SystemTime};

use swofty_core::app_control::Block;
use swofty_core::busy::Busy;
use swofty_core::crash::Symbols;
use swofty_core::install::{Game, Installer, PROXY};
use swofty_core::launch;
use swofty_core::quarantine::{self, Stage};
use swofty_core::recovery::SessionLock;
use swofty_core::session::{
    Config, LOADER_FALLBACK, LOADER_LOG, LaunchMode, LaunchRequest, LogMark, PERF_LOG, STOP_FILE, SessionEnd,
    game_args, parse_steam_command,
};
use swofty_core::steam_client::{self};
use swofty_core::store::Store;
use swofty_core::{GAME_EXE, STEAM_APP_ID};

use super::RunArgs;
use super::failure::{note_blocked, start};
use super::follow::{Follow, Follower};
use super::launch::{refused, settle};
use super::wrapper::loader::{check_loader, loader_missed};
use super::wrapper::play::{keep_game_logs, why_no_code};
use super::wrapper::{
    NETPLAY_REQUEST_LIFE, Session, age_out, finish, gate_unsupported_build, run_loop, written_within,
};
use super::wrapper_process::{
    DIRECT_START_TIMEOUT, STEAM_START_TIMEOUT, Silence, WRAPPER_LOG, WrapperProcess, spawn_logged, tail,
    which, wrapper_command,
};
use crate::signals::{StopPhase, session_stop, stop_phase, stop_requested_at};
use crate::stores::steam::{StageFile, SteamPolicy, steam_launch_args, steam_policy};

#[test]
fn an_unsupported_build_launches_stock_with_nothing_applied() {
    let request = LaunchRequest {
        id: "x".into(),
        apply: true,
        steam_saves_once: true,
        row_ids: Some(PathBuf::from("ids.json")),
        ..LaunchRequest::default()
    };
    let (profile, out) = gate_unsupported_build(Some("1.0.145.2472".into()), "grid".into(), request);
    assert_eq!(profile, swofty_core::profile::VANILLA);
    assert!(!out.apply && out.row_ids.is_none() && !out.steam_saves_once);
    assert_eq!(out.id, "x");
}

#[test]
fn a_supported_build_keeps_its_profile_and_request() {
    let request = LaunchRequest {
        apply: true,
        ..LaunchRequest::default()
    };
    let (profile, out) = gate_unsupported_build(None, "grid".into(), request.clone());
    assert_eq!(profile, "grid");
    assert_eq!(out, request);
}

#[test]
fn an_old_launch_request_loses_its_arming() {
    let path = std::env::temp_dir().join(format!("swofty-launch-age-{}", std::process::id()));
    std::fs::write(&path, "{}").unwrap();
    assert!(written_within(&path, NETPLAY_REQUEST_LIFE));
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    assert!(!written_within(&path, NETPLAY_REQUEST_LIFE));
    assert!(!written_within(
        &path.with_extension("missing"),
        NETPLAY_REQUEST_LIFE
    ));
    std::fs::remove_file(&path).ok();
}

#[test]
fn an_old_launch_request_plays_on_launcher_saves_again() {
    let asked = || LaunchRequest {
        netplay: Some("armed".into()),
        row_ids: Some(PathBuf::from("row_ids.json")),
        steam_saves_once: true,
        ..LaunchRequest::default()
    };
    let mut fresh = asked();
    age_out(&mut fresh, true);
    assert_eq!(fresh, asked());
    let mut old = asked();
    age_out(&mut old, false);
    assert!(!old.steam_saves_once);
    assert_eq!(old.netplay, None);
    assert_eq!(old.row_ids, None);
}

fn temp_log(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("swofty-run-{}-{name}", std::process::id()))
        .join(WRAPPER_LOG)
}

#[test]
fn a_stop_request_closes_the_game_then_gives_up_waiting_for_it() {
    assert_eq!(stop_phase(None, 100.0, false), StopPhase::Running);
    assert_eq!(stop_phase(None, 100.0, true), StopPhase::Closing);
    assert_eq!(stop_phase(Some(90.0), 100.0, false), StopPhase::Closing);
    assert_eq!(stop_phase(Some(80.0), 100.0, false), StopPhase::Closing);
    assert_eq!(stop_phase(Some(79.0), 100.0, false), StopPhase::GiveUp);
    assert_eq!(stop_phase(Some(79.0), 100.0, true), StopPhase::GiveUp);
}

#[test]
fn reads_when_the_launcher_asked_the_session_to_stop() {
    let dir = std::env::temp_dir().join(format!("swofty-stop-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(stop_requested_at(&dir), None);
    assert_eq!(session_stop(&dir), StopPhase::Running);
    std::fs::write(dir.join(STOP_FILE), "12.5\n").unwrap();
    assert_eq!(stop_requested_at(&dir), Some(12.5));
    assert_eq!(session_stop(&dir), StopPhase::GiveUp);
    std::fs::write(dir.join(STOP_FILE), "garbage").unwrap();
    assert_eq!(session_stop(&dir), StopPhase::Closing);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reports_a_wrapper_that_exits_before_the_game_starts() {
    let log = temp_log("early");
    let child = spawn_logged(
            Command::new("sh").args([
                "-c",
                "echo '[info] installing profile Default'; \
                 echo 'swofty: version.dll already exists in the game folder; run swofty restore' >&2; exit 3",
            ]),
            &log,
        )
        .unwrap();
    let mut w = WrapperProcess {
        child: Some(child),
        log: log.clone(),
        timeout: DIRECT_START_TIMEOUT,
    };
    let status = loop {
        if let Some(s) = w.exited() {
            break s;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(3));
    assert_eq!(
        w.failure(status, "before the game started").to_string(),
        "the wrapper exited (exit status: 3) before the game started: \
             version.dll already exists in the game folder; run swofty restore"
    );
    std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
}

#[test]
fn an_error_over_several_lines_leads_with_what_failed() {
    let log = temp_log("lines");
    let child = spawn_logged(
            Command::new("sh").args([
                "-c",
                "echo 'downloading offline play 0.3.11 from https://x'; \
                 echo '[info] rolled the profile'\"'\"'s game files back'; \
                 echo 'swofty: downloading the offline play extension: offline play 0.3.11: the download was \
                 blocked on this PC or network, typically by antivirus HTTPS or web scanning, a VPN or proxy, a \
                 network filter or a Wi-Fi login page; turning that off or trying another network fixes it' >&2; \
                 echo '(could not download https://x/v0.3.11/f1op_hook.dll: io: received corrupt message of type \
                 InvalidContentType)' >&2; \
                 exit 1",
            ]),
            &log,
        )
        .unwrap();
    let mut w = WrapperProcess {
        child: Some(child),
        log: log.clone(),
        timeout: DIRECT_START_TIMEOUT,
    };
    let status = loop {
        if let Some(s) = w.exited() {
            break s;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        w.failure(status, "before the game started").to_string(),
        "the wrapper exited (exit status: 1) before the game started: downloading the offline play extension: \
             offline play 0.3.11: the download was blocked on this PC or network, typically by antivirus HTTPS or \
             web scanning, a VPN or proxy, a network filter or a Wi-Fi login page; turning that off or trying \
             another network fixes it\n\
             (could not download https://x/v0.3.11/f1op_hook.dll: io: received corrupt message of type \
             InvalidContentType)"
    );
    std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
}

#[test]
fn a_blocked_start_is_recorded_for_the_launcher() {
    let root = std::env::temp_dir().join(format!("swofty-run-{}-blocked", std::process::id()));
    let store = Store::new(&root);
    let missing = start(&mut Command::new(root.join("no-such-program"))).unwrap_err();
    assert!(missing.downcast_ref::<std::io::Error>().is_some(), "{missing}");
    note_blocked(&store, &missing);
    assert_eq!(launch::Failure::read(&root), None);
    let block = Block::from_code(&root.join("f1op-probe.exe"), 4551).unwrap();
    let blocked = anyhow::Error::from(swofty_core::Error::Blocked(block.clone()));
    assert_eq!(blocked.to_string(), block.text());
    note_blocked(&store, &blocked.context("staging offline play"));
    assert_eq!(
        launch::Failure::read(&root).map(|f| f.cause),
        Some(launch::Cause::Blocked(block))
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn names_the_log_when_the_wrapper_says_nothing() {
    let log = temp_log("silent");
    let child = spawn_logged(Command::new("sh").args(["-c", "exit 1"]), &log).unwrap();
    let mut w = WrapperProcess {
        child: Some(child),
        log: log.clone(),
        timeout: DIRECT_START_TIMEOUT,
    };
    let status = loop {
        if let Some(s) = w.exited() {
            break s;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let msg = w.failure(status, "before the game started").to_string();
    assert!(
        msg.ends_with(&format!("without output; see {}", log.display())),
        "{msg}"
    );
    std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
}

const STALE: &str = "2026-10-17 21:44:08.228 [  416] swofty loader (version.dll proxy) attached, pid 412\n\
                         2026-10-17 21:45:23.672 [  620] [game] (error) from the previous run\n";

fn session_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-run-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("game/swofty")).unwrap();
    dir
}

#[test]
fn ignores_a_stale_loader_log_and_follows_its_replacement() {
    let dir = session_dir("rotate");
    let log = dir.join("game").join(LOADER_LOG);
    std::fs::write(&log, STALE).unwrap();
    let mut follower = Follower::replacing(log.clone(), LogMark::read(&log));
    assert!(follower.lines().is_empty());
    assert!(follower.lines().is_empty());
    std::fs::rename(&log, log.with_file_name("loader.prev.log")).unwrap();
    std::fs::write(
        &log,
        "2026-10-17 22:12:08.004 [  420] swofty loader (version.dll proxy) attached, pid 416\npar",
    )
    .unwrap();
    assert_eq!(
        follower.lines(),
        ["2026-10-17 22:12:08.004 [  420] swofty loader (version.dll proxy) attached, pid 416"]
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reports_a_game_process_proton_could_not_start() {
    let dir = session_dir("run-process");
    let log = dir.join("game").join(LOADER_LOG);
    std::fs::write(&log, STALE).unwrap();
    let before = LogMark::read(&log);
    std::fs::write(
        dir.join(format!("steam-{STEAM_APP_ID}.log")),
        include_str!("../../../../../packages/core/tests/fixtures/steam-run-process-731.txt"),
    )
    .unwrap();
    std::fs::write(dir.join("exit_code"), "1\n").unwrap();
    let mut follow = Follow::new(&dir, Some((log, before)), Symbols::default());
    follow.poll();
    assert_eq!(follow.last_loader, None);
    let err = follow.summary().unwrap_err().to_string();
    assert!(
        err.starts_with("Proton could not start the game process (CreateProcess error 731). "),
        "{err}"
    );
    assert!(err.ends_with("Nothing from SwoftyMod ran."), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn keeps_the_loader_and_perf_logs_this_session_wrote() {
    let dir = session_dir("keep-logs");
    let game = dir.join("game");
    let session = dir.join("session");
    std::fs::create_dir_all(&session).unwrap();
    let loader = game.join(LOADER_LOG);
    let perf = game.join(PERF_LOG);
    let touch = |path: &Path, at: SystemTime| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    };
    let launched = SystemTime::now();
    std::fs::write(&loader, STALE).unwrap();
    std::fs::write(&perf, "Swofty Launcher performance report\nold\n").unwrap();
    touch(&perf, launched - Duration::from_secs(600));
    let before = LogMark::read(&loader);
    let fallback = dir.join(LOADER_FALLBACK);
    keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
    assert!(!session.join("loader.log").exists());
    assert!(!session.join("perf.log").exists());

    std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    std::fs::write(&fallback, "attached in the data folder\n").unwrap();
    keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
    assert_eq!(
        std::fs::read_to_string(session.join("loader.log")).unwrap(),
        "attached in the data folder\n"
    );

    std::fs::write(&loader, "fresh\n").unwrap();
    std::fs::write(&perf, "Swofty Launcher performance report\nnew\n").unwrap();
    touch(&perf, launched + Duration::from_secs(1));
    keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
    assert_eq!(
        std::fs::read_to_string(session.join("loader.log")).unwrap(),
        "fresh\n"
    );
    assert_eq!(
        std::fs::read_to_string(session.join("perf.log")).unwrap(),
        "Swofty Launcher performance report\nnew\n"
    );

    std::fs::write(
        fallback.with_file_name("crash-20261101-125350.txt"),
        "Swofty crash report\n",
    )
    .unwrap();
    keep_game_logs(&dir, &game, &fallback, &session, before.as_ref(), launched);
    assert!(session.join("game-logs/crash-20261101-125350.txt").is_file());
    assert!(!fallback.with_file_name("crash-20261101-125350.txt").exists());
    std::fs::remove_dir_all(&dir).ok();
}

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
}

#[test]
fn a_vr_launch_reaches_the_game_on_every_path() {
    let game = Path::new("/games/F1 25");
    let proton = Path::new("/steam/compatibilitytools.d/GE");
    let direct = strings(&wrapper_command(Some(proton), game, LaunchMode::Vr, &[]));
    assert_eq!(
        direct,
        [
            "--",
            "/steam/compatibilitytools.d/GE/proton",
            "waitforexitandrun",
            "/games/F1 25/F1_25.exe",
            "-vr"
        ]
    );
    let seen = parse_steam_command(&direct[1..]);
    assert_eq!(seen.game_args(), ["-vr"]);
    assert_eq!(LaunchMode::of(seen.game_args()), LaunchMode::Vr);
    assert_eq!(game_args(seen.game_args(), LaunchMode::Vr), ["-vr"]);

    let flat = strings(&wrapper_command(Some(proton), game, LaunchMode::Normal, &[]));
    assert!(parse_steam_command(&flat[1..]).game_args().is_empty());

    let native = strings(&wrapper_command(None, game, LaunchMode::Vr, &[]));
    assert_eq!(native, ["--", "/games/F1 25/F1_25.exe", "-vr"]);
    assert_eq!(parse_steam_command(&native[1..]).game_args(), ["-vr"]);

    let bench = ["-benchmark".to_owned(), "swofty_test.xml".to_owned()];
    let passed = strings(&wrapper_command(Some(proton), game, LaunchMode::Normal, &bench));
    let seen = parse_steam_command(&passed[1..]);
    assert_eq!(seen.game_args(), bench, "game arguments reach the game");
    assert_eq!(
        game_args(seen.game_args(), LaunchMode::of(seen.game_args())),
        bench
    );

    assert_eq!(steam_launch_args(LaunchMode::Normal), ["-applaunch", "3059520"]);
    assert_eq!(
        steam_launch_args(LaunchMode::Vr),
        ["-applaunch", "3059520", "-vr"]
    );
}

#[test]
fn starts_steam_only_for_a_steam_copy_with_the_setting_on() {
    let root = std::env::temp_dir().join(format!("swofty-run-{}-policy", std::process::id()));
    let game = root.join("lib/steamapps/common/F1 25");
    let other = root.join("Games/F1 25");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        root.join(format!("lib/steamapps/appmanifest_{STEAM_APP_ID}.acf")),
        r#""AppState" { "installdir" "F1 25" }"#,
    )
    .unwrap();
    assert_eq!(steam_policy(true, &game, false), SteamPolicy::Start);
    assert_eq!(steam_policy(false, &game, false), SteamPolicy::Off);
    assert_eq!(steam_policy(true, &game, true), SteamPolicy::SteamSession);
    assert_eq!(steam_policy(true, &other, false), SteamPolicy::NotSteam);
    assert_eq!(steam_policy(false, &other, true), SteamPolicy::NotSteam);
    let stage = StageFile(root.join("data").join(steam_client::STAGE_FILE));
    stage.set(steam_client::Stage::SigningIn.text());
    assert_eq!(
        std::fs::read_to_string(&stage.0).unwrap(),
        "Waiting for Steam to sign in…"
    );
    let path = stage.0.clone();
    drop(stage);
    assert!(!path.exists());
    std::fs::remove_dir_all(&root).ok();
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

#[cfg(unix)]
#[test]
fn a_game_ended_by_a_signal_has_no_exit_code_and_says_why() {
    let status = Command::new("sh").args(["-c", "kill -9 $$"]).status().unwrap();
    assert_eq!(status.code(), None);
    let end = SessionEnd::resolve(status.code(), &why_no_code(status), None, None);
    assert_eq!(
        (end.end, end.exit_code),
        (swofty_core::session::Ending::Unknown, None)
    );
    assert_eq!(
        end.reason,
        "the game's process was ended by signal 9, which leaves no exit code"
    );
}

#[test]
fn the_wait_for_the_wrapper_counts_from_its_last_output() {
    let log = temp_log("silence");
    let data = log.ancestors().nth(2).unwrap().to_path_buf();
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let mut silence = Silence::new();
    std::thread::sleep(Duration::from_millis(30));
    assert!(silence.of(&log, &data) >= Duration::from_millis(30));
    std::fs::write(&log, "[info] installing: 2000 of 7977 file(s) copied\n").unwrap();
    assert!(silence.of(&log, &data) < Duration::from_millis(30));
    std::thread::sleep(Duration::from_millis(30));
    assert!(silence.of(&log, &data) >= Duration::from_millis(30));
    std::fs::remove_dir_all(&data).ok();
}

fn beat(data: &Path, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(Busy::path(data))
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn a_live_build_that_keeps_its_heartbeat_is_not_silence() {
    let log = temp_log("building");
    let data = log.ancestors().nth(2).unwrap().to_path_buf();
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "[info] build: still working after 60 s\n").unwrap();
    let build = Busy {
        pid: std::process::id().wrapping_add(1),
        ..Busy::new("default", vec!["swofty.classic-seasons".into()])
    };
    build.write(&data).unwrap();
    let lock = SessionLock::try_acquire(&data).unwrap().unwrap();
    let mut silence = Silence::new();
    silence.of(&log, &data);
    std::thread::sleep(Duration::from_millis(30));
    beat(&data, 1_000);
    assert!(
        silence.of(&log, &data) < Duration::from_millis(30),
        "the heartbeat moved"
    );
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        silence.of(&log, &data) >= Duration::from_millis(30),
        "a build whose heartbeat stopped is silent"
    );
    drop(lock);
    drop(
        SessionLock::wait(&data, Duration::from_secs(10))
            .unwrap()
            .expect("a forked child still holds the dropped lock"),
    );
    beat(&data, 2_000);
    assert!(
        silence.of(&log, &data) >= Duration::from_millis(30),
        "a build file left by a process that ended is silent"
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn keeps_the_last_lines() {
    let log = temp_log("tail");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "a\n\nb\r\nc\n").unwrap();
    assert_eq!(tail(&log, 2), ["b", "c"]);
    assert_eq!(tail(&log, 9), ["a", "b", "c"]);
    assert!(tail(&log.with_extension("missing"), 3).is_empty());
    std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
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
            identity: swofty_core::baseline::Identity::detect(&dir, Some(1)).unwrap(),
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
            && swofty_core::journal::Journal::open(&self.store.root)
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
    let notice = swofty_core::recovery::Notice::read(&m.store.root).unwrap();
    assert!(notice.lines[0].ends_with("(6 change(s) for profile MODDED) that a session left open"));
}

#[test]
fn waits_for_a_steam_wrapper_to_finish_rolling_back_before_recovering() {
    if !which("flock") {
        return;
    }
    let m = Modded::new("steam");
    let lock = swofty_core::recovery::lock_path(&m.store.root);
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
fn relays_the_wrappers_lines_under_their_own_tags() {
    let log = temp_log("relay");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "old\n").unwrap();
    let mut relay = Follower::from_end(log.clone());
    std::fs::write(
        &log,
        "old\n[info] rolled the profile's game files back\nswofty: oops\n",
    )
    .unwrap();
    assert_eq!(
        relay.lines(),
        ["[info] rolled the profile's game files back", "swofty: oops"]
    );
    std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
}
