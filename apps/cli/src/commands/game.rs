use std::path::PathBuf;

use crate::commands::{Execute, Invocation};
use anyhow::{Context, Result};
use clap::Subcommand;
use std::process::ExitCode;
use swofty_core::store::Store;

impl Execute for GameCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, self).map(|()| ExitCode::SUCCESS)
    }
}

#[derive(Subcommand)]
pub(crate) enum GameCommand {
    #[command(about = "List every copy of F1 25 found: store, version, folder, and the active one")]
    List,
    #[command(about = "Play this copy: steam, ea, or a game folder")]
    Use { copy: String },
    #[command(about = "Add a game folder (with F1_25.exe) and play it")]
    Add { path: PathBuf },
    #[command(
        about = "Start the EA app copy through the EA app as installed: no profile, no offline play, EA AntiCheat runs"
    )]
    Launch {
        #[arg(long, help = "Start F1 25 in VR (passes -vr)")]
        vr: bool,
        #[arg(long, help = "Don't start the EA app when it's closed")]
        no_start_store: bool,
        #[arg(long, help = crate::commands::cli::GAME_HELP)]
        game: Option<PathBuf>,
    },
}

pub(crate) fn command(store: &Store, cmd: GameCommand) -> Result<()> {
    use swofty_core::game_folder::copies::{self, Choice};
    let mut config = store.config();
    match cmd {
        GameCommand::List => {
            let found = copies::discover(&config);
            let active = copies::active(&found, &config).map(|c| c.dir.clone());
            if found.is_empty() {
                println!("no copy of F1 25 found; add one with swofty game add <folder>");
            }
            for c in &found {
                let mark = if active.as_ref() == Some(&c.dir) { "*" } else { " " };
                let version = c.version().unwrap_or_else(|| "unknown".into());
                println!(
                    "{mark} {:7} {:14} {}",
                    copies::store_name(c.source),
                    version,
                    c.dir.display()
                );
            }
        }
        GameCommand::Use { copy } => {
            let found = copies::discover(&config);
            let c = Choice::parse(&copy)
                .pick(&found)
                .with_context(|| format!("no copy of F1 25 matches {copy}; see swofty game list"))?;
            let dir = c.dir.clone();
            copies::set_active(&mut config, &dir);
            store.save_config(&config)?;
            println!(
                "playing the {} copy in {}",
                copies::store_name(c.source),
                dir.display()
            );
        }
        GameCommand::Launch {
            vr,
            no_start_store,
            game,
        } => {
            let g = crate::commands::build::game(store, game)?;
            if swofty_core::install::Source::detect(&g.dir) == swofty_core::install::Source::EaApp {
                crate::stores::ea::check_recorded(&g.dir)?;
            }
            crate::stores::ea::launch_installed(store, &g, vr, no_start_store)?;
        }
        GameCommand::Add { path } => {
            let dir = std::path::absolute(&path)?;
            copies::add(&mut config, &dir).map_err(anyhow::Error::msg)?;
            store.save_config(&config)?;
            println!(
                "added {} ({}); it is now the active copy",
                dir.display(),
                copies::store_name(swofty_core::install::Source::detect(&dir))
            );
        }
    }
    Ok(())
}
