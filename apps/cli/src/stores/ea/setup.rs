use std::path::Path;
#[cfg(not(windows))]
use std::time::Instant;

use anyhow::{Result, bail};
use swofty_core::ea_app::{self, Handoff};
use swofty_core::install::{Game, Source};
#[cfg(not(windows))]
use swofty_core::process::stop_process;
#[cfg(not(windows))]
use swofty_core::steam_client;
use swofty_core::store::Store;

#[cfg(not(windows))]
use crate::console::output::say;
#[cfg(not(windows))]
use crate::stores::ea::SERVER_STOP_TIMEOUT;

/// Which store Play brings up first for a copy of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreStart {
    Steam,
    EaApp,
}

/// An EA app copy always starts through the EA app, which launches only a
/// game its account owns; `--direct` is for Steam copies.
pub fn store_start(source: Source) -> StoreStart {
    match source {
        Source::EaApp => StoreStart::EaApp,
        _ => StoreStart::Steam,
    }
}

pub(super) fn handoff(store: &Store) -> Option<Handoff> {
    ea_app::handoff(store.config().launch.ea_launch_command.as_deref())
}

/// Stop before anything is built when the EA app has F1 25 in another
/// folder: its launch link can't start this copy.
pub fn check_recorded(game: &Path) -> Result<()> {
    let Some(m) = ea_app::moved(game, &ea_app::recorded_here(), ea_app::is_install) else {
        return Ok(());
    };
    let pick = if m.there {
        format!(
            ". To play the copy in {0}: swofty game use \"{0}\"",
            m.recorded.display()
        )
    } else {
        String::new()
    };
    bail!("{}{pick}", m.text())
}

pub(super) fn desktop_opener(h: &Handoff) -> Option<(std::process::Command, bool)> {
    ea_app::desktop_opener(h, swofty_core::compat::elevated())
}

/// Whether a session puts offline play's file in EA AntiCheat's launcher's
/// place. The EA app runs that launcher to start F1 25 and asks to repair
/// F1 25 when another file is there, so a vanilla session through the EA
/// app leaves it as installed.
pub fn stages_offline(via_ea: bool, vanilla: bool) -> bool {
    !(via_ea && vanilla)
}

/// Wine reads a prefix's user.reg only when its wineserver starts, and the
/// EA app's wineserver also runs F1 25. So the version.dll override goes into
/// the EA app's prefix while the EA app is closed, and stays there: without
/// Swofty's version.dll in the game folder Wine loads its own.
#[cfg(not(windows))]
pub fn dll_setting(store: &Store, g: &Game, opted_out: bool) -> Result<()> {
    use swofty_core::install::{add_override, has_override};
    let Some((reg, pfx)) = g.user_reg.as_deref().and_then(|r| Some((r, r.parent()?))) else {
        return Ok(());
    };
    if has_override(&std::fs::read_to_string(reg)?) {
        return Ok(());
    }
    if let Some(server) = ea_app::prefix_server(pfx) {
        if !store.config().launch.start_store || opted_out {
            bail!(
                "close the EA app and press Play again: Wine loads Swofty's version.dll in F1 25 only when the EA \
                 app starts after Swofty sets it up in the EA app's prefix"
            );
        }
        say(
            "ea app",
            "closing the EA app so Wine loads Swofty's version.dll in F1 25; it starts again with the setting",
        );
        stop_process(server)?;
        let since = Instant::now();
        while ea_app::prefix_server(pfx).is_some() || ea_app::running() {
            if since.elapsed() >= SERVER_STOP_TIMEOUT {
                bail!(
                    "the EA app didn't close within {} s; close it and press Play again",
                    SERVER_STOP_TIMEOUT.as_secs()
                );
            }
            std::thread::sleep(steam_client::POLL_INTERVAL);
        }
    }
    if let Some(out) = add_override(&std::fs::read_to_string(reg)?) {
        std::fs::write(reg, out)?;
    }
    say(
        "ea app",
        &format!("{} has Wine load Swofty's version.dll in F1 25", reg.display()),
    );
    Ok(())
}

#[cfg(windows)]
pub fn dll_setting(_: &Store, _: &Game, _: bool) -> Result<()> {
    Ok(())
}
