use std::path::Path;
use std::process::{Child, Command};

use anyhow::Result;
use swofty_core::diagnostics::launch;
use swofty_core::platform::app_control::Block;
use swofty_core::store::Store;

use crate::console::output::{info, say};

/// Refuse a launch over a running copy, and record why for the launcher.
pub(crate) fn already_running(store: &Store, pid: u32, then: &str) -> anyhow::Error {
    let failure = launch::Failure::new(launch::Cause::AlreadyRunning { pid });
    if let Err(e) = failure.write(&store.root) {
        info(&format!("could not record it for the launcher: {e}"));
    }
    anyhow::anyhow!("F1 25 is already running (pid {pid}); {then}")
}

/// Start `c`; a program Windows refused to start is an error that says what
/// blocked it.
pub(crate) fn start(c: &mut Command) -> Result<Child> {
    c.spawn()
        .map_err(|e| match Block::of(Path::new(c.get_program()), &e) {
            Some(b) => swofty_core::Error::Blocked(b).into(),
            None => e.into(),
        })
}

/// Every error a wrapper Steam started stops with is kept for Send Logs, in the wrapper
/// log and as the launch's failure unless this run already recorded a more
/// specific one (`since` is when the run began).
pub(crate) fn note_failed(store: &Store, e: &anyhow::Error, since: f64, launcher_started: bool) {
    let chain = causes(e);
    crate::console::output::tee_line("error", &chain.join(": "));
    if launcher_started || matches!(e.downcast_ref(), Some(swofty_core::Error::Blocked(_))) {
        return note_blocked(store, e);
    }
    if launch::Failure::read(&store.root).is_some_and(|f| f.at >= since) {
        return;
    }
    let reason: String = chain[0]
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect();
    let failure = launch::Failure::new(launch::Cause::Failed { reason, chain });
    if let Err(e) = failure.write(&store.root) {
        info(&format!("could not record it for the launcher: {e}"));
    }
}

fn causes(e: &anyhow::Error) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in e.chain().map(|c| c.to_string()) {
        if !out.iter().any(|o| o.contains(&c)) {
            out.push(c);
        }
    }
    out
}

/// A program Windows refused to start: the log names it and the code, and
/// the launcher and Send Logs get it as the launch's failure.
pub(crate) fn note_blocked(store: &Store, e: &anyhow::Error) {
    let Some(swofty_core::Error::Blocked(b)) = e.downcast_ref::<swofty_core::Error>() else {
        return;
    };
    say(
        "warning",
        &format!(
            "Windows refused to start {} (os error {})",
            b.program.display(),
            b.code
        ),
    );
    if let Err(e) = launch::Failure::new(launch::Cause::Blocked(b.clone())).write(&store.root) {
        info(&format!("could not record it for the launcher: {e}"));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

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
    fn any_wrapper_error_is_recorded_with_its_chain() {
        let root = std::env::temp_dir().join(format!("swofty-run-{}-failed", std::process::id()));
        let store = Store::new(&root);
        let e = anyhow::anyhow!("access denied").context("staging offline play");
        note_failed(&store, &e, 0.0, false);
        let f = launch::Failure::read(&root).unwrap();
        assert_eq!(
            f.cause,
            launch::Cause::Failed {
                reason: "staging offline play".into(),
                chain: vec!["staging offline play".into(), "access denied".into()],
            }
        );
        assert_eq!(
            f.headline(),
            "the last launch through Steam failed: staging offline play"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_launcher_started_wrapper_records_only_blocked_starts() {
        let root = std::env::temp_dir().join(format!("swofty-run-{}-launcher", std::process::id()));
        let store = Store::new(&root);
        note_failed(
            &store,
            &anyhow::anyhow!("two plugins with the same file name"),
            0.0,
            true,
        );
        assert_eq!(launch::Failure::read(&root), None);
        let block = Block::from_code(&root.join("f1op-probe.exe"), 4551).unwrap();
        let blocked = anyhow::Error::from(swofty_core::Error::Blocked(block.clone()));
        note_failed(&store, &blocked, 0.0, true);
        assert_eq!(
            launch::Failure::read(&root).map(|f| f.cause),
            Some(launch::Cause::Blocked(block))
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_failure_this_run_already_recorded_is_kept() {
        let root = std::env::temp_dir().join(format!("swofty-run-{}-kept", std::process::id()));
        let store = Store::new(&root);
        let running = already_running(&store, 77, "close it");
        note_failed(&store, &running, 0.0, false);
        assert_eq!(
            launch::Failure::read(&root).map(|f| f.cause),
            Some(launch::Cause::AlreadyRunning { pid: 77 })
        );
        note_failed(
            &store,
            &anyhow::anyhow!("later"),
            crate::time::now_secs() + 60.0,
            false,
        );
        assert!(matches!(
            launch::Failure::read(&root).map(|f| f.cause),
            Some(launch::Cause::Failed { .. })
        ));
        std::fs::remove_dir_all(&root).ok();
    }
}
