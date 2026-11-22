use std::path::Path;

use swofty_core::diagnostics::launch;
use swofty_core::platform::app_control::Block;
use swofty_core::store::Store;

use super::import_retry::{IMPORT_RETRY_SECS, RetryGate, should_retry_import};
use super::run::{cause_first, crash, failure_line, launch_failure, not_started, run_args};
use swofty_core::session::LaunchMode;

fn idle_steam() -> RetryGate {
    RetryGate {
        game_running: false,
        pending: true,
        launch_in_progress: false,
        steam_usable: true,
        ea_copy: false,
        now: 1000,
        last: 0,
    }
}

#[test]
fn a_steam_copy_with_nothing_running_retries_the_import() {
    assert!(should_retry_import(idle_steam()));
}

#[test]
fn an_ea_copy_never_retries_the_import() {
    assert!(!should_retry_import(RetryGate {
        ea_copy: true,
        ..idle_steam()
    }));
}

#[test]
fn a_launch_in_progress_never_retries_the_import() {
    assert!(!should_retry_import(RetryGate {
        launch_in_progress: true,
        ..idle_steam()
    }));
    assert!(!should_retry_import(RetryGate {
        game_running: true,
        ..idle_steam()
    }));
}

#[test]
fn the_import_retry_needs_a_usable_steam_and_a_pending_import() {
    assert!(!should_retry_import(RetryGate {
        steam_usable: false,
        ..idle_steam()
    }));
    assert!(!should_retry_import(RetryGate {
        pending: false,
        ..idle_steam()
    }));
}

#[test]
fn the_import_retries_are_spaced() {
    let g = idle_steam();
    assert!(!should_retry_import(RetryGate {
        last: g.now - IMPORT_RETRY_SECS + 1,
        ..g
    }));
    assert!(should_retry_import(RetryGate {
        last: g.now - IMPORT_RETRY_SECS,
        ..g
    }));
}

#[test]
fn a_cli_that_does_not_start_says_so_in_the_launch_log() {
    let root = std::env::temp_dir().join(format!("swofty-launcher-not-started-{}", std::process::id()));
    std::fs::create_dir_all(root.join("state")).unwrap();
    let store = Store::new(&root);
    let log = root.join("state/launcher-run.log");
    let cli = Path::new(r"C:\Swofty\bin\swofty.exe");
    let missing = std::io::Error::from_raw_os_error(2);
    assert_eq!(not_started(&store, &log, cli, &missing), missing.to_string());
    let written = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        written,
        format!("swofty-launcher: could not start {}: {missing}\n", cli.display())
    );
    assert_eq!(launch::Failure::read(&root), None);
    let blocked = std::io::Error::from_raw_os_error(4551);
    let text = not_started(&store, &log, cli, &blocked);
    let written = std::fs::read_to_string(&log).unwrap();
    match Block::of(cli, &blocked) {
        Some(b) => {
            assert_eq!(text, b.text());
            assert!(
                written.ends_with(&format!("swofty-launcher: {}\n", b.headline())),
                "{written}"
            );
            assert_eq!(
                launch::Failure::read(&root).map(|f| f.cause),
                Some(launch::Cause::Blocked(b))
            );
        }
        None => assert_eq!(text, blocked.to_string()),
    }
    assert!(written.contains("could not start"), "{written}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_launch_shows_the_first_line_of_the_wrappers_error() {
    let log = "[info] installing profile default\nswofty: 2 UI edit(s) break rules the game's UI loader enforces:\n  a.erp: b.ui: page.kid: on pressed -> missing page.gone\n  a.erp: b.ui: page.kid: id\n";
    assert_eq!(
        failure_line(log).as_deref(),
        Some("2 UI edit(s) break rules the game's UI loader enforces")
    );
    assert_eq!(failure_line("[info] starting\nexit\n").as_deref(), Some("exit"));
    assert_eq!(failure_line("\n"), None);
}

#[test]
fn play_passes_a_profile_that_starts_with_a_dash_as_the_profiles_value() {
    let args = run_args(
        Path::new("/data"),
        "-swofty",
        Path::new("/games/F1 25"),
        LaunchMode::Normal,
        false,
    );
    assert_eq!(
        args,
        [
            "--data-dir=/data",
            "--profile=-swofty",
            "run",
            "--apply",
            "--game=/games/F1 25"
        ]
    );
    let vr = run_args(Path::new("/data"), "grid", Path::new("/g"), LaunchMode::Vr, true);
    assert_eq!(
        vr,
        [
            "--data-dir=/data",
            "--profile=grid",
            "run",
            "--apply",
            "--vr",
            "--steam-saves-once",
            "--game=/g"
        ]
    );
}

#[test]
fn a_command_line_the_cli_refused_shows_claps_error() {
    let log = "error: unexpected argument '-s' found\n\nUsage: swofty.exe [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.\n";
    assert_eq!(
        failure_line(log).as_deref(),
        Some("the swofty command refused its command line: unexpected argument '-s' found")
    );
    let tipped = "swofty: t.x: its command build has the name of a built-in command or another addon's; run it as swofty t.x:build\n\
                  error: unexpected argument '--frobnicate' found\n\n  tip: to pass '--frobnicate' as a value, use '-- --frobnicate'\n\n\
                  Usage: swofty run --apply\n\nFor more information, try '--help'.\n";
    assert_eq!(
        failure_line(tipped).as_deref(),
        Some("the swofty command refused its command line: unexpected argument '--frobnicate' found")
    );
    assert_eq!(
        failure_line("[wrapper] error: x\nswofty: the wrapper exited\n").as_deref(),
        Some("the wrapper exited")
    );
}

#[test]
fn a_launch_that_exits_without_a_word_says_how_it_exited() {
    assert_eq!(
        launch_failure("", None),
        "the launch exited before the game started"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let status = std::process::ExitStatus::from_raw(2 << 8);
        assert_eq!(
            launch_failure("\n", Some(status)),
            format!("the swofty command exited ({status}) before the game started, without saying why")
        );
    }
    assert_eq!(launch_failure("swofty: no room\n", None), "no room");
}

#[test]
fn a_crash_is_an_ntstatus_error_or_a_fatal_signal() {
    assert!(crash(Some(0xC000_0005_u32 as i32), None));
    assert!(crash(Some(0xC000_0409_u32 as i32), None));
    assert!(crash(Some(0xC000_00FD_u32 as i32), None));
    assert!(!crash(Some(0xC000_013A_u32 as i32), None), "Ctrl-C");
    for code in [0, 1, 3, 101, 255] {
        assert!(!crash(Some(code), None), "{code}");
    }
    for signal in [4, 6, 7, 8, 11] {
        assert!(crash(None, Some(signal)), "{signal}");
    }
    for signal in [2, 9, 15] {
        assert!(!crash(None, Some(signal)), "{signal}");
    }
    assert!(!crash(None, None));
}

#[test]
fn a_cli_that_crashed_without_its_error_says_it_crashed() {
    #[cfg(unix)]
    let crashed = {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(11)
    };
    #[cfg(windows)]
    let crashed = {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0xC000_0005)
    };
    let said = format!(
        "the swofty command crashed ({crashed}) before the game started. Reinstall Swofty Launcher with the \
         installer from f1.swofty.net; your addons and settings are kept"
    );
    assert_eq!(launch_failure("", Some(crashed)), said);
    assert_eq!(
        launch_failure(
            "[info] launching F1 25 directly; waiting for the wrapper, output in C:\\Swofty\\state/wrapper-run.log\n",
            Some(crashed)
        ),
        said
    );
    assert_eq!(
        launch_failure(
            "swofty: the wrapper exited (exit code: 1) before the game started: Heavier Damage conflicts with \
             Formula NASCAR. Turn one of them off on the Addons page\n",
            Some(crashed)
        ),
        "the wrapper exited (exit code: 1) before the game started: Heavier Damage conflicts with Formula \
         NASCAR. Turn one of them off on the Addons page",
        "the error the CLI wrote before it crashed"
    );
    #[cfg(windows)]
    assert_eq!(
        said,
        "the swofty command crashed (exit code: 0xc0000005) before the game started. Reinstall Swofty \
         Launcher with the installer from f1.swofty.net; your addons and settings are kept"
    );
}

#[test]
fn a_wrapper_failure_leads_with_its_innermost_cause() {
    let blocked = "the download was blocked on this PC or network, typically by antivirus HTTPS or web \
                   scanning, a VPN or proxy, a network filter or a Wi-Fi login page; turning that off or trying \
                   another network fixes it";
    let log = format!(
        "[info] building profile default\n\
         [wrapper] downloading offline play 0.3.11 from https://x\n\
         swofty: the wrapper exited (exit code: 1) before the game started: downloading the offline play \
         extension: offline play 0.3.11: {blocked}\n\
         (could not download https://x/v0.3.11/f1op_hook.dll: io: received corrupt message of type \
         InvalidContentType)\n"
    );
    let failure = failure_line(&log).unwrap();
    assert!(
        failure.starts_with("the wrapper exited (exit code: 1)"),
        "{failure}"
    );
    assert_eq!(
        cause_first(&failure),
        format!("{blocked} (downloading the offline play extension: offline play 0.3.11)")
    );
    assert_eq!(
        cause_first(
            "the wrapper exited (exit status: 3) before the game started: version.dll already exists in the \
             game folder; run swofty restore"
        ),
        "version.dll already exists in the game folder; run swofty restore"
    );
    assert_eq!(
        cause_first(
            "the wrapper exited (signal: 9 (SIGKILL)) while the game was running: installing profile default: \
             /games/F1 25/swofty/x.erp: Permission denied (os error 13)"
        ),
        "Permission denied (os error 13) (installing profile default: /games/F1 25/swofty/x.erp)"
    );
    for kept in [
        r"the wrapper exited (exit code: 1) before the game started without output; see C:\Swofty\state\wrapper-run.log",
        "the wrapper did not start within 10 min; check the Steam launch option (swofty doctor)",
        "building profile default: reading x: missing",
        "an antivirus removed mod files, so F1 25 was not started",
    ] {
        assert_eq!(cause_first(kept), kept);
    }
}

#[test]
fn an_addon_that_needs_a_newer_game_shows_the_update() {
    let outdated = "Classic Seasons needs F1 25 1.0.145.2472, the latest update. Your F1 25 is 1.0.136.6820. \
                    Update F1 25 in Steam or the EA app, then press Play again.";
    let log = format!(
        "[info] launching F1 25 directly; waiting for the wrapper, output in C:\\Swofty\\state/wrapper-run.log\n\
         [wrapper] [info] installing profile default\n\
         swofty: the wrapper exited (exit code: 1) before the game started: {outdated}\n\
         (applying swofty.classic-seasons: eaid://ui_package/databases/overrides.uidb: \
         overrides.Contents.teams.Contents.mp_team_select_logos.Contents: no entry [Name=vanamersfoort_26])\n"
    );
    let failure = failure_line(&log).unwrap();
    assert_eq!(
        failure,
        format!("the wrapper exited (exit code: 1) before the game started: {outdated}")
    );
    assert_eq!(cause_first(&failure), outdated);
}

#[test]
fn a_row_an_earlier_addon_removed_leads_with_that_addon() {
    let what = "Korea edits that row, but SERPS Ineos Mercedes Concept - Arabic Modder renamed it earlier in the \
                load. Turn SERPS Ineos Mercedes Concept - Arabic Modder off, or move it after Korea";
    let doing = "applying pajko.korea: track_data.game_setup.TrackDataSet[_metadata_=f2_2026]: no row matches, \
                 even after every addon's changes";
    let log = format!(
        "[info] building profile myteamporsche\n\
         swofty: the wrapper exited (exit code: 1) before the game started: {doing}: {what}\n"
    );
    let failure = failure_line(&log).unwrap();
    assert_eq!(cause_first(&failure), format!("{what} ({doing})"));
}
