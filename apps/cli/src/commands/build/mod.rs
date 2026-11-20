mod apply;
mod conflicts;
mod heartbeat;
mod install;
mod profile;
mod restore;
mod stock_check;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::{Execute, Invocation};
use anyhow::{Context, Result};
use clap::Args;
use swofty_core::install::{EA_APP_EXPERIMENTAL, Game, Source};
use swofty_core::store::Store;

use self::profile::{report, timed_build};

pub use apply::apply;
pub use conflicts::conflicts;
pub use install::install;
pub use profile::with_session_env;
pub use restore::restore;
pub use stock_check::check_entitled;
pub use verify::verify;

#[derive(Args)]
pub(crate) struct BuildArgs {
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
    #[arg(
        long,
        help = "Also list every side-data finding of add-row copies: known lookups, matches to review, matches in other game areas and acknowledged ones"
    )]
    pub(crate) side_data: bool,
}

#[derive(Args)]
pub(crate) struct ConflictsArgs {
    #[arg(long, help = "Build the profile without installing it and show its conflicts")]
    pub(crate) check: bool,
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ApplyArgs {
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    #[arg(long)]
    pub(crate) game: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct RestoreArgs {
    #[arg(long)]
    pub(crate) game: Option<PathBuf>,
}

impl Execute for BuildArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        build(&invocation.store, &invocation.profile, self.game, self.side_data)
    }
}

impl Execute for ConflictsArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        conflicts(&invocation.store, &invocation.profile, self.check, self.game)
    }
}

impl Execute for ApplyArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        apply(&invocation.store, &invocation.profile, self.game)
    }
}

impl Execute for VerifyArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        verify(&invocation.store, self.game)
    }
}

impl Execute for RestoreArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        restore(&invocation.store, self.game)
    }
}

pub fn game(store: &Store, explicit: Option<PathBuf>) -> Result<Game> {
    Ok(Game::locate(&store.root, game_dir(store, explicit)?)?)
}

pub fn supported(g: &Game) -> Result<()> {
    if Source::detect(&g.dir) == Source::EaApp {
        eprintln!("swofty: {}: {EA_APP_EXPERIMENTAL}", g.dir.display());
    }
    Ok(())
}

/// `--game`, else the active copy (`swofty game use`, the launcher's game
/// switcher), else Steam's.
fn game_dir(store: &Store, explicit: Option<PathBuf>) -> Result<PathBuf> {
    explicit
        .or_else(|| swofty_core::game_folder::copies::find_active(&store.config()).map(|c| c.dir))
        .context("F1 25 not found; pass --game, add the folder with swofty game add, or run swofty doctor")
}

pub fn build(store: &Store, profile: &str, game: Option<PathBuf>, side_data: bool) -> Result<ExitCode> {
    let g = self::game(store, game)?;
    let built = timed_build(store, profile, &g, None, &[])?;
    report(profile, &built);
    print_phases();
    if side_data {
        println!(
            "side data of add-row copies ({} finding(s)):",
            built.side_data.len()
        );
        for f in &built.side_data {
            println!("  {f}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Prints the Play path's phase timings and findings recorded since the last call.
pub fn print_phases() {
    for line in swofty_core::diagnostics::phases::take() {
        crate::console::output::say(swofty_core::diagnostics::phases::TAG, &line);
    }
}

#[cfg(test)]
mod tests;
