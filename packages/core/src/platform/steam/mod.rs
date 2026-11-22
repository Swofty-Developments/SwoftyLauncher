pub mod client;
mod library;
mod proton;
mod saves;
pub mod vdf;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::install::Source;
use crate::platform::ensure::SteamClient;
use crate::platform::{ClientNote, ClientReady, ClientStart, LaunchPlatform};
use crate::{GAME_EXE, STEAM_APP_ID};
use vdf::Vdf;

pub(crate) use library::isolated_root;
pub use library::{
    drive_roots, find_app, find_app_in, find_game, find_game_in, in_steam_library, libraries,
    registered_roots, root_of_game, root_of_game_in, steam_roots,
};
pub use proton::{ProtonSearch, compat_tool, find_proton, find_proton_in, system_compat_dirs};
pub use saves::{
    SaveSearch, active_user, find_save_dir, save_dir, save_dir_with, search_save, search_save_in,
};

/// Steam, as a [`LaunchPlatform`].
pub struct Steam;

impl LaunchPlatform for Steam {
    fn name(&self) -> &'static str {
        "Steam"
    }

    fn source(&self) -> Source {
        Source::Steam
    }

    fn installed(&self, dir: &Path) -> bool {
        dir.join("EAJavelinInstaller_installscript.vdf").is_file() || in_steam_library(dir)
    }

    fn installs(&self) -> Vec<PathBuf> {
        find_game().map(|g| g.dir).into_iter().collect()
    }

    fn client_running(&self) -> bool {
        client::status().state != client::State::Stopped
    }

    fn move_hint(&self) -> &'static str {
        " in Steam (Properties > Installed Files > Move install folder)"
    }

    fn ensure_client(
        &self,
        start: ClientStart<'_>,
        note: &mut dyn FnMut(ClientNote),
    ) -> Result<ClientReady, String> {
        let mut detect = client::status;
        SteamClient::for_launch(start, &mut detect).ensure(start.allowed, note)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameInstall {
    pub steam_root: PathBuf,
    pub library: PathBuf,
    pub dir: PathBuf,
    pub build_id: Option<u64>,
    pub compatdata: Option<PathBuf>,
}

impl GameInstall {
    pub fn exe(&self) -> PathBuf {
        self.dir.join(GAME_EXE)
    }

    pub fn prefix(&self) -> Option<PathBuf> {
        self.compatdata.as_ref().map(|c| c.join("pfx"))
    }
}

fn read_vdf(path: &Path) -> Option<Vdf> {
    vdf::parse(&std::fs::read_to_string(path).ok()?)
}

const VALIDATING: u64 = 0x2_0000;
// EAppState bits for a running update, uninstall or backup, and the stages of
// a download: Steam is writing the game's files.
const CHANGING: u64 = 0x100
    | 0x400
    | 0x800
    | 0x1000
    | 0x1_0000
    | 0x4_0000
    | 0x8_0000
    | 0x10_0000
    | 0x20_0000
    | 0x40_0000
    | 0x80_0000;

/// What Steam is doing to the game's files: "verifying" while it validates
/// them, "updating" while it changes them any other way, None when it leaves
/// them alone. Steam keeps the `StateFlags` of the app manifest as they were
/// while it verifies, so a running Steam's content log, which records every
/// state change, is read first.
pub fn busy(install: &GameInstall) -> Option<&'static str> {
    let running = client::status().state != client::State::Stopped;
    running
        .then(|| log_busy(&install.steam_root.join("logs/content_log.txt")))
        .flatten()
        .or_else(|| manifest_busy(install))
}

/// The game's last state in Steam's content log: busy while an update runs,
/// and verifying when the update is a check of the installed files.
fn log_busy(log: &Path) -> Option<&'static str> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(log).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(256 * 1024))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let last = |what: &str| {
        let key = format!("AppID {STEAM_APP_ID} {what} : ");
        text.lines()
            .rev()
            .find_map(|l| l.split_once(key.as_str()).map(|(_, state)| state.to_owned()))
    };
    if !last("state changed")?.contains("Update Running") {
        return None;
    }
    Some(
        if last("App update changed").is_some_and(|u| u.contains("Verifying")) {
            "verifying"
        } else {
            "updating"
        },
    )
}

fn manifest_busy(install: &GameInstall) -> Option<&'static str> {
    let acf = read_vdf(
        &install
            .library
            .join("steamapps")
            .join(format!("appmanifest_{STEAM_APP_ID}.acf")),
    )?;
    let flags: u64 = acf.path(&["AppState", "StateFlags"])?.str()?.parse().ok()?;
    if flags & VALIDATING != 0 {
        Some("verifying")
    } else if flags & CHANGING != 0 {
        Some("updating")
    } else {
        None
    }
}

pub const STEAMVR_APP_ID: u32 = 250_820;

pub fn steamvr_installed(steam_root: &Path) -> bool {
    libraries(steam_root).iter().any(|lib| {
        lib.join(format!("steamapps/appmanifest_{STEAMVR_APP_ID}.acf"))
            .is_file()
    })
}

pub fn launch_options(steam_root: &Path) -> Vec<(String, String)> {
    let Ok(users) = std::fs::read_dir(steam_root.join("userdata")) else {
        return Vec::new();
    };
    users
        .flatten()
        .filter_map(|u| {
            let cfg = read_vdf(&u.path().join("config/localconfig.vdf"))?;
            let apps = cfg.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"])?;
            let opts = apps.get(&STEAM_APP_ID.to_string())?.get("LaunchOptions")?.str()?;
            Some((u.file_name().to_string_lossy().into_owned(), opts.to_owned()))
        })
        .collect()
}

pub fn wrapper_installed(launch_options: &str) -> bool {
    launch_options.contains("swofty")
        && launch_options.contains("run")
        && launch_options.contains("%command%")
}
