//! The Windows "Run this program as an administrator" setting on the game.
//!
//! Windows will not start an exe that is set to run as administrator from an
//! unelevated process (CreateProcess error 740), and the offline play session
//! launcher does not elevate the game, so a modded session cannot start while
//! the setting is on and the launcher runs unelevated. The setting is a
//! RUNASADMIN layer on the exe's path under `AppCompatFlags\Layers`, per user
//! (HKCU) or for all users (HKLM). This only reads it; the registry is never
//! changed.

use crate::GAME_EXE;

mod compat;
mod ea;
mod elevation;
mod tools;
mod unelevated;
mod write_access;

pub use compat::{
    AdminLayer, admin_layers, elevation_blocker, explain, has_layers, has_run_as_admin, layers,
    session_admin_layers, starter_layers, starter_layers_in,
};
pub use ea::{
    EA_APP_EXES, ELEVATION_REQUIRED, Elevation, RunAsAdmin, STUB_ELEVATION, ea_blocker, ea_blocker_in,
    game_refused, game_refused_in, needs_elevation, refused_start, refused_start_in, stub_needs_elevation,
};
pub use elevation::{Elevated, elevated, relaunch_elevated};
pub(crate) use tools::windows_tool;
#[cfg(windows)]
pub(crate) use tools::windows_tool_within;
pub use write_access::{
    CORE_CACHES, FIRST_WRITE, core_cache_dir, open_core_caches, unwritable_dir, virtual_store,
    write_blocked_text, write_blocked_text_for, write_blocker,
};

pub const LAYERS_KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

/// The exes a modded session starts: the game, and the session launcher in
/// the anti-cheat launcher's slot.
pub const SESSION_EXES: &[&str] = &[GAME_EXE, "EAAntiCheat.GameServiceLauncher.exe"];

/// The programs that start F1 25, wherever they are installed. Windows hands
/// a program's compatibility layers on to the programs it starts.
pub const STARTERS: &[&str] = &[
    crate::platform::ea_app::PROCESS,
    crate::platform::ea_app::LAUNCHER_EXE,
    "swofty-launcher.exe",
];

/// The string values `reg query <key>` lists, as (name, data).
pub fn parse_reg_query(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|line| {
            let rest = line.trim_end_matches('\r').strip_prefix("    ")?;
            ["    REG_SZ", "    REG_EXPAND_SZ"].iter().find_map(|ty| {
                let (name, data) = rest.split_once(ty)?;
                let data = data.strip_prefix("    ").or(data.is_empty().then_some(""))?;
                Some((name.to_owned(), data.to_owned()))
            })
        })
        .collect()
}

/// A REG_DWORD value `reg query <key>` lists, by name.
pub fn reg_dword(out: &str, name: &str) -> Option<u32> {
    out.lines().find_map(|line| {
        let rest = line.trim_end_matches('\r').strip_prefix("    ")?;
        let (value, data) = rest.split_once("    REG_DWORD    ")?;
        value.eq_ignore_ascii_case(name).then_some(())?;
        u32::from_str_radix(data.trim().strip_prefix("0x")?, 16).ok()
    })
}

/// The per-user and all-users `reg query` listings of [`LAYERS_KEY`].
fn listings() -> Vec<(String, bool)> {
    [("HKCU", false), ("HKLM", true)]
        .into_iter()
        .filter_map(|(hive, all_users)| {
            let key = format!(r"{hive}\{LAYERS_KEY}");
            Some((
                crate::platform::registry::reg_query(&[&key, "/reg:64"])?,
                all_users,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
