use swofty_core::session::{
    LOADER_FALLBACK, LOADER_LOG, PERF_FALLBACK, PERF_LOG, SessionEnd, keep_game_logs,
};

use crate::commands::run::tests::{STALE, session_dir};

use super::*;

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
    let perf_fallback = dir.join(PERF_FALLBACK);
    keep_game_logs(&dir, &game, &session, before.as_ref(), launched);
    assert!(!session.join("loader.log").exists());
    assert!(!session.join("perf.log").exists());

    std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    std::fs::write(&fallback, "attached in the data folder\n").unwrap();
    std::fs::write(
        &perf_fallback,
        "Swofty Launcher performance report\nan earlier session\n",
    )
    .unwrap();
    touch(&perf_fallback, launched - Duration::from_secs(600));
    keep_game_logs(&dir, &game, &session, before.as_ref(), launched);
    assert_eq!(
        std::fs::read_to_string(session.join("loader.log")).unwrap(),
        "attached in the data folder\n"
    );
    assert!(!session.join("perf.log").exists());

    std::fs::write(
        &perf_fallback,
        "Swofty Launcher performance report\nin the data folder\n",
    )
    .unwrap();
    touch(&perf_fallback, launched + Duration::from_secs(1));
    keep_game_logs(&dir, &game, &session, before.as_ref(), launched);
    assert_eq!(
        std::fs::read_to_string(session.join("perf.log")).unwrap(),
        "Swofty Launcher performance report\nin the data folder\n"
    );

    std::fs::write(&loader, "fresh\n").unwrap();
    std::fs::write(&perf, "Swofty Launcher performance report\nnew\n").unwrap();
    touch(&perf, launched + Duration::from_secs(1));
    keep_game_logs(&dir, &game, &session, before.as_ref(), launched);
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
    keep_game_logs(&dir, &game, &session, before.as_ref(), launched);
    assert!(session.join("game-logs/crash-20261101-125350.txt").is_file());
    assert!(!fallback.with_file_name("crash-20261101-125350.txt").exists());
    std::fs::remove_dir_all(&dir).ok();
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
