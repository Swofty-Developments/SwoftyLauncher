use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use swofty_core::store::Store;

pub fn restore(store: &Store, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = super::game(store, game)?;
    let done = swofty_core::game_folder::recovery::restore(&store.root, &g)?;
    for n in &done.notes {
        println!("restored: {n}");
    }
    for p in &done.set_aside {
        println!("set aside: {}", p.display());
    }
    let left = done.left;
    if left.is_empty() {
        println!(
            "restored: the game folder matches the baseline for build {}",
            g.identity.build_id
        );
        return Ok(ExitCode::SUCCESS);
    }
    for d in &left {
        println!("not restored: {d}");
    }
    Ok(ExitCode::FAILURE)
}
