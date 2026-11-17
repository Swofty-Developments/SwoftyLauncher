use std::path::Path;
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use swofty_core::ea_app::{self, Handoff};
use swofty_core::install::{Game, Source};
#[cfg(not(windows))]
use swofty_core::process::stop_process;
use swofty_core::steam_client;
use swofty_core::store::Store;

use crate::console::output::say;
#[cfg(not(windows))]
use crate::stores::ea::SERVER_STOP_TIMEOUT;
use crate::stores::ea::{APP_SETTLE, APP_START_TIMEOUT, AppStart, detach, wait_for_app};
use crate::stores::steam::StageFile;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    NoHandoff,
    Running,
    Start,
    Off,
}

pub fn policy(handoff: bool, on: bool, running: bool) -> Policy {
    match (handoff, running, on) {
        (false, _, _) => Policy::NoHandoff,
        (true, true, _) => Policy::Running,
        (true, false, true) => Policy::Start,
        (true, false, false) => Policy::Off,
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

/// An EA app copy needs the EA app: start it when it is closed and wait for
/// it, as Steam is for a Steam copy. On Linux the launch.ea_launch_command
/// setting, run without a link, starts it; a command that needs the link
/// leaves the start to the link.
pub fn ensure(store: &Store, opted_out: bool) -> Result<()> {
    let h = handoff(store);
    let on = store.config().launch.start_store && !opted_out;
    match policy(h.is_some(), on, ea_app::running()) {
        Policy::NoHandoff => bail!("{}", ea_app::no_handoff()),
        Policy::Running => say("ea app", "detected: running"),
        Policy::Off => bail!(
            "the EA app is closed and starting it is turned off (launch.start_store or --no-start-store); \
             open the EA app and press Play again"
        ),
        Policy::Start => match h.as_ref().and_then(desktop_opener) {
            Some((mut open, exit_tells)) => {
                say("ea app", "detected: closed; starting the EA app");
                let stage = StageFile(store.root.join(steam_client::STAGE_FILE));
                stage.set("Starting the EA app…");
                let mut child = detach(&mut open).map_err(|e| {
                    anyhow!("could not start the EA app ({e}); open the EA app and press Play again")
                })?;
                let since = Instant::now();
                match wait_for_app(&mut child, exit_tells, || since.elapsed() >= APP_START_TIMEOUT) {
                    AppStart::Running => {
                        say(
                            "ea app",
                            &format!("running after {:.1} s", since.elapsed().as_secs_f64()),
                        );
                        std::thread::sleep(APP_SETTLE);
                    }
                    AppStart::CommandFailed(exit) => say(
                        "ea app",
                        &format!("the command did not open the EA app ({exit}); the launch link starts it"),
                    ),
                    AppStart::TimedOut => bail!(
                        "the EA app didn't start within {} s; open the EA app and press Play again",
                        APP_START_TIMEOUT.as_secs()
                    ),
                }
            }
            None => say("ea app", "detected: closed; the launch link starts it"),
        },
    }
    Ok(())
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
