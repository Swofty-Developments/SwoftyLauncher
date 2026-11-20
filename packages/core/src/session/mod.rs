pub mod record;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::asset::Kind;

pub use record::{
    CORE_STOPPED_LOAD, END_FILE, Ending, LogMark, STOP_FILE, SessionEnd, SessionRecord, crash_code,
    request_stop, written_since,
};

pub const DEFAULT_WINEDEBUG: &str = "+timestamp,+pid,+tid,+loaddll,fixme-all";
pub const LOADER_LOG: &str = "swofty/loader.log";
/// Where in the data folder the loader logs when the game folder refuses `LOADER_LOG`.
pub const LOADER_FALLBACK: &str = "state/loader-fallback.log";
pub const LOADER_MARK: &str = "loader_mark.json";
pub const PERF_LOG: &str = "swofty/perf.log";
/// Where in the data folder the core writes its performance report when the game folder refuses `PERF_LOG`.
pub const PERF_FALLBACK: &str = "state/perf-fallback.log";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub schema: u32,
    pub ticket: String,
    pub expires: String,
    pub mode: String,
    pub game: GameRef,
    pub profile: String,
    pub overlay: Overlay,
    #[serde(default)]
    pub plugins: Vec<PluginPin>,
    #[serde(default)]
    pub options: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    pub ui_bridge: UiBridge,
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameRef {
    pub app_id: u32,
    pub build: String,
    pub exe_sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Overlay {
    pub root: String,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginPin {
    pub id: String,
    pub dll: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UiBridge {
    #[serde(default)]
    pub notify_map: BTreeMap<String, (String, String, serde_json::Value)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Policy {
    pub offline: String,
    pub save_namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub launch: LaunchConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
    /// The folder Locate F1 25 saved before 1.3.0; [`Config::migrate_games`]
    /// moves it into `games`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Games::is_empty")]
    pub games: Games,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub console: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub developer: bool,
    /// Opt-in: clear old game session logs each time the launcher starts (see
    /// `data_usage::prune_sessions`). Off unless the player turns it on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_old_session_logs: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sidebar_list: bool,
    /// The layout the player chose on first run; `None` until they choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_mode: Option<UiMode>,
    /// The interface scale in percent, 1 to 400; `None` is Auto, the scale the display mode gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<u16>,
    /// The code of the language the launcher shows; `None` is English.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The version of the welcome wizard the player last finished or skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboarding: Option<u32>,
    /// Set by "Don't ask again" on the prompt that offers addons and saves found in another folder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_find_prompt: bool,
    #[serde(default)]
    pub data_version: u32,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub runtime_assets: BTreeSet<Kind>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub build_assets: BTreeSet<Kind>,
    #[serde(default, skip_serializing_if = "UpdateConfig::is_default")]
    pub updates: UpdateConfig,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub vr_profiles: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_username: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub news_seen: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dismissed_cards: BTreeSet<String>,
    /// The career vault's size cap in MB; 500 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_cap_mb: Option<u64>,
    /// Maximum simultaneous Addon Browser install jobs; two when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_jobs: Option<usize>,
    /// Upload completed Time Trial laps to the Swofty leaderboards while signed in; on unless turned off.
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub time_trial_boards: bool,
    /// Show the Swofty boards and rival ghosts inside Time Trial; on unless turned off.
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub time_trial_ingame: bool,
    /// Show the latest Swofty video card on the F1 25 home screen; on unless turned off.
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub home_video: bool,
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub use_profiles: bool,
    /// Offer Addon Browser updates for installed addons that are off as well; off unless turned on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub update_off_addons: bool,
}

fn on() -> bool {
    true
}

fn is_on(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateConfig {
    #[serde(default = "yes")]
    pub check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { check: true }
    }
}

impl UpdateConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        serde_json::from_str("{}").expect("every field of Config has a default")
    }
}

/// The launcher's two layouts: the desktop one, and TV mode for handhelds and
/// couch play, which scales the interface up and widens the scrollbars.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    #[default]
    Pc,
    Tv,
}

impl UiMode {
    pub fn is_tv(self) -> bool {
        self == Self::Tv
    }
}

/// The smallest interface scale in percent.
pub const UI_SCALE_MIN: u16 = 1;

/// The largest interface scale in percent.
pub const UI_SCALE_MAX: u16 = 400;

/// `percent` held to the range the launcher supports, so a hand-edited value stays usable.
pub fn clamp_ui_scale(percent: u16) -> u16 {
    percent.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

/// The version of the welcome wizard. Raise it to show the wizard once more to everyone.
pub const ONBOARDING_VERSION: u32 = 2;

impl Config {
    /// Whether the welcome wizard is due: never seen, or an older version.
    pub fn needs_onboarding(&self) -> bool {
        self.onboarding.is_none_or(|seen| seen < ONBOARDING_VERSION)
    }

    /// The chosen interface scale in percent, held to 1 to 400; `None` is Auto.
    pub fn ui_scale_percent(&self) -> Option<u16> {
        self.ui_scale.map(clamp_ui_scale)
    }

    /// The layout in use: what the player chose, PC until they do.
    pub fn ui_mode_or_default(&self) -> UiMode {
        self.ui_mode.unwrap_or_default()
    }
}

/// The copies of F1 25 the player added by folder, and the one Play uses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Games {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<PathBuf>,
}

impl Games {
    pub fn is_empty(&self) -> bool {
        self.active.is_none() && self.added.is_empty()
    }

    pub fn add(&mut self, dir: PathBuf) {
        if !self.added.contains(&dir) {
            self.added.push(dir);
        }
    }
}

impl Config {
    /// Return a valid limit even if a config was edited by hand.
    pub fn download_jobs(&self) -> usize {
        self.download_jobs.unwrap_or(2).clamp(1, 10)
    }

    pub fn set_download_jobs(&mut self, limit: usize) {
        self.download_jobs = Some(limit.clamp(1, 10));
    }

    pub fn migrate_games(&mut self) {
        if let Some(dir) = self.game_dir.take() {
            self.games.active.get_or_insert_with(|| dir.clone());
            self.games.add(dir);
        }
    }

    pub fn launch_mode(&self, profile: &str) -> LaunchMode {
        if self.vr_profiles.contains(profile) {
            LaunchMode::Vr
        } else {
            LaunchMode::Normal
        }
    }

    pub fn set_launch_mode(&mut self, profile: &str, mode: LaunchMode) {
        match mode {
            LaunchMode::Vr => self.vr_profiles.insert(profile.to_owned()),
            LaunchMode::Normal => self.vr_profiles.remove(profile),
        };
    }
}

/// Steam's own launch options for app 3059520 (`appinfo.vdf`, `config` →
/// `launch`): option 0 runs `EAAntiCheat.GameServiceLauncher.exe` with no
/// arguments, and option 1, of type `vr`, runs the same launcher with `-vr`.
pub const VR_ARG: &str = "-vr";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    #[default]
    Normal,
    Vr,
}

impl LaunchMode {
    pub fn of(game_args: &[String]) -> Self {
        if game_args.iter().any(|a| a.eq_ignore_ascii_case(VR_ARG)) {
            Self::Vr
        } else {
            Self::Normal
        }
    }

    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Normal => &[],
            Self::Vr => &[VR_ARG],
        }
    }

    pub fn is_vr(self) -> bool {
        self == Self::Vr
    }
}

/// The game's arguments for a launch: what Steam passed after the target
/// (its VR launch option adds `-vr` there), plus `-vr` for a VR launch when
/// Steam's command lacks it.
pub fn game_args(steam_args: &[String], mode: LaunchMode) -> Vec<String> {
    let mut out = steam_args.to_vec();
    if mode.is_vr() && !LaunchMode::of(steam_args).is_vr() {
        out.push(VR_ARG.to_owned());
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_affinity: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_winedebug")]
    pub winedebug: String,
    /// Start the store the active copy came from (Steam or the EA app) when
    /// Play finds it closed.
    #[serde(default = "yes", skip_serializing_if = "is_yes", alias = "start_steam")]
    pub start_store: bool,
    /// The command that hands an EA app launch link to the EA app, with
    /// `{url}` where the link goes. Needed on Linux, optional on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ea_launch_command: Option<String>,
    /// The Wine prefix the EA app runs in on Linux (the folder with
    /// `user.reg`, or the Proton compatdata folder holding `pfx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ea_prefix: Option<PathBuf>,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            cpu_affinity: None,
            env: BTreeMap::new(),
            winedebug: default_winedebug(),
            start_store: true,
            ea_launch_command: None,
            ea_prefix: None,
        }
    }
}

fn yes() -> bool {
    true
}

fn is_yes(on: &bool) -> bool {
    *on
}

/// The Windows environment block cannot exceed 32767 characters, and
/// Proton's steam helper packs the game's environment into one. A `nix
/// develop` (or CI) shell more than doubles that with build variables, and
/// the helper then creates a game process that dies during startup before
/// loading a single DLL. Build-shell variables are not part of running a
/// game, so they are dropped, and whatever remains is trimmed to fit.
///
/// The launcher's own [`COMPAT_LAYER`] is dropped too: Windows applies the
/// layers it names to every program started with it, and with one set (such
/// as `DetectorsAppHealth`) F1 25 has held System32's version.dll instead of
/// the loader. `overrides` can still set one.
pub fn game_env(
    parent: impl IntoIterator<Item = (String, String)>,
    overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    const LIMIT: usize = 30_000; // under the 32767 block limit, names included

    fn is_build_shell_var(name: &str) -> bool {
        const EXACT: &[&str] = &[
            "AR",
            "AS",
            "CC",
            "CXX",
            "LD",
            "NM",
            "OBJCOPY",
            "OBJDUMP",
            "RANLIB",
            "READELF",
            "SIZE",
            "STRIP",
            "STRINGS",
            "PKG_CONFIG",
            "LIBCLANG_PATH",
            "CONFIG_SHELL",
            "HOST_PATH",
            "SOURCE_DATE_EPOCH",
            "DETERMINISTIC_BUILD",
            "IN_NIX_SHELL",
            "NIXPKGS_CMAKE_PREFIX_PATH",
            "CMAKE_INCLUDE_PATH",
            "CMAKE_LIBRARY_PATH",
            "PYTHONPATH",
            "PYTHONHASHSEED",
            "PYTHONNOUSERSITE",
            "_PYTHON_HOST_PLATFORM",
            "_PYTHON_SYSCONFIGDATA_NAME",
            "buildInputs",
            "nativeBuildInputs",
            "buildPhase",
            "builder",
            "shell",
            "shellHook",
            "stdenv",
            "out",
            "outputs",
            "name",
            "system",
            "phases",
            "patches",
            "strictDeps",
            "doCheck",
            "doInstallCheck",
            "preferLocalBuild",
            "allowSubstitutes",
            "dontAddDisableDepTrack",
            "__structuredAttrs",
            "cmakeFlags",
            "configureFlags",
            "mesonFlags",
        ];
        const PREFIXES: &[&str] = &["NIX_", "NIXPKGS_", "CMAKE_", "deps", "propagated"];
        const SUFFIX: &str = "FLAGS";
        EXACT.contains(&name) || PREFIXES.iter().any(|p| name.starts_with(p)) || name.ends_with(SUFFIX)
    }

    fn block_size(env: &BTreeMap<String, String>) -> usize {
        env.iter().map(|(k, v)| k.len() + v.len() + 2).sum()
    }

    let mut env: BTreeMap<String, String> = parent
        .into_iter()
        .filter(|(k, _)| !is_build_shell_var(k) && !is_compat_layer(k))
        .collect();
    for (k, v) in overrides {
        env.insert(k.clone(), v.clone());
    }
    // Trim whatever is left to fit: the largest values go first, and the
    // variables a game launch cannot do without are kept.
    const KEEP: &[&str] = &["PATH", "HOME", "USER", "TEMP", "TMP", "TMPDIR", "LANG"];
    while block_size(&env) > LIMIT {
        let Some(largest) = env
            .iter()
            .filter(|(k, _)| !KEEP.contains(&k.as_str()) && !overrides.contains_key(*k))
            .max_by_key(|(k, v)| k.len() + v.len())
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        env.remove(&largest);
    }
    env
}

/// The variable Windows reads a process's compatibility layers from, and
/// hands on to the programs that process starts.
pub const COMPAT_LAYER: &str = "__COMPAT_LAYER";

pub fn is_compat_layer(name: &str) -> bool {
    name.eq_ignore_ascii_case(COMPAT_LAYER)
}

/// `WINEDLLOVERRIDES` with Wine loading a native version.dll first, so the
/// loader in the game folder attaches whatever the prefix's registry holds:
/// a prefix the launcher didn't find, a version value of its own, or a
/// registry a wineserver wrote back over the launcher's. None when the value
/// already names version.dll, which is left as the player set it.
pub fn with_version_override(value: Option<&str>) -> Option<String> {
    const OVERRIDE: &str = "version=n,b";
    let names_version = |v: &str| {
        v.split(';').any(|entry| {
            let names = entry.split_once('=').map_or(entry, |(names, _)| names);
            names.split(',').any(|n| {
                let n = n.trim().trim_start_matches('*').to_ascii_lowercase();
                n == "version" || n == "version.dll"
            })
        })
    };
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if names_version(v) => None,
        Some(v) => Some(format!("{};{OVERRIDE}", v.trim_end_matches(';'))),
        None => Some(OVERRIDE.to_owned()),
    }
}

pub const SDL2_SONAME: &str = "libSDL2-2.0.so.0";
pub const RUNTIME_LIBS_DIR: &str = "state/runtime-libs";

const HOST_LIB_DIRS: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu",
    "/lib/x86_64-linux-gnu",
    "/usr/lib64",
    "/lib64",
    "/usr/lib",
    "/lib",
    "/usr/local/lib",
];

const SDL3_SONAME: &str = "libSDL3.so.0";

const STEAM_RUNTIME_LIB_DIRS: &[&str] = &[
    "ubuntu12_32/steam-runtime/usr/lib/x86_64-linux-gnu",
    "ubuntu12_32/steam-runtime/amd64/usr/lib/x86_64-linux-gnu",
];

/// The 64-bit Steam client's own SDL3, which loads X11, Wayland, udev and
/// audio at run time and links nothing past libc.
const STEAM_CLIENT_SDL3: &str = "steamrt64/libSDL3.so.0";

/// The client runtime's platform folders, one per runtime version. Their SDL2
/// is sdl2-compat, which forwards to whichever `libSDL3.so.0` loads.
const STEAM_CLIENT_PLATFORMS: &str = "steamrt64/pv-runtime/steam-runtime-steamrt";
const PLATFORM_LIB_DIR: &str = "files/lib/x86_64-linux-gnu";

/// Proton's HID bus reads wheels and pads through SDL2, which it loads by
/// soname. Under Steam it comes from Steam's runtime; a direct launch (from
/// `steam-run` on NixOS, or a host without SDL2) has none, and every
/// joystick Proton does not drive over hidraw (a G920, most wheels) is then
/// left unclaimed: the evdev side defers it to an SDL side that never
/// started, so the game sees no wheel. When the host has no 64-bit SDL2,
/// this links Steam's copy into `link_dir` and returns `library_path` with
/// that directory appended; `None` leaves the launch as it was. Steam's copy
/// is the old runtime's SDL2 where the client still has it, else the 64-bit
/// client's SDL3 under its runtime's sdl2-compat.
pub fn sdl_library_path(library_path: Option<&str>, steam_root: &Path, link_dir: &Path) -> Option<String> {
    sdl_library_path_in(library_path, HOST_LIB_DIRS, steam_root, link_dir)
}

fn sdl_library_path_in(
    library_path: Option<&str>,
    host: &[&str],
    steam_root: &Path,
    link_dir: &Path,
) -> Option<String> {
    let current = library_path.unwrap_or_default();
    let mut searched = current
        .split(':')
        .filter(|d| !d.is_empty())
        .chain(host.iter().copied());
    if searched.any(|d| is_elf64(&Path::new(d).join(SDL2_SONAME))) {
        return None;
    }
    for (source, soname) in steam_sdl(steam_root)? {
        link_library(&source, &link_dir.join(soname)).ok()?;
    }
    let dir = link_dir.display().to_string();
    Some(if current.is_empty() {
        dir
    } else {
        format!("{current}:{dir}")
    })
}

fn steam_sdl(steam_root: &Path) -> Option<Vec<(PathBuf, &'static str)>> {
    if let Some(sdl2) = STEAM_RUNTIME_LIB_DIRS
        .iter()
        .map(|d| steam_root.join(d).join(SDL2_SONAME))
        .find(|p| is_elf64(p))
    {
        return Some(vec![(sdl2, SDL2_SONAME)]);
    }
    let sdl3 = steam_root.join(STEAM_CLIENT_SDL3);
    if !is_elf64(&sdl3) {
        return None;
    }
    let mut platforms: Vec<PathBuf> = std::fs::read_dir(steam_root.join(STEAM_CLIENT_PLATFORMS))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("_platform_"))
        })
        .collect();
    platforms.sort();
    let sdl2 = platforms.iter().rev().find_map(|p| {
        let mut libs: Vec<PathBuf> = std::fs::read_dir(p.join(PLATFORM_LIB_DIR))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|f| {
                f.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(SDL2_SONAME))
            })
            .collect();
        libs.sort();
        libs.into_iter().find(|f| is_elf64(f))
    })?;
    Some(vec![(sdl2, SDL2_SONAME), (sdl3, SDL3_SONAME)])
}

fn is_elf64(path: &Path) -> bool {
    let mut head = [0u8; 5];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == *b"\x7fELF\x02"
}

#[cfg(unix)]
fn link_library(source: &Path, link: &Path) -> std::io::Result<()> {
    if std::fs::read_link(link).ok().as_deref() == Some(source) {
        return Ok(());
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::remove_file(link) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::os::unix::fs::symlink(source, link)
}

#[cfg(not(unix))]
fn link_library(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

fn default_winedebug() -> String {
    DEFAULT_WINEDEBUG.into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LaunchRequest {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winedebug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub apply: bool,
    /// `SWOFTY_NETPLAY` for the game, for a wrapper Steam starts without the caller's environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netplay: Option<String>,
    /// A file of the host's row ids ([`crate::row_ids::Overrides`]) this one session's builds use, for a
    /// room launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_ids: Option<PathBuf>,
    /// Run this one session on Steam's saves, though the profile uses launcher saves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub steam_saves_once: bool,
}

/// Keep the logs the game wrote this session in the session folder `dir`: the loader log and the performance
/// report as `loader.log` and `perf.log`, from the game folder or, when the game folder refused them, from
/// their fallbacks in the data folder (which a session starts without), and the crash and load-stall reports
/// the core left in the data folder.
pub fn keep_game_logs(data: &Path, game: &Path, dir: &Path, before: Option<&LogMark>, launched: SystemTime) {
    crate::diagnostics::crash_report::keep_fallback(data, &dir.join("game-logs"), launched);
    let loader = game.join(LOADER_LOG);
    let fallback = data.join(LOADER_FALLBACK);
    if LogMark::replaced(&loader, before) {
        std::fs::copy(&loader, dir.join("loader.log")).ok();
    } else if fallback.is_file() {
        std::fs::copy(fallback, dir.join("loader.log")).ok();
    }
    if let Some(perf) = [game.join(PERF_LOG), data.join(PERF_FALLBACK)]
        .into_iter()
        .find(|p| written_since(p, launched))
    {
        std::fs::copy(perf, dir.join("perf.log")).ok();
    }
}

pub fn parse_cpu_list(spec: &str) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => out.extend(a.parse::<usize>().ok()?..=b.parse::<usize>().ok()?),
            None => out.push(part.parse().ok()?),
        }
    }
    (!out.is_empty()).then_some(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteamCommand {
    pub proton: Option<String>,
    pub verb: Option<String>,
    pub target: Option<usize>,
    pub argv: Vec<String>,
}

impl SteamCommand {
    pub fn game_args(&self) -> &[String] {
        self.target
            .and_then(|t| self.argv.get(t + 1..))
            .unwrap_or_default()
    }
}

pub fn parse_steam_command(argv: &[String]) -> SteamCommand {
    let proton = argv.iter().position(|a| a.ends_with("/proton"));
    let verb = proton.and_then(|i| argv.get(i + 1)).cloned();
    let target = proton.map(|i| i + 2).filter(|&i| i < argv.len()).or_else(|| {
        argv.iter()
            .rposition(|a| a.to_ascii_lowercase().ends_with(".exe"))
    });
    SteamCommand {
        proton: proton.map(|i| argv[i].clone()),
        verb,
        target,
        argv: argv.to_vec(),
    }
}
