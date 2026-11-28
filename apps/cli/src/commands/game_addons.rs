//! `swofty --game <id> addons ...` for a game that keeps its addons in its own store.

use std::path::Path;

use anyhow::{Result, bail};
use swofty_core::games::{Addons, GameDef};

use crate::commands::addons::AddonsCommand;

pub(crate) fn command(root: &Path, game: &GameDef, shelf: &dyn Addons, cmd: AddonsCommand) -> Result<()> {
    match cmd {
        AddonsCommand::List => print!("{}", list(root, game, shelf)),
        AddonsCommand::Install { path } => {
            let mut failed = None;
            for result in shelf.install(root, &path) {
                match result {
                    Ok(a) => println!("installed {} {} (on)", a.manifest.id, a.manifest.version),
                    Err(e) => {
                        eprintln!("swofty: installing {}: {e}", path.display());
                        failed = Some(e);
                    }
                }
            }
            if let Some(e) = failed {
                bail!("{e}");
            }
        }
        AddonsCommand::Enable { id } => {
            shelf.set_enabled(root, &id, true)?;
            println!("enabled {id}");
        }
        AddonsCommand::Disable { id, .. } => {
            if !shelf.list(root).addons.iter().any(|a| a.manifest.id == id) {
                bail!("no addon named {id} is installed for {}", game.info.name);
            }
            shelf.set_enabled(root, &id, false)?;
            println!("disabled {id}");
        }
        AddonsCommand::Remove { id, .. } => {
            shelf.uninstall(root, &id)?;
            println!("removed {id}");
        }
        AddonsCommand::Verify { .. } => bail!("addons verify is for F1 25 addons"),
    }
    Ok(())
}

pub(crate) fn list(root: &Path, game: &GameDef, shelf: &dyn Addons) -> String {
    let listing = shelf.list(root);
    let mut out = String::new();
    if listing.addons.is_empty() && listing.unreadable.is_empty() {
        out.push_str(&format!(
            "no {} addons installed in {}\n",
            game.info.name,
            shelf.addons_dir(root).display()
        ));
    }
    for a in &listing.addons {
        let m = &a.manifest;
        let state = if a.enabled { "enabled" } else { "disabled" };
        out.push_str(&format!(
            "{:32} {:10} {state:9} {}\n",
            m.id,
            m.version.to_string(),
            m.name
        ));
    }
    for u in &listing.unreadable {
        out.push_str(&format!("{:32} {:10} {:9} {}\n", u.id, "?", "unusable", u.reason));
    }
    out
}
