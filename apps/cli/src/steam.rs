use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::Result;
use swofty_core::process::steamvr_running;
use swofty_core::session::LaunchMode;
use swofty_core::steam_client::{self, Ready, Report, State, Wait};
use swofty_core::store::Store;
use swofty_core::{STEAM_APP_ID, steam};

use crate::output::say;

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

#[derive(Debug, PartialEq)]
pub(crate) enum SteamPolicy {
    Start,
    Off,
    NotSteam,
    SteamSession,
}

pub(crate) fn steam_policy(on: bool, game: &Path, steam_session: bool) -> SteamPolicy {
    if !steam::in_steam_library(game) {
        SteamPolicy::NotSteam
    } else if steam_session {
        SteamPolicy::SteamSession
    } else if on {
        SteamPolicy::Start
    } else {
        SteamPolicy::Off
    }
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

/// A Steam copy of the game needs the Steam client up and signed in; start
/// it when it is closed, as Steam's own Play button would.
pub(crate) fn ensure_steam(store: &Store, game: &Path, opted_out: bool) -> Result<()> {
    let on = store.config().launch.start_store && !opted_out;
    match steam_policy(on, game, steam_client::steam_session()) {
        SteamPolicy::NotSteam => say(
            "steam",
            "the game folder isn't in a Steam library; not checking Steam",
        ),
        SteamPolicy::SteamSession => say("steam", "Steam runs this session; not checking it"),
        SteamPolicy::Off => {
            let status = steam_client::status();
            say("steam", &format!("detected: {status}"));
            if !status.state.usable() {
                say(
                    "steam",
                    "starting Steam is turned off (launch.start_store or --no-start-store); \
                     open Steam if F1 25 doesn't start",
                );
            }
        }
        SteamPolicy::Start => bring_up_steam(store, Some(game))?,
    }
    Ok(())
}

fn bring_up_steam(store: &Store, game: Option<&Path>) -> Result<()> {
    let starter = steam_starter(game);
    let stage = StageFile(store.root.join(steam_client::STAGE_FILE));
    let ready = steam_client::ensure(
        steam_client::status,
        || {
            let starter = starter.as_ref().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Steam was not found on this computer",
                )
            })?;
            say("steam", &format!("starting Steam: {starter}"));
            starter.start()
        },
        Wait::default(),
        |report| match report {
            Report::Detected(status) => say("steam", &format!("detected: {status}")),
            Report::Changed(status, after) if status.state == State::Ready => say(
                "steam",
                &format!("ready after {:.1} s ({})", after.as_secs_f64(), status.detail()),
            ),
            Report::Changed(status, after) => {
                say("steam", &format!("{status} at {:.1} s", after.as_secs_f64()))
            }
            Report::Stage(s) => stage.set(s.text()),
        },
    )?;
    if ready == Ready::Unconfirmed {
        say(
            "steam",
            &format!(
                "no sign-in seen after {} s; launching anyway",
                steam_client::READY_TIMEOUT.as_secs()
            ),
        );
    }
    Ok(())
}

/// `swofty steam`: what the launch sees, and with `start`, the same start
/// and wait as Play, without launching the game.
pub fn check(store: &Store, start: bool) -> Result<ExitCode> {
    if start {
        bring_up_steam(store, None)?;
        return Ok(ExitCode::SUCCESS);
    }
    let status = steam_client::status();
    say("steam", &format!("detected: {status}"));
    if let Some(g) = swofty_core::steam::find_game() {
        say(
            "steam",
            &match swofty_core::steam::busy(&g) {
                Some(state) => format!("F1 25: Steam is {state} its files"),
                None => "F1 25: Steam isn't changing its files".to_owned(),
            },
        );
    }
    Ok(if status.state.usable() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

pub(crate) fn steam_launch_args(mode: LaunchMode) -> Vec<String> {
    ["-applaunch".to_owned(), STEAM_APP_ID.to_string()]
        .into_iter()
        .chain(mode.args().iter().map(|a| (*a).to_owned()))
        .collect()
}
