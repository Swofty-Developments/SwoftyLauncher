//! `swofty --game overcooked_2 play`: start a game whose Play the launcher does itself, with its Swofty Core.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Args;
use swofty_core::games::overcooked_2::{addons::Store, core_dir, launch, session};
use swofty_core::games::{GameDef, Probe};

use crate::commands::{Execute, Invocation};

#[derive(Args)]
pub(crate) struct PlayArgs {
    #[arg(
        long,
        help = "Game folder (default: the copy the launcher plays, else the first one found)"
    )]
    dir: Option<PathBuf>,
    #[arg(
        long,
        help = "Print the session and the launch without writing or starting anything"
    )]
    dry_run: bool,
    #[arg(long, help = "Wait for the game to close, then print what the core reported")]
    wait: bool,
}

impl Execute for PlayArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        let game = invocation.game;
        let Some(launcher) = game.launch.filter(|_| game.caps().play && game.addons.is_some()) else {
            bail!(
                "{} plays through Steam or the EA app: press Play in the launcher, or use swofty run as the \
                 Steam launch option",
                game.info.name
            );
        };
        let root = &invocation.store.root;
        let dir = match self.dir {
            Some(dir) => dir,
            None => copy(invocation, game)?,
        };
        if self.dry_run {
            return dry_run(root, &dir).map(|()| ExitCode::SUCCESS);
        }
        let mut started = launcher
            .start(root, &dir)
            .with_context(|| format!("starting {}", game.info.name))?;
        println!("started {} (pid {})", game.info.name, started.game_pid());
        println!("session {}", started.session.display());
        if !self.wait {
            return Ok(ExitCode::SUCCESS);
        }
        let status = started.child.wait().context("waiting for the game")?;
        started.finish();
        println!("exited {status}");
        match session::read_status(&started.session) {
            Some(s) => {
                println!(
                    "core {} std {}, loaded: {}",
                    s.core.as_deref().unwrap_or("?"),
                    s.std.as_deref().unwrap_or("?"),
                    s.loaded.join(", ")
                );
                for f in &s.failed {
                    println!("failed {}: {}", f.id, f.error);
                }
            }
            None => println!("the core wrote no status.json"),
        }
        Ok(ExitCode::SUCCESS)
    }
}

/// The copy Play uses: the one the launcher picked, else the first one found.
fn copy(invocation: &Invocation, game: &GameDef) -> Result<PathBuf> {
    let config = invocation.store.config();
    let found = game.find(&Probe::here(&config));
    let picked = config
        .game_copies
        .get(game.info.id)
        .filter(|d| found.iter().any(|f| &f.dir == *d) || d.is_dir());
    picked
        .cloned()
        .or_else(|| found.into_iter().next().map(|f| f.dir))
        .with_context(|| format!("{} was not found; pass --dir <game folder>", game.info.name))
}

fn dry_run(root: &std::path::Path, dir: &std::path::Path) -> Result<()> {
    let core = core_dir::require()?;
    let store = Store::new(root);
    let at = session::next_dir(&store, swofty_core::catalog::now_secs());
    let launcher = swofty_core::updates::release::current_version().to_string();
    let written = session::build(&store, dir, &launcher, &at);
    let plan = launch::plan(dir, &core, &at.join(session::SESSION_FILE), &launch::Host::here())?;
    println!("core {}", core.0.display());
    println!("session {}", at.join(session::SESSION_FILE).display());
    println!("{}", serde_json::to_string_pretty(&written)?);
    println!("cwd {}", plan.cwd.display());
    for (k, v) in &plan.env {
        println!("env {k}={v}");
    }
    for p in &plan.place {
        println!("place {}", p.path().display());
    }
    let args: Vec<String> = std::iter::once(plan.program.display().to_string())
        .chain(plan.args.iter().cloned())
        .collect();
    println!("run {}", args.join(" "));
    Ok(())
}
