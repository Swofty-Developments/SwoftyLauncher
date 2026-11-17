use std::path::Path;
use std::process::{Child, Command};

use anyhow::Result;
use swofty_core::app_control::Block;
use swofty_core::launch;
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
