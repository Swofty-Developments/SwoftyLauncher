use std::process::ExitCode;

use anyhow::Result;
use swofty_core::platform::steam;
use swofty_core::store::Store;

use crate::commands::{Execute, Invocation};
use crate::console::output::say;
use crate::stores::steam::bring_up_steam;
use clap::Args;

#[derive(Args)]
pub(crate) struct SteamArgs {
    #[arg(
        long,
        help = "Start Steam if it's closed and wait up to 90 s for it to sign in"
    )]
    pub(crate) start: bool,
}

impl Execute for SteamArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        check(&invocation.store, self.start)
    }
}

/// `swofty steam`: what the launch sees, and with `start`, the same start
/// and wait as Play, without launching the game.
fn check(store: &Store, start: bool) -> Result<ExitCode> {
    if start {
        bring_up_steam(store, None)?;
        return Ok(ExitCode::SUCCESS);
    }
    let status = steam::client::status();
    say("steam", &format!("detected: {status}"));
    if let Some(g) = steam::find_game() {
        say(
            "steam",
            &match steam::busy(&g) {
                Some(state) => format!("F1 25: Steam is {state} its files"),
                None => "F1 25: Steam isn't changing its files".to_owned(),
            },
        );
    }
    Ok(if status.state.usable() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
