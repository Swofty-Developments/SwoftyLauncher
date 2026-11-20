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
}
