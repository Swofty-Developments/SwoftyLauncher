mod failure;
mod follow;
mod launch;
mod wrapper;
mod wrapper_process;

#[cfg(all(test, unix))]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::{Execute, Invocation};
use anyhow::Result;
use clap::Args;
use swofty_core::session::DEFAULT_WINEDEBUG;
use swofty_core::store::Store;

impl Execute for RunArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        run(&invocation.store, &invocation.profile, self)
    }
}

pub(crate) use follow::{Follow, symbols};

#[derive(Args)]
pub struct RunArgs {
    #[arg(long, default_value = DEFAULT_WINEDEBUG, help = "WINEDEBUG channels for the game process")]
    winedebug: String,
    #[arg(long, help = "Close this copy's running F1 25 first")]
    restart: bool,
    #[arg(
        long,
        help = "Launch a Steam copy through Proton directly instead of asking Steam (an EA app copy always starts through the EA app)"
    )]
    direct: bool,
    #[arg(
        long,
        alias = "no-start-steam",
        help = "Don't start Steam or the EA app when it's closed (the launch.start_store setting does the same)"
    )]
    no_start_store: bool,
    #[arg(long, hide = true)]
    via_ea: bool,
    #[arg(
        long,
        help = "Play this one session on Steam's saves, though the profile uses launcher saves"
    )]
    steam_saves_once: bool,
    #[arg(
        long,
        help = "Start F1 25 in VR, like Steam's VR launch option (passes -vr; on Linux, starts SteamVR first)"
    )]
    vr: bool,
    #[arg(
        long,
        help = "Build and install the profile (database, tracks) before the game starts, and roll it back when it exits"
    )]
    apply: bool,
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    game: Option<PathBuf>,
    #[arg(
        long = "game-arg",
        value_name = "ARG",
        allow_hyphen_values = true,
        requires = "direct",
        help = "Pass ARG to F1 25 in a direct launch; repeat it for more, such as --game-arg=-benchmark --game-arg=x.xml"
    )]
    game_arg: Vec<String>,
    #[arg(
        long,
        env = "SWOFTY_EXE_COPY",
        help = "A copy of F1_25.exe whose .pdata gives function bounds"
    )]
    exe: Option<PathBuf>,
    #[arg(
        last = true,
        help = "The Steam command (%command%); present when Steam runs the wrapper"
    )]
    steam_command: Vec<String>,
}

pub fn run(store: &Store, profile: &str, args: RunArgs) -> Result<ExitCode> {
    let wrapped = !args.steam_command.is_empty();
    let explicit = args
        .game
        .clone()
        .or_else(|| wrapped.then(|| std::env::current_dir().ok()).flatten());
    let g = crate::commands::build::game(store, explicit.clone())?;
    if wrapped {
        return wrapper::run(store, profile, &args, &g)
            .or_else(|e| {
                again_if_updated(store, e, || {
                    let g = crate::commands::build::game(store, explicit.clone())?;
                    wrapper::run(store, profile, &args, &g)
                })
            })
            .inspect_err(|e| failure::note_blocked(store, e));
    }
    launch::run(store, profile, &args, &g)
}

/// A session the EA app updated the game under ends rolled back; it runs once
/// more on the game as the update left it, which builds the profile for the
/// new version. An update again on that start ends the session.
fn again_if_updated(
    store: &Store,
    e: anyhow::Error,
    run: impl FnOnce() -> Result<ExitCode>,
) -> Result<ExitCode> {
    let Some(updated) = e.downcast_ref::<crate::stores::ea::Updated>() else {
        return Err(e);
    };
    crate::console::output::say(
        "info",
        &format!("{updated}: building the profile for the new version and starting F1 25 again"),
    );
    let stage =
        crate::stores::steam::StageFile(store.root.join(swofty_core::platform::steam::client::STAGE_FILE));
    stage.set("The EA app updated F1 25: building the profile for the new version…");
    run().map_err(|e| match e.downcast_ref::<crate::stores::ea::Updated>() {
        Some(again) => anyhow::anyhow!(
            "{again} again, so the modded session was not started; wait until the EA app shows F1 25 as up to \
             date, then press Play again"
        ),
        None => e,
    })
}
