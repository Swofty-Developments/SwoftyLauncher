#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::time::SystemTime;

use swofty_core::diagnostics::launch;
use swofty_core::platform::ea_app::Handoff;
use swofty_core::platform::run_as_admin as compat;
#[cfg(unix)]
use swofty_core::session::LOADER_LOG;
use swofty_core::session::LogMark;

use super::setup::{handoff_failed, refused_elevation};
#[cfg(unix)]
use super::stub::STUB_SETTLE;
use super::stub::waiting;
use super::stub::written_after;
use super::*;

#[test]
fn the_active_copy_picks_the_store_to_start() {
    assert_eq!(store_start(Source::Steam), StoreStart::Steam);
    assert_eq!(store_start(Source::EaApp), StoreStart::EaApp);
    assert_eq!(store_start(Source::Unknown), StoreStart::Steam);
}

#[test]
fn a_vanilla_session_through_the_ea_app_leaves_the_anti_cheat_launcher_alone() {
    assert!(!stages_offline(true, true));
    assert!(stages_offline(true, false));
    assert!(stages_offline(false, true));
    assert!(stages_offline(false, false));
}

#[test]
fn says_the_ea_app_is_repairing_when_it_took_offline_plays_file_out() {
    for slot in [Slot::Missing, Slot::Replaced] {
        let (line, status) = waiting(Some(slot));
        assert!(
            line.starts_with("the EA app took offline play's file out"),
            "{line}"
        );
        assert_eq!(status, "The EA app is repairing F1 25");
    }
    for slot in [Some(Slot::Staged), Some(Slot::Unknown), None] {
        let (line, status) = waiting(slot);
        assert!(
            line.starts_with("F1 25 did not start from the launch link"),
            "{line}"
        );
        assert_eq!(status, "Press Play on F1 25 in the EA app");
    }
}

#[test]
fn a_genuine_repair_still_waits_for_the_ea_app() {
    let repairing = NoGame::Wait {
        line: "the EA app took offline play's file out to repair F1 25; F1 25 starts without mods after the \
               repair",
        status: "The EA app is repairing F1 25",
    };
    assert_eq!(no_game(Some(Slot::Replaced), || None), repairing);
    assert_eq!(no_game(Some(Slot::Missing), || None), repairing);
    assert_eq!(
        no_game(Some(Slot::Replaced), || panic!("a replaced file is the EA app's")),
        repairing
    );
    let block = offline::stub_removed(Path::new("F1 25"));
    assert_eq!(
        no_game(Some(Slot::Missing), || Some(block.clone())),
        NoGame::Antivirus(block)
    );
    assert!(matches!(
        no_game(Some(Slot::Staged), || None),
        NoGame::Wait {
            status: "Press Play on F1 25 in the EA app",
            ..
        }
    ));
}

#[cfg(unix)]
fn ea_copy(name: &str) -> (PathBuf, PathBuf, Store, PathBuf) {
    let root = std::env::temp_dir().join(format!("swofty-ea-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("F1 25");
    std::fs::create_dir_all(game.join("__Installer")).unwrap();
    std::fs::write(
        game.join(ea_app::MARKER),
        "<contentIDs><contentID>16425845</contentID><contentID>16425845_oa</contentID></contentIDs>",
    )
    .unwrap();
    let store = Store::new(root.join("data"));
    std::fs::create_dir_all(&store.root).unwrap();
    let got = root.join("url.txt");
    let mut config = store.config();
    config.launch.ea_launch_command = Some(format!("printf %s {{url}} > '{}'", got.display()));
    store.save_config(&config).unwrap();
    (root, game, store, got)
}

#[cfg(unix)]
fn stage_journal(store: &Store, game: &Path, sha256: &str) -> PathBuf {
    let journal = offline::journal_path(&store.root);
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let slot = game.join("EAAntiCheat.GameServiceLauncher.exe");
    let line = serde_json::json!({
        "op": "copy", "from": "", "to": slot.display().to_string(), "source": "stub", "sha256": sha256,
    });
    std::fs::write(&journal, format!("{line}\n")).unwrap();
    journal
}

#[cfg(unix)]
#[test]
fn a_stub_gone_seconds_after_staging_stops_before_the_launch_link() {
    let (root, game, store, got) = ea_copy("stub-gone");
    stage_journal(&store, &game, "0123");
    let session = root.join("session");
    std::fs::create_dir_all(&session).unwrap();
    let since = Instant::now();
    let err = start(&store, &game, &[], &session, &[]).err().unwrap();
    assert!(since.elapsed() >= STUB_SETTLE - Duration::from_millis(500));
    let block = offline::stub_removed(&game);
    assert_eq!(err.to_string(), block.text());
    assert!(err.to_string().contains("antivirus"), "{err}");
    assert!(err.to_string().contains(&game.display().to_string()), "{err}");
    assert_eq!(
        launch::Failure::read(&store.root).map(|f| f.cause),
        Some(launch::Cause::Blocked(block))
    );
    assert!(!got.exists(), "the launch link was sent");
    assert!(!session.join(HANDOFF_LOG).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_stub_in_place_after_staging_lets_the_link_go() {
    let (root, game, store, _) = ea_copy("stub-there");
    let slot = game.join("EAAntiCheat.GameServiceLauncher.exe");
    std::fs::write(&slot, "stub").unwrap();
    let sha = swofty_core::game_folder::baseline::sha256_file(&slot).unwrap();
    let journal = stage_journal(&store, &game, &sha);
    std::fs::File::options()
        .write(true)
        .open(&journal)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(30))
        .unwrap();
    let since = Instant::now();
    check_stub(&store, &game).unwrap();
    assert!(since.elapsed() < Duration::from_secs(1));
    assert_eq!(launch::Failure::read(&store.root), None);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn hands_the_link_to_the_configured_command() {
    let root = std::env::temp_dir().join(format!("swofty-ea-handoff-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("F1 25");
    std::fs::create_dir_all(game.join("__Installer")).unwrap();
    std::fs::write(
        game.join(ea_app::MARKER),
        "<contentIDs><contentID>16425845</contentID><contentID>16425845_oa</contentID></contentIDs>",
    )
    .unwrap();
    let store = Store::new(root.join("data"));
    std::fs::create_dir_all(&store.root).unwrap();
    let got = root.join("url.txt");
    let mut config = store.config();
    config.launch.ea_launch_command = Some(format!("printf %s {{url}} > '{}'", got.display()));
    store.save_config(&config).unwrap();
    let url = ea_app::install_url(&game, &["-vr".into()]).unwrap();
    let mut c = handoff(&store).unwrap().command(&url);
    assert!(c.status().unwrap().success());
    assert_eq!(
        std::fs::read_to_string(&got).unwrap(),
        "origin2://game/launch?offerIds=16425845,16425845_oa&autoDownload=1&cmdParams=-vr"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_handoff_windows_refuses_with_740_names_what_to_untick() {
    let root = std::env::temp_dir().join(format!("swofty-ea-740-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(root.join("data"));
    std::fs::create_dir_all(&store.root).unwrap();
    let exe = r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe";
    let h = Handoff::Program(vec![exe.into(), "%1".into()]);
    let c = h.command("origin2://game/launch?offerIds=16425845");
    let err = handoff_failed(&store, &c, &h, &std::io::Error::from_raw_os_error(740)).to_string();
    assert!(
        err.starts_with("EALauncher.exe is set to run as administrator, so Windows won't start it"),
        "{err}"
    );
    assert!(
        err.contains(&format!("Right-click {exe}, open Properties > Compatibility")),
        "{err}"
    );
    assert!(err.contains("\"Change settings for all users\""), "{err}");
    assert!(!err.contains("Swofty Launcher as administrator"), "{err}");
    match launch::Failure::read(&store.root).map(|f| f.cause) {
        Some(launch::Cause::RunAsAdmin(r)) => {
            assert_eq!(r.exes.first(), Some(&std::path::PathBuf::from(exe)));
            assert!(!r.game);
        }
        other => panic!("{other:?}"),
    }
    launch::Failure::clear(&store.root).unwrap();
    let err = handoff_failed(&store, &c, &h, &std::io::Error::from_raw_os_error(2)).to_string();
    assert!(
        err.starts_with(&format!("could not run {h} to hand the launch link")),
        "{err}"
    );
    assert_eq!(launch::Failure::read(&store.root), None);
    assert!(refused_elevation(&store, c.get_program(), &std::io::Error::other("x")).is_none());
    std::fs::remove_dir_all(&root).ok();
}

const REFUSED_LINE: &str = "10:06:01.380 [stub] [offline] CreateProcessW failed, error 740 \
                            (ERROR_ELEVATION_REQUIRED): Windows will only start F1_25.exe as administrator";

#[test]
fn only_the_session_launchers_lines_since_the_link_count() {
    let root = std::env::temp_dir().join(format!("swofty-ea-stub-log-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("loader.log");
    let since = |log: &Path| (LogMark::read(log), std::fs::metadata(log).map_or(0, |m| m.len()));
    assert_eq!(written_after(&log, None, 0), "");
    std::fs::write(&log, format!("{REFUSED_LINE}\n")).unwrap();
    let (mark, from) = since(&log);
    assert!(!compat::stub_needs_elevation(&written_after(
        &log,
        mark.as_ref(),
        from
    )));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&log)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{REFUSED_LINE}\n").as_bytes()))
        .unwrap();
    assert!(compat::stub_needs_elevation(&written_after(
        &log,
        mark.as_ref(),
        from
    )));
    std::fs::write(
        &log,
        "10:07:00.000 [stub] [offline] session launcher invoked, args: <none>\n",
    )
    .unwrap();
    let (mark, from) = since(&log);
    std::fs::write(&log, format!("{REFUSED_LINE}\n")).unwrap();
    assert!(
        compat::stub_needs_elevation(&written_after(&log, mark.as_ref(), from)),
        "a log started again counts from its start"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_session_launcher_windows_refuses_with_740_ends_the_wait_at_once() {
    let (root, game, store, _) = ea_copy("stub-740");
    let log = game.join(LOADER_LOG);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, format!("{REFUSED_LINE} (an earlier session)\n")).unwrap();
    let mut config = store.config();
    config.launch.ea_launch_command = Some(format!(
        ": {{url}}; sleep 0.3; printf '%s\\n' '{REFUSED_LINE}' >> '{}'",
        log.display()
    ));
    store.save_config(&config).unwrap();
    let session = root.join("session");
    std::fs::create_dir_all(&session).unwrap();
    let before: Vec<u32> = game_processes().iter().map(|g| g.pid).collect();
    let since = Instant::now();
    let err = start(&store, &game, &[], &session, &before).err().unwrap();
    assert!(since.elapsed() < Duration::from_secs(10), "{:?}", since.elapsed());
    let refused = compat::game_refused_in(&[], &game);
    assert_eq!(err.to_string(), refused.text());
    assert!(
        err.to_string()
            .contains("running Swofty Launcher as administrator doesn't help"),
        "{err}"
    );
    assert_eq!(
        launch::Failure::read(&store.root).map(|f| f.cause),
        Some(launch::Cause::RunAsAdmin(refused))
    );
    assert!(!store.root.join(steam_client::STAGE_FILE).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_hand_off_whose_program_is_not_found_names_the_setting_and_the_quotes() {
    let status = std::process::Command::new("sh")
        .args(["-c", "/nowhere/Proton - Experimental/proton run x"])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    let hint = handoff_hint(status.code());
    assert!(hint.contains("launch.ea_launch_command"), "{hint}");
    assert!(
        hint.contains("put quotes around a path that has spaces"),
        "{hint}"
    );
    assert!(handoff_hint(Some(126)).contains("could not run"));
    assert_eq!(handoff_hint(Some(1)), "");
    assert_eq!(handoff_hint(None), "");
}
