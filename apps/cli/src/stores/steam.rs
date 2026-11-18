use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use swofty_core::install::Source;
use swofty_core::platform::Steam;
use swofty_core::platform::process::steamvr_running;
use swofty_core::platform::steam::client as steam_client;
use swofty_core::session::LaunchMode;
use swofty_core::store::Store;
use swofty_core::{STEAM_APP_ID, platform::steam};

use crate::console::output::say;
use crate::stores::client::Notes;

const STEAMVR_START_TIMEOUT: Duration = Duration::from_secs(90);

/// Steam starts SteamVR before a `vr` launch option. Proton reads the VR
/// runtime when the game process starts, so a direct launch does the same.
pub(crate) fn ensure_steamvr() {
    if steamvr_running() {
        say("info", "VR: SteamVR is running");
        return;
    }
    if !steam::find_game().is_some_and(|g| steam::steamvr_installed(&g.steam_root)) {
        say(
            "info",
            "VR: SteamVR is not installed; F1 25 needs it to start in VR (install SteamVR from Steam)",
        );
        return;
    }
    say(
        "info",
        "VR: starting SteamVR first, as Steam does for its VR launch",
    );
    let asked = steam_command(&["-applaunch".to_owned(), steam::STEAMVR_APP_ID.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(e) = asked {
        say("info", &format!("VR: could not ask Steam to start SteamVR: {e}"));
        return;
    }
    let started = Instant::now();
    while !steamvr_running() {
        if started.elapsed() > STEAMVR_START_TIMEOUT {
            say(
                "info",
                &format!(
                    "VR: SteamVR did not start within {} s; launching F1 25 anyway",
                    STEAMVR_START_TIMEOUT.as_secs()
                ),
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    say("info", "VR: SteamVR is running");
}

/// The launcher's status line while the launch waits on Steam; gone once
/// the wait ends.
pub(crate) struct StageFile(pub(crate) PathBuf);

impl StageFile {
    pub(crate) fn set(&self, text: &str) {
        if let Some(dir) = self.0.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(&self.0, text).ok();
    }
}

impl Drop for StageFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

fn steam_starter(game: Option<&Path>) -> Option<steam_client::Starter> {
    let root = steam::find_game().map(|g| g.steam_root);
    let hints: Vec<&Path> = root.as_deref().into_iter().chain(game).collect();
    steam_client::starter(&hints)
}

pub(crate) fn steam_command(args: &[String]) -> Command {
    steam_starter(None)
        .and_then(|s| s.command(args))
        .unwrap_or_else(|| {
            let mut c = Command::new("steam");
            c.args(args);
            c
        })
}

/// Bring Steam up and signed in, whatever folder the game is in.
pub(crate) fn bring_up_steam(store: &Store, game: Option<&Path>) -> Result<()> {
    let mut notes = Notes::new(&store.root, Source::Steam, say);
    Steam
        .start_client(game, &mut |n| notes.note(n))
        .map_err(|e| anyhow!(e))?;
    Ok(())
}

pub(crate) fn steam_launch_args(mode: LaunchMode) -> Vec<String> {
    ["-applaunch".to_owned(), STEAM_APP_ID.to_string()]
        .into_iter()
        .chain(mode.args().iter().map(|a| (*a).to_owned()))
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_goes_when_the_wait_ends() {
        let root = std::env::temp_dir().join(format!("swofty-run-{}-policy", std::process::id()));
        let stage = StageFile(root.join("data").join(steam_client::STAGE_FILE));
        stage.set(steam_client::Stage::SigningIn.text());
        assert_eq!(
            std::fs::read_to_string(&stage.0).unwrap(),
            "Waiting for Steam to sign in…"
        );
        let path = stage.0.clone();
        drop(stage);
        assert!(!path.exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
