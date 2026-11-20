//! The compatibility layers Windows keeps for an exe under `AppCompatFlags\Layers`: the ones set on the game
//! and on the programs that start it, and why they stop a modded session.

use std::path::Path;

use super::elevation::elevated;
use super::{SESSION_EXES, STARTERS, listings, parse_reg_query};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminLayer {
    pub all_users: bool,
    /// The file name of the exe the session starts.
    pub name: String,
    /// The exe path as the setting stores it.
    pub exe: String,
    /// The layers set on it, e.g. `~ RUNASADMIN`.
    pub layers: String,
}

impl AdminLayer {
    pub fn scope(&self) -> &'static str {
        if self.all_users {
            "for all users"
        } else {
            "for this user"
        }
    }
}

pub fn has_run_as_admin(layers: &str) -> bool {
    layers
        .split_whitespace()
        .any(|l| l.eq_ignore_ascii_case("RUNASADMIN"))
}

/// The setting keeps the path as Explorer saw it, which need not be the
/// spelling the launcher found (case, separators, a linked library folder).
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\");
    norm(a).eq_ignore_ascii_case(&norm(b))
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Whether `layers` sets any compatibility layer, not just the `~` marker.
pub fn has_layers(layers: &str) -> bool {
    layers.split_whitespace().any(|l| l != "~")
}

#[cfg(test)]
pub(super) fn matching(listing: &str, all_users: bool, exe: &Path) -> Vec<AdminLayer> {
    matching_with(listing, all_users, exe, has_run_as_admin)
}

pub(super) fn matching_with(
    listing: &str,
    all_users: bool,
    exe: &Path,
    keep: fn(&str) -> bool,
) -> Vec<AdminLayer> {
    let path = exe.to_string_lossy();
    let name = path.rsplit(['\\', '/']).next().unwrap_or(&path);
    parse_reg_query(listing)
        .into_iter()
        .filter(|(stored, data)| keep(data) && same_file(Path::new(stored), exe))
        .map(|(stored, layers)| AdminLayer {
            all_users,
            name: name.to_owned(),
            exe: stored,
            layers,
        })
        .collect()
}

/// Every run-as-administrator setting on `exe`, per user first.
pub fn admin_layers(exe: &Path) -> Vec<AdminLayer> {
    layer_settings(exe, has_run_as_admin)
}

/// Every compatibility setting on `exe` (the Compatibility tab of its
/// Properties, run as administrator included), per user first.
pub fn layers(exe: &Path) -> Vec<AdminLayer> {
    layer_settings(exe, has_layers)
}

pub(super) fn layer_settings(exe: &Path, keep: fn(&str) -> bool) -> Vec<AdminLayer> {
    listings()
        .into_iter()
        .flat_map(|(listing, all_users)| matching_with(&listing, all_users, exe, keep))
        .collect()
}

/// The compatibility settings on the programs that start the game in `game`:
/// [`STARTERS`], and the session launcher in the game folder.
pub fn starter_layers(game: &Path) -> Vec<AdminLayer> {
    starter_layers_in(&listings(), game)
}

/// [`starter_layers`] from the listings given, in [`STARTERS`] order with the
/// session launcher last, per user first.
pub fn starter_layers_in(listings: &[(String, bool)], game: &Path) -> Vec<AdminLayer> {
    let stub = game.join(SESSION_EXES[1]);
    let mut found: Vec<AdminLayer> = listings
        .iter()
        .flat_map(|(listing, all_users)| {
            parse_reg_query(listing)
                .into_iter()
                .filter(|(_, data)| has_layers(data))
                .filter_map(|(stored, layers)| {
                    let file = stored.rsplit(['\\', '/']).next().unwrap_or(&stored);
                    let name = STARTERS
                        .iter()
                        .find(|n| n.eq_ignore_ascii_case(file))
                        .map(|n| (*n).to_owned())
                        .or_else(|| {
                            same_file(Path::new(&stored), &stub).then(|| SESSION_EXES[1].to_owned())
                        })?;
                    Some(AdminLayer {
                        all_users: *all_users,
                        name,
                        exe: stored,
                        layers,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let rank = |l: &AdminLayer| {
        STARTERS
            .iter()
            .position(|n| *n == l.name)
            .unwrap_or(STARTERS.len())
    };
    found.sort_by_key(|l| (rank(l), l.all_users));
    found
}

/// The settings on the exes a modded session in `game` starts.
pub fn session_admin_layers(game: &Path) -> Vec<AdminLayer> {
    SESSION_EXES
        .iter()
        .flat_map(|exe| admin_layers(&game.join(exe)))
        .collect()
}

/// Why a modded session in `game` cannot start, when one of its exes is set
/// to run as administrator and this process is not elevated.
pub fn elevation_blocker(game: &Path) -> Option<String> {
    let found = session_admin_layers(game);
    if found.is_empty() || elevated() {
        return None;
    }
    Some(explain(&found, game))
}

pub fn explain(found: &[AdminLayer], game: &Path) -> String {
    let mut names: Vec<&str> = found.iter().map(|l| l.name.as_str()).collect();
    names.dedup();
    let (is, it) = if names.len() > 1 {
        ("are", "them")
    } else {
        ("is", "it")
    };
    let names = names.join(" and ");
    let all_users = found.iter().any(|l| l.all_users);
    format!(
        "{names} {is} set to run as administrator ({}), and Windows will not start {it} for a modded \
         session while Swofty Launcher runs unelevated (error 740). Right-click {names} in {}, \
         open Properties > Compatibility and untick \"Run this program as an administrator\"{}, \
         then press Play again, or run Swofty Launcher as administrator",
        if all_users {
            "for all users"
        } else {
            "for this user"
        },
        game.display(),
        if all_users {
            " under \"Change settings for all users\""
        } else {
            ""
        },
    )
}
