use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::build::install::install;
use crate::commands::build::supported;
use anyhow::{Result, bail};
use swofty_core::game_folder::busy::Busy;
use swofty_core::game_folder::recovery::{Scope, SessionLock};
use swofty_core::store::Store;

pub fn apply(store: &Store, profile: &str, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = super::game(store, game)?;
    supported(&g)?;
    let out = |l: &str| println!("{l}");
    let Some(lock) = SessionLock::try_acquire(&store.root)? else {
        if let Some(b) = Busy::live(&store.root) {
            bail!("{}", b.text());
        }
        bail!(
            "{}",
            crate::session_lock::held(store, swofty_core::platform::process::game_running())
        );
    };
    crate::session_lock::stale(store, &lock, &g, Scope::All, &out)?;
    install(store, profile, &g, false, &out)?;
    println!(
        "applied {profile} to {}; swofty restore puts the game back",
        g.dir.display()
    );
    Ok(ExitCode::SUCCESS)
}
