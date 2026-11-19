use std::path::Path;

use anyhow::Result;
use swofty_core::game_folder::baseline::DepotHashes;
use swofty_core::game_folder::stock::{self, Depots};
use swofty_core::install::Game;
use swofty_core::platform::steam;
use swofty_core::store::Store;

/// Refuse a modded build or session on a copy no store checks the
/// ownership of, and on a Steam copy whose game or Steam API files are not
/// the ones Steam installed. Steam's list is only read here; a copy whose
/// list can't be read is checked without it.
pub fn check_entitled(store: &Store, g: &Game) -> Result<()> {
    let depots = steam::find_game()
        .filter(|i| i.dir == g.dir)
        .and_then(|i| Depots::load(&i).ok());
    swofty_core::game_folder::entitlement::check(&g.dir, depots.as_ref())?;
    let Some(depots) = depots else {
        return Ok(());
    };
    let changed = swofty_core::game_folder::entitlement::changed_files(&store.root, &g.dir, &depots)?;
    if !changed.is_empty() {
        stock::Report::new(changed.clone(), &g.dir).write(&store.root)?;
        return Err(swofty_core::Error::NotStock(changed).into());
    }
    Ok(())
}

/// Check the game against Steam's manifests before anything is built from
/// it. Steam's list stays with the install, which checks every original it
/// backs up for the first time. A game folder outside Steam is not checked;
/// a Steam install whose list can't be read says why.
pub(super) fn check_stock(store: &Store, g: &Game, out: &dyn Fn(&str)) -> Result<Option<Depots>> {
    let Some(found) = steam::find_game().filter(|i| i.dir == g.dir) else {
        return Ok(None);
    };
    let depots = match Depots::load(&found) {
        Ok(d) => d,
        Err(why) => {
            out(&format!(
                "[stock] {why}; the game files are not checked against Steam"
            ));
            heal_launcher(store, &g.dir, None, out)?;
            return Ok(None);
        }
    };
    if !depots.names_known() {
        out(&format!(
            "[stock] Steam's manifests have encrypted file names; {} game file(s) are checked by the hash of their path, \
             the rest by their size and SHA-1",
            depots.hashed_len()
        ));
    }
    heal_launcher(store, &g.dir, Some(&depots as &dyn DepotHashes), out)?;
    let checked = stock::check(&store.root, g, &depots)?;
    if !checked.missing.is_empty() {
        let version = swofty_core::game_folder::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
        stock::Report::missing(checked.missing.clone(), &g.dir, version.clone())
            .write(&store.root)
            .ok();
        return Err(swofty_core::Error::MissingStock {
            files: checked.missing,
            version,
        }
        .into());
    }
    if checked.proven > 0 {
        out(&format!(
            "[stock] {} original(s) on record match Steam's copy",
            checked.proven
        ));
    }
    for rel in &checked.rerecorded {
        out(&format!(
            "[stock] {rel} matches Steam's copy again; backed up as the original"
        ));
    }
    for d in &checked.eac {
        out(&format!(
            "[stock] {} differs from Steam's copy (size {}, Steam {}); not used by modded sessions, continuing",
            d.rel,
            d.size.map_or("missing".to_owned(), |n| n.to_string()),
            d.steam
        ));
    }
    if !checked.third_party.is_empty() {
        let dlls: Vec<String> = checked
            .third_party
            .iter()
            .map(|d| {
                format!(
                    "{} (size {}, Steam {})",
                    d.rel,
                    d.size.map_or("missing".to_owned(), |n| n.to_string()),
                    d.steam
                )
            })
            .collect();
        out(&format!(
            "[stock] {} DLL file(s) differ from Steam's copy and are left as third-party files, continuing: {}",
            dlls.len(),
            dlls.join(", ")
        ));
    }
    if let Some(n) = checked.swept {
        out(&format!("[stock] {n} game file(s) checked against Steam's list"));
    }
    if !checked.foreign.is_empty() {
        for f in &checked.detail {
            out(&format!(
                "[stock] {} is not Steam's copy (measured {}, size {}, Steam {}, modified {})",
                f.rel,
                f.measured,
                f.size.map_or("missing".to_owned(), |n| n.to_string()),
                f.steam,
                f.modified.map_or("unknown".to_owned(), |t| format!("{t:.0}"))
            ));
        }
        let (changed, missing) = stock::split_absent(&checked.detail);
        let version = swofty_core::game_folder::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
        if changed.is_empty() && !missing.is_empty() {
            stock::Report::missing(missing.clone(), &g.dir, version.clone())
                .with_detail(checked.detail.clone())
                .write(&store.root)?;
            return Err(swofty_core::Error::MissingStock {
                files: missing,
                version,
            }
            .into());
        }
        stock::Report::new(checked.foreign.clone(), &g.dir)
            .with_detail(checked.detail.clone())
            .write(&store.root)?;
        if missing.is_empty() {
            return Err(swofty_core::Error::NotStock(checked.foreign).into());
        }
        return Err(swofty_core::Error::NotStockAndMissing {
            changed,
            missing,
            version,
        }
        .into());
    }
    Ok(Some(depots))
}

/// Clear a stale EA AntiCheat backup an interrupted session left before the
/// stock check runs, logging what it did. Does nothing while a session is
/// pending.
fn heal_launcher(
    store: &Store,
    game: &Path,
    depot: Option<&dyn DepotHashes>,
    out: &dyn Fn(&str),
) -> Result<()> {
    for note in swofty_core::offline::heal_before_check(&store.root, game, depot)? {
        out(&format!("[stock] {note}"));
    }
    Ok(())
}

/// An original that turned out not to be Steam's while the install backed it
/// up, or one missing from the game folder with no backup, stops the launch
/// the same way as the check before the build.
pub(super) fn not_stock(store: &Store, g: &Game, r: swofty_core::Result<()>) -> swofty_core::Result<()> {
    match &r {
        Err(swofty_core::Error::NotStock(files)) => {
            stock::Report::new(files.clone(), &g.dir).write(&store.root).ok();
        }
        Err(swofty_core::Error::MissingStock { files, version }) => {
            stock::Report::missing(files.clone(), &g.dir, version.clone())
                .write(&store.root)
                .ok();
        }
        _ => {}
    }
    r
}
