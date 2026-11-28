//! Overcooked 2: found on Steam and the Epic Games Launcher, its own addon store under
//! `<data>/games/overcooked_2/`, and Play that starts it with the Swofty Core for Overcooked 2.

pub mod addons;
pub mod core_dir;
pub mod launch;
pub mod manifest;
pub mod session;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use super::detect::ByStores;
use super::{Addons, GameDef, Installed, Launch, Listing, Loader, Started};
use crate::catalog::requires::Event;
use crate::catalog::{self, Addon, Version};
use crate::manifest::AddonManifest;
use crate::{Error, Result};

pub static DEF: GameDef = GameDef {
    info: &swofty_addon::game::OVERCOOKED_2,
    addon_kinds: &[],
    online: false,
    time_trial: false,
    detect: &ByStores,
    build: None,
    loader: Some(&Doorstop),
    launch: Some(&Direct),
    addons: Some(&Shelf),
};

/// The Swofty Core, loaded into the game's Mono runtime by Unity Doorstop.
pub struct Doorstop;

impl Loader for Doorstop {
    fn core_dir(&self) -> Option<PathBuf> {
        core_dir::find().map(|c| c.0)
    }
}

/// Starts the game's exe itself with Doorstop's environment, not through a store.
pub struct Direct;

impl Launch for Direct {
    fn start(&self, root: &Path, game_dir: &Path) -> Result<Started> {
        start(
            root,
            game_dir,
            &launch::Host::here(),
            &crate::updates::release::current_version().to_string(),
        )
    }
}

/// Writes the session, places what the launch needs and spawns the game.
pub fn start(root: &Path, game_dir: &Path, host: &launch::Host, launcher: &str) -> Result<Started> {
    let core = core_dir::require()?;
    launch::build_in(game_dir, host.os)?;
    let store = addons::Store::new(root);
    let written = session::write(&store, game_dir, launcher, catalog::now_secs())?;
    let plan = launch::plan(game_dir, &core, &written.path, host)?;
    let placed = plan.place()?;
    let started = Started {
        child: match plan.spawn(&written.dir.join(session::OUTPUT_FILE)) {
            Ok(child) => child,
            Err(e) => {
                for p in &placed {
                    std::fs::remove_file(p).ok();
                }
                return Err(e);
            }
        },
        session: written.dir,
        exe: plan.exe,
        placed,
    };
    Ok(started)
}

/// The Overcooked 2 addon store as the games seam offers it.
pub struct Shelf;

impl Addons for Shelf {
    fn dir(&self, root: &Path) -> PathBuf {
        addons::Store::new(root).dir
    }

    fn addons_dir(&self, root: &Path) -> PathBuf {
        addons::Store::new(root).addons_dir()
    }

    fn list(&self, root: &Path) -> Listing {
        addons::Store::new(root).listing()
    }

    fn install(&self, root: &Path, src: &Path) -> Vec<Result<Installed>> {
        addons::Store::new(root)
            .install(src)
            .into_iter()
            .map(|r| r.map(|a| addons::installed(&a)))
            .collect()
    }

    fn set_enabled(&self, root: &Path, id: &str, on: bool) -> Result<()> {
        addons::Store::new(root).set_enabled(id, on)
    }

    fn uninstall(&self, root: &Path, id: &str) -> Result<()> {
        addons::Store::new(root).uninstall(id)
    }

    fn feed(&self, root: &Path) -> catalog::Feed {
        catalog::Feed::game(DEF.info.id, addons::Store::new(root).catalog_path())
    }

    fn install_listed(
        &self,
        root: &Path,
        addon: &Addon,
        v: &Version,
        progress: &dyn Fn(Event),
    ) -> Result<Vec<AddonManifest>> {
        if !v.game.is_empty() && v.game != DEF.info.id {
            return Err(Error::Other(format!(
                "the registry lists {} {} for {}, not {}",
                addon.id, v.version, v.game, DEF.info.name
            )));
        }
        addons::Store::new(root).install_listed(root, addon, v, progress)
    }
}
