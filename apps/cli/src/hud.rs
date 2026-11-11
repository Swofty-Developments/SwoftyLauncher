//! `swofty hud`: check the stock facts the HUD composer relies on, and compose an addon's
//! `hud.element` records by hand.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Subcommand;
use swofty_core::hud::{self, GameStock};
use swofty_core::store::Store;

#[derive(Subcommand)]
pub enum HudCommand {
    #[command(
        about = "Assert the stock dial facts the HUD composer relies on (scenes, quads, template parents, value tags) against the game's original files; exits 1 when one fails"
    )]
    Check,
    #[command(
        about = "Compose the hud.element records of an addon folder's hud file into ready overlay archives, written to a folder"
    )]
    Compose {
        #[arg(help = "The addon's folder")]
        addon: PathBuf,
        #[arg(help = "The hud file inside it, as operations.hud lists it")]
        file: String,
        #[arg(long, help = "The addon's id (default: the folder's name)")]
        id: Option<String>,
        #[arg(long, default_value = "hud-out", help = "Where to write the archives")]
        out: PathBuf,
    },
}

fn stock(store: &Store) -> Result<GameStock> {
    let game = crate::build::game(store, None)?;
    Ok(GameStock::with_baseline(&game.dir, &store.root).unwrap_or_else(|_| GameStock::new(&game.dir)))
}

pub fn command(store: &Store, cmd: HudCommand) -> Result<ExitCode> {
    match cmd {
        HudCommand::Check => {
            let report = hud::check::stock_facts(&stock(store)?);
            for l in &report.ok {
                println!("ok    {l}");
            }
            for l in &report.problems {
                println!("FAIL  {l}");
            }
            println!("{} ok, {} failed", report.ok.len(), report.problems.len());
            Ok(if report.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        HudCommand::Compose { addon, file, id, out } => {
            let id = id.unwrap_or_else(|| {
                addon
                    .file_name()
                    .map_or_else(|| "addon".into(), |n| n.to_string_lossy().into_owned())
            });
            let inputs = hud::inputs_of(&id, &addon, &file).with_context(|| file.clone())?;
            let built = hud::compose(&stock(store)?, &inputs)?;
            built.write(&out)?;
            for l in &built.lines {
                println!("{l}");
            }
            for a in &built.archives {
                println!(
                    "{}: {} bytes, overlaid on {} archives",
                    a.file,
                    a.bytes.len(),
                    a.erps.len()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
