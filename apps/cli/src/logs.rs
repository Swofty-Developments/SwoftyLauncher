use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use swofty_core::steam;
use swofty_core::store::Store;

use crate::output::say;
use crate::run::{Follow, symbols};

pub fn summarise(store: &Store, dir: Option<PathBuf>, exe: Option<PathBuf>) -> Result<ExitCode> {
    let dir = match dir {
        Some(d) => d,
        None => store.latest_session().context("no previous session")?,
    };
    let game = steam::find_game().map(|g| g.dir).unwrap_or_default();
    let mut follow = Follow::new(&dir, None, symbols(&game, exe.as_deref()));
    follow.poll();
    follow.summary()?;
    if let Some(crash) = swofty_core::send_logs::newest_crash(store, Some(&game), std::slice::from_ref(&dir))
    {
        say("crash", &format!("crash report {}", crash.report.display()));
        match swofty_core::send_logs::summarise_crash(&crash.report) {
            Ok(lines) => {
                for l in lines {
                    say("crash", &l);
                }
            }
            Err(e) => say("crash", &format!("could not read it: {e}")),
        }
        if let Some(dump) = &crash.dump {
            say(
                "crash",
                &format!("minidump {} (open it in a debugger)", dump.display()),
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}
