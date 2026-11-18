use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use swofty_core::GAME_EXE;
use swofty_core::build::GVDB;
use swofty_core::install::Installer;
use swofty_core::store::Store;
use swofty_formats::erp::Erp;
use swofty_formats::gvdb::Gvdb;

const UI_ERPS: &[&str] = &[
    "2025_asset_groups/ui_package/flow_frontend.erp",
    "2025_asset_groups/ui_package/persistent.erp",
    "2025_asset_groups/ui_package/databases.erp",
    "2025_asset_groups/ui_package/screens/fe_home.erp",
];

pub fn verify(store: &Store, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = super::game(store, game)?;
    let drift = Installer::new(&store.root, &g).verify()?;
    let game = g.dir;
    let mut ok = drift.is_empty();
    for d in &drift {
        println!("DRIFT {d}");
    }
    let mut check = |name: &str, result: Result<bool>| match result {
        Ok(true) => println!("ok    {name}"),
        Ok(false) => {
            ok = false;
            println!("FAIL  {name}: round trip differs");
        }
        Err(e) => {
            ok = false;
            println!("FAIL  {name}: {e:#}");
        }
    };
    check(
        GVDB,
        read(&game, GVDB).and_then(|d| Ok(Gvdb::parse(&d)?.to_bytes() == d)),
    );
    for rel in UI_ERPS {
        check(
            rel,
            read(&game, rel).and_then(|d| Ok(Erp::parse(&d)?.to_bytes()? == d)),
        );
    }
    if !game.join(GAME_EXE).is_file() {
        ok = false;
        println!("FAIL  {GAME_EXE} not found in {}", game.display());
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn read(game: &Path, rel: &str) -> Result<Vec<u8>> {
    std::fs::read(game.join(rel)).with_context(|| rel.to_owned())
}
