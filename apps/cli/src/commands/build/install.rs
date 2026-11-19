use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::commands::build::heartbeat::{Copied, heartbeat};
use crate::commands::build::print_phases;
use crate::commands::build::profile::{report, server_plugins, timed_build};
use crate::commands::build::stock_check::{check_entitled, check_stock, not_stock};
use anyhow::{Result, bail};
use swofty_core::GAME_EXE;
use swofty_core::game_folder::baseline::DepotHashes;
use swofty_core::game_folder::busy::{self, Busy};
use swofty_core::game_folder::recovery::Blocked;
use swofty_core::game_folder::stock;
use swofty_core::install::{Game, Installer};
use swofty_core::store::Store;
use swofty_core::updates::managed;

const REPAIR_TIMEOUT: Duration = Duration::from_secs(20);

/// Build the profile and install it. Files an unfinished session left in the
/// way that are provably ours are cleared and the install is tried once
/// more; anything else stops it untouched, with what a restore would do.
pub fn install(store: &Store, profile: &str, g: &Game, session: bool, out: &dyn Fn(&str)) -> Result<()> {
    if swofty_core::platform::run_as_admin::write_blocker(&g.dir).is_some() {
        bail!(
            "{}",
            swofty_core::platform::run_as_admin::write_blocked_text(&g.dir)
        );
    }
    repair_importer(store, profile, out);
    let done = AtomicBool::new(false);
    let copied = Copied::default();
    let progress = |n: usize, total: usize| {
        copied.0.store(n as u64, Ordering::Relaxed);
        copied.1.store(total as u64, Ordering::Relaxed);
    };
    let busy = session.then(|| {
        busy::clear_cancel(&store.root);
        let addons = store
            .profile(profile)
            .map(|p| p.enabled().map(str::to_owned).collect())
            .unwrap_or_default();
        Busy::new(profile, addons)
    });
    let r = std::thread::scope(|s| {
        s.spawn(|| heartbeat(&done, &store.root, busy, &copied));
        let r = install_checked(store, profile, g, session, out, &progress)
            .map_err(|e| named(store, g, e))
            .map_err(|e| denied(e, &store.root, &g.dir))
            .map_err(|e| outdated(store, g, e));
        done.store(true, Ordering::Relaxed);
        r
    });
    print_phases();
    if session {
        Busy::clear(&store.root);
        busy::clear_cancel(&store.root);
    }
    r
}

/// Downloads Simplified for Swofty again when its program for this OS is gone,
/// which an antivirus does, before the build starts it.
fn repair_importer(store: &Store, profile: &str, out: &dyn Fn(&str)) {
    let on = store
        .profile(profile)
        .is_ok_and(|p| p.enabled().any(|id| id == managed::ID));
    let Some(version) = managed::broken(store).filter(|_| on) else {
        return;
    };
    out(&format!(
        "[repair] {} {version} is missing its program; downloading it again",
        managed::NAME
    ));
    let repaired = managed::release_of(&version, REPAIR_TIMEOUT)
        .map_err(|e| e.to_string())
        .and_then(|r| managed::repair(store, &r).map_err(|e| e.to_string()));
    match repaired {
        Ok(v) => out(&format!("[repair] installed {} {v} again", managed::NAME)),
        Err(e) => out(&format!(
            "[repair] could not install {} again: {e}",
            managed::NAME
        )),
    }
}

/// Names the addon or game file a build stopped on when it is missing or cut
/// short, and what to do about it.
fn named(store: &Store, g: &Game, e: anyhow::Error) -> anyhow::Error {
    match e.downcast::<swofty_core::Error>() {
        Ok(e) => swofty_core::build::named(store, &g.dir, e).into(),
        Err(e) => e,
    }
}

/// Names the folder and the fix when Windows refused a write in the game
/// folder or the data folder.
pub(super) fn denied(e: anyhow::Error, data: &Path, game: &Path) -> anyhow::Error {
    let fix = e.chain().find_map(|c| {
        c.downcast_ref::<swofty_core::Error>()
            .and_then(|c| swofty_core::build::refused_fix(data, game, c))
    });
    match fix {
        Some(fix) => anyhow::anyhow!("{e}. {fix}"),
        None => e,
    }
}

/// Leads with the update to install when an addon's changes failed to apply
/// to an F1 25 older than the newest update the launcher knows. The build's
/// own error follows on the next line, for the logs and Send Logs.
fn outdated(store: &Store, g: &Game, e: anyhow::Error) -> anyhow::Error {
    let version = swofty_core::game_folder::exe_version::read(&g.dir.join(GAME_EXE));
    let names = store
        .addons()
        .map(|a| a.into_iter().map(|(id, m)| (id, m.name)).collect())
        .unwrap_or_default();
    outdated_with(e, version.as_deref(), &names)
}

pub(super) fn outdated_with(
    e: anyhow::Error,
    version: Option<&str>,
    names: &std::collections::BTreeMap<String, String>,
) -> anyhow::Error {
    let raw = format!("{e:#}");
    match swofty_core::diagnostics::known_problems::outdated_game(&raw, version, names) {
        Some(d) => anyhow::anyhow!("{}\n({raw})", d.text()),
        None => e,
    }
}

fn install_checked(
    store: &Store,
    profile: &str,
    g: &Game,
    session: bool,
    out: &dyn Fn(&str),
    progress: &dyn Fn(usize, usize),
) -> Result<()> {
    crate::commands::setup::gate(store)?;
    Blocked::clear(&store.root).ok();
    stock::Report::clear(&store.root).ok();
    // A session's wrapper checked the copy before it got here.
    if !session {
        check_entitled(store, g)?;
    }
    let depots = swofty_core::diagnostics::phases::time("play.stock_check", || check_stock(store, g, out))?;
    let inst = Installer {
        session,
        depot: depots.as_ref().map_or(&() as &dyn DepotHashes, |d| d),
        out,
        progress,
        ..Installer::new(&store.root, g)
    };
    let asked = session
        .then(|| {
            swofty_core::diagnostics::phases::time("play.fetch.integrity", || {
                crate::commands::laps::integrity(store, out)
            })
        })
        .flatten();
    let integrity = asked.as_ref().and_then(|(found, _)| found.as_ref().ok());
    let plugins = if session {
        swofty_core::diagnostics::phases::time("play.fetch.modules", || server_plugins(store, out))
    } else {
        Vec::new()
    };
    let built = timed_build(store, profile, g, integrity, &plugins)?;
    if session {
        crate::commands::laps::note_integrity(store, asked.as_ref(), !built.plan.loader.is_empty(), out);
    }
    report(profile, &built);
    let found = match swofty_core::diagnostics::phases::time("play.install", || inst.apply(&built.plan)) {
        Err(swofty_core::Error::Leftovers(found)) => found,
        r => return Ok(not_stock(store, g, r)?),
    };
    if found.iter().any(|c| !c.owned) {
        return Err(crate::session_lock::blocked(store, g, &found));
    }
    let cleared = inst.clear(&found)?;
    crate::session_lock::announce(store, g, &cleared, out);
    crate::session_lock::record_cleared(store, &cleared);
    let built = timed_build(store, profile, g, integrity, &plugins)?;
    report(profile, &built);
    match swofty_core::diagnostics::phases::time("play.install", || inst.apply(&built.plan)) {
        Err(swofty_core::Error::Leftovers(found)) => Err(crate::session_lock::blocked(store, g, &found)),
        r => Ok(not_stock(store, g, r)?),
    }
}
