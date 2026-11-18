pub(crate) mod send;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use swofty_core::platform::steam;
use swofty_core::store::Store;

use crate::commands::run::{Follow, symbols};
use crate::commands::{Execute, Invocation};
use crate::console::output::say;
use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub(crate) enum LogsCommand {
    #[command(
        about = "Send the launcher and game logs to the developers on Discord, with your username and what went wrong"
    )]
    Send(send::SendArgs),
}

#[derive(Args)]
pub(crate) struct LogsArgs {
    #[command(subcommand)]
    pub(crate) action: Option<LogsCommand>,
    pub(crate) dir: Option<PathBuf>,
    #[arg(long, env = "SWOFTY_EXE_COPY")]
    pub(crate) exe: Option<PathBuf>,
}

impl Execute for LogsArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        match self.action {
            Some(LogsCommand::Send(args)) => send::send(&invocation.store, args),
            None => summarise(&invocation.store, self.dir, self.exe),
        }
    }
}

pub fn summarise(store: &Store, dir: Option<PathBuf>, exe: Option<PathBuf>) -> Result<ExitCode> {
    let dir = match dir {
        Some(d) => d,
        None => store.latest_session().context("no previous session")?,
    };
    let game = steam::find_game().map(|g| g.dir).unwrap_or_default();
    let mut follow = Follow::new(&dir, None, symbols(&game, exe.as_deref()));
    follow.poll();
    follow.summary()?;
    if let Some(crash) =
        swofty_core::diagnostics::send_logs::newest_crash(store, Some(&game), std::slice::from_ref(&dir))
    {
        say("crash", &format!("crash report {}", crash.report.display()));
        match swofty_core::diagnostics::send_logs::summarise_crash(&crash.report) {
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
