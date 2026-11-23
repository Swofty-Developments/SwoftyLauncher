//! Offline play: the launch extension that lets modded sessions start without
//! EA AntiCheat (Javelin).
//!
//! Modded sessions need in-process code, which Javelin blocks, and a game
//! started without the service aborts on its own launch check. The extension
//! (a separately distributed project) swaps the anti-cheat launcher for the
//! session and lets the game continue past that check. Sessions stay offline:
//! multiplayer is unavailable and cannot be enabled, because online play
//! requires the Javelin handshake with EA services.
//!
//! This module owns the policy around it:
//! - [consent]: the terms are shown and recorded once; without acceptance the
//!   manager refuses to launch the game modded.
//! - [install] / [fetch]: the extension is verified against the SHA-256 digests
//!   pinned here before anything is staged, so a tampered or lookalike download
//!   is refused however it arrived.
//! - [stage] / [restore]: session-scoped changes are journaled and returned to
//!   vanilla when the game exits, on `swofty restore`, and at the next start.

use std::path::{Path, PathBuf};

mod antivirus;
mod consent;
mod exec;
mod extension;
mod heal;
mod journal;
mod probe;

#[cfg(test)]
mod tests;

use self::antivirus::{ANTIVIRUS, av_error, virus_blocked};
pub use self::antivirus::{
    staged_at, stub_blocked, stub_removed, stub_removed_from_extension, stub_removed_text,
};
pub use self::consent::{Consent, consent, is_accepted, record_accept, withdraw};
pub use self::exec::{Exec, exec_mode};
use self::exec::{command, loader_hint, start_error};
pub use self::extension::{Artifacts, artifacts, fetch, install};
use self::heal::{ea_signed, heal_disk, not_eas, require_launcher, set_aside};
pub use self::heal::{heal_before_check, heal_stale_backup};
pub(crate) use self::journal::Journal;
pub use self::journal::{Slot, restage, restore, restore_pending, slot, stage};
pub use self::probe::{Probe, probe, probe_cached, probe_path};

const EAC_EXE: &str = "EAAntiCheat.GameServiceLauncher.exe";
const EAC_BAK: &str = "EAAntiCheat.GameServiceLauncher.exe.bak";

/// The terms shown before consent and by `swofty offline accept`.
pub const DISCLOSURE: &str = "\
Offline play - read this before accepting

Modded sessions run F1 25 without EA AntiCheat so the loader and addons
can work. Everything is undone when the game exits.

- Playing without EA AntiCheat is outside EA's terms of service, even
  offline. We know of no action taken against players for offline modding,
  but EA can still act on accounts under its terms.
- Sessions run offline. Multiplayer is unavailable, and this component
  cannot enable it.
- Everything is scoped to the session: the game folder is restored to
  vanilla when the game exits, by `swofty restore`, and at the next start.
- The extension is downloaded from Swofty's site and verified against
  pinned SHA-256 digests before it is used. Source: Swofty-Developments/F1-OfflinePlay.

We may collect anonymised usage data from your use of the launcher.";

/// The extension release this manager pins.
pub const EXTENSION_VERSION: &str = "0.4.0";
/// Where the artifacts are hosted (binaries; source lives in the repository).
pub const DEFAULT_BASE_URL: &str = "https://swofty-developments.github.io/F1-OfflinePlay";
pub const SOURCE_URL: &str = "https://github.com/Swofty-Developments/F1-OfflinePlay";

/// Pinned digests for release v0.4.0. Every artifact is checked against its
/// pin at install time and again at use time. The notice files are text that
/// ships beside the binaries; they are verified like the rest but are never run
/// or staged into the game folder.
pub const PINS: &[(&str, &str)] = &[
    (
        "f1op_hook.dll",
        "4d347140704dca05cc78b88794f8e8fe67aa6bd1f8be5004f974842e7c3d7133",
    ),
    (
        "f1op_stub.exe",
        "e428b80f183c5c95533ecb8a1d7fcf0b8ad8a9152e9b029d388c592508dcf727",
    ),
    (
        "f1op-prelaunch.exe",
        "907fc221c7be99e561e5dfae0a17095aaae718d4caef81d67e623adc12187efc",
    ),
    (
        "f1op-probe.exe",
        "f268b062e17921c1a75d0e75d464bba78b649313852b628777928486a595a75f",
    ),
    (
        "f1op-prelaunch-linux-x86_64",
        "098f9142b81671a4861b173787deddd39cc472931b5ad481e283f650af743966",
    ),
    (
        "f1op-probe-linux-x86_64",
        "8c5a3353f38ddcb511f3502d7b6ee1032ede344c3a8db74165e46dce2b3cb9b8",
    ),
    (
        "LICENSE.txt",
        "5560fee8e4299e8046d5b043b86c7d6e8c27f756ca9492a46c15cb013cea10d7",
    ),
    (
        "notices/README.md",
        "074bb16d4908739956d8f2d37fe042bd76ebe57022c972b18d39afdf3e859133",
    ),
    (
        "notices/AGENTS.md",
        "33a324752952df10d1ce36e07886da535a008721be7976340304419f726dc54a",
    ),
    (
        "notices/CLAUDE.md",
        "5af4092cca6c0d23e49f16633f014fd43458c563659f243686175357fb6eb730",
    ),
];

/// The files this platform installs: the artifacts it runs or stages, then the
/// notice files, which install beside them but are never run or staged.
#[cfg(windows)]
pub const REQUIRED: &[&str] = &[
    "f1op_hook.dll",
    "f1op_stub.exe",
    "f1op-prelaunch.exe",
    "f1op-probe.exe",
    "LICENSE.txt",
    "notices/README.md",
    "notices/AGENTS.md",
    "notices/CLAUDE.md",
];
#[cfg(not(windows))]
pub const REQUIRED: &[&str] = &[
    "f1op_hook.dll",
    "f1op_stub.exe",
    "f1op-prelaunch-linux-x86_64",
    "f1op-probe-linux-x86_64",
    "LICENSE.txt",
    "notices/README.md",
    "notices/AGENTS.md",
    "notices/CLAUDE.md",
];

fn pin(name: &str) -> Option<&'static str> {
    PINS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// Where the verified extension is installed.
pub fn extension_dir(data: &Path) -> PathBuf {
    data.join("extensions/offline-play")
}

/// The staging journal for the current session; present only while staged.
pub fn journal_path(data: &Path) -> PathBuf {
    data.join("state/offline/staging.jsonl")
}

pub fn base_url() -> String {
    std::env::var("SWOFTY_OFFLINE_PLAY_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned())
}
