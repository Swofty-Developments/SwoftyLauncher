//! Run-as-administrator settings on an EA app copy's programs.
//!
//! An EA app copy's game is started by the EA app, which runs without
//! administrator rights even when Swofty Launcher has them, so there the
//! setting can only be unticked; see [`RunAsAdmin`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::compat::{AdminLayer, has_run_as_admin, matching_with};
use super::{SESSION_EXES, elevated, listings, parse_reg_query};
use crate::GAME_EXE;

/// The Windows error for a start of a program that has to run as
/// administrator from a process that doesn't (ERROR_ELEVATION_REQUIRED).
pub const ELEVATION_REQUIRED: i32 = 740;

pub fn needs_elevation(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(ELEVATION_REQUIRED)
}

/// The EA app's own programs, wherever it is installed.
pub const EA_APP_EXES: &[&str] = &[
    crate::platform::ea_app::LAUNCHER_EXE,
    crate::platform::ea_app::PROCESS,
];

/// The offline play session launcher's line, in the game folder's loader.log,
/// when Windows would start F1 25 for it only as administrator.
pub const STUB_ELEVATION: &str = "CreateProcessW failed, error 740";

/// Whether a loader.log holds the session launcher's [`STUB_ELEVATION`] line.
pub fn stub_needs_elevation(log: &str) -> bool {
    log.lines().any(|line| {
        line.contains("[stub]")
            && line
                .split_once(STUB_ELEVATION)
                .is_some_and(|(_, rest)| !rest.starts_with(|c: char| c.is_ascii_digit()))
    })
}

/// Programs Windows won't start for an EA app copy's session because each is
/// set to run as administrator (error 740). The EA app starts F1 25 and the
/// session launcher without administrator rights, whether or not Swofty
/// Launcher has them, so the fix is to untick the setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAsAdmin {
    /// Each program, as the path to right-click.
    pub exes: Vec<PathBuf>,
    /// F1 25 or the session launcher is one of them.
    pub game: bool,
}

impl RunAsAdmin {
    pub const TITLE: &'static str = "A program is set to run as administrator";

    /// The programs in order, each once however its path is spelled.
    pub fn new(exes: impl IntoIterator<Item = PathBuf>, game: bool) -> Self {
        let key = |p: &Path| p.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
        let mut out: Vec<PathBuf> = Vec::new();
        for exe in exes {
            if !out.iter().any(|o| key(o) == key(&exe)) {
                out.push(exe);
            }
        }
        Self { exes: out, game }
    }

    pub fn names(&self) -> Vec<String> {
        self.exes
            .iter()
            .map(|p| {
                let path = p.to_string_lossy();
                path.rsplit(['\\', '/']).next().unwrap_or(&path).to_owned()
            })
            .collect()
    }

    pub fn paths(&self) -> Vec<String> {
        self.exes.iter().map(|p| p.display().to_string()).collect()
    }

    /// One line for the launcher's status bar.
    pub fn headline(&self) -> String {
        match self.names().as_slice() {
            [name] => format!("Windows won't start {name}: it is set to run as administrator"),
            names => format!(
                "Windows won't start {}: they are set to run as administrator",
                names.join(", ")
            ),
        }
    }

    pub fn text(&self) -> String {
        let (names, paths) = (self.names(), self.paths());
        let (what, fix) = match (names.as_slice(), paths.as_slice()) {
            ([name], [path]) => (
                format!(
                    "{name} is set to run as administrator, so Windows won't start it for an EA app session (error 740)."
                ),
                format!("Right-click {path}, open {FIX}"),
            ),
            _ => (
                format!(
                    "These files are set to run as administrator, so Windows won't start them for an EA app session \
                     (error 740): {}.",
                    paths.join(", ")
                ),
                format!("For each one, right-click it, open {FIX}"),
            ),
        };
        let note = if self.game {
            format!(" {GAME_NOTE}")
        } else {
            String::new()
        };
        format!("{what}{note} {fix}")
    }
}

const FIX: &str = "Properties > Compatibility and untick \"Run this program as an administrator\", also under \
                   \"Change settings for all users\". Then press Play again.";
const GAME_NOTE: &str = "The EA app starts F1 25 without administrator rights, so running Swofty Launcher as administrator doesn't help.";

/// Whether Swofty Launcher and the EA app run as administrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elevation {
    pub launcher: bool,
    pub ea_app: bool,
}

impl Elevation {
    pub fn now() -> Self {
        let launcher = elevated();
        let ea_app = match crate::diagnostics::ea_launch::ea_app_elevation() {
            Some(Some(elevated)) => elevated,
            // A normal process can't read an elevated one.
            Some(None) => !launcher,
            // Swofty starts it with the signed-in user's own rights.
            None => false,
        };
        Self { launcher, ea_app }
    }
}

fn admin_on(listings: &[(String, bool)], exe: &Path) -> Vec<AdminLayer> {
    listings
        .iter()
        .flat_map(|(listing, all_users)| matching_with(listing, *all_users, exe, has_run_as_admin))
        .collect()
}

/// The run-as-administrator settings on any program named one of `names`.
fn admin_named(listings: &[(String, bool)], names: &[&str]) -> Vec<PathBuf> {
    listings
        .iter()
        .flat_map(|(listing, _)| parse_reg_query(listing))
        .filter(|(stored, layers)| {
            let file = stored.rsplit(['\\', '/']).next().unwrap_or(stored);
            has_run_as_admin(layers) && names.iter().any(|n| n.eq_ignore_ascii_case(file))
        })
        .map(|(stored, _)| PathBuf::from(stored))
        .collect()
}

/// What stops an EA app copy's session in `game` before it starts: the
/// program that hands the EA app the launch link, set to run as administrator
/// while Swofty Launcher isn't, or, for a modded session, F1 25 or the
/// session launcher set to while the EA app isn't.
pub fn ea_blocker(game: &Path, handoff: Option<&Path>, modded: bool) -> Option<RunAsAdmin> {
    ea_blocker_in(&listings(), game, handoff, modded, Elevation::now)
}

/// [`ea_blocker`] from the listings given; `elevation` is asked only when a
/// setting is found.
pub fn ea_blocker_in(
    listings: &[(String, bool)],
    game: &Path,
    handoff: Option<&Path>,
    modded: bool,
    elevation: impl FnOnce() -> Elevation,
) -> Option<RunAsAdmin> {
    let starter = handoff.map(|h| admin_on(listings, h)).unwrap_or_default();
    let session: Vec<AdminLayer> = match modded {
        true => SESSION_EXES
            .iter()
            .flat_map(|exe| admin_on(listings, &game.join(exe)))
            .collect(),
        false => Vec::new(),
    };
    if starter.is_empty() && session.is_empty() {
        return None;
    }
    let now = elevation();
    let mut exes = Vec::new();
    if !starter.is_empty() && !now.launcher {
        exes.extend(starter.into_iter().map(|l| PathBuf::from(l.exe)));
        exes.extend(admin_named(listings, EA_APP_EXES));
    }
    let game_blocked = !session.is_empty() && !now.ea_app;
    if game_blocked {
        exes.extend(session.into_iter().map(|l| PathBuf::from(l.exe)));
    }
    (!exes.is_empty()).then(|| RunAsAdmin::new(exes, game_blocked))
}

/// What a start of `program` that Windows refused with error 740 names: the
/// program, and the EA app's own programs that are set to run as administrator.
pub fn refused_start(program: &Path) -> RunAsAdmin {
    refused_start_in(&listings(), program)
}

pub fn refused_start_in(listings: &[(String, bool)], program: &Path) -> RunAsAdmin {
    let named = admin_named(listings, EA_APP_EXES);
    RunAsAdmin::new(std::iter::once(program.to_path_buf()).chain(named), false)
}

/// What the session launcher's [`STUB_ELEVATION`] names: the settings on the
/// exes a modded session in `game` starts, or F1 25 when none can be read.
pub fn game_refused(game: &Path) -> RunAsAdmin {
    game_refused_in(&listings(), game)
}

pub fn game_refused_in(listings: &[(String, bool)], game: &Path) -> RunAsAdmin {
    let found: Vec<PathBuf> = SESSION_EXES
        .iter()
        .flat_map(|exe| admin_on(listings, &game.join(exe)))
        .map(|l| PathBuf::from(l.exe))
        .collect();
    match found.is_empty() {
        true => RunAsAdmin::new([game.join(GAME_EXE)], true),
        false => RunAsAdmin::new(found, true),
    }
}
