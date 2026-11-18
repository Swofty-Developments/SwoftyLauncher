use std::collections::BTreeMap;

use super::Diagnosis;

/// The newest F1 25 update the launcher knows, as `F1_25.exe`'s file
/// version: Steam build 26566308, and the EA app's copy of the same update.
pub const LATEST_GAME_VERSION: &str = "1.0.145.2472";

/// A game version's four numbers, as in `1.0.141.7865`.
pub(super) fn version_numbers(version: &str) -> Option<[u32; 4]> {
    let mut parts = version.trim().split('.');
    let mut numbers = [0; 4];
    for n in &mut numbers {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *n = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

/// Whether game version `version` is older than `than`, number by number.
/// None when either isn't a version.
pub fn older_game(version: &str, than: &str) -> Option<bool> {
    Some(version_numbers(version)? < version_numbers(than)?)
}

/// The addon whose changes failed to apply, from a build error such as
/// `applying swofty.classic-seasons: eaid://…: no entry [Name=…]`, on its
/// own or after what was being done when it failed.
pub(super) fn failed_addon(error: &str) -> Option<&str> {
    let rest = error
        .strip_prefix("applying ")
        .or_else(|| error.split_once(": applying ").map(|(_, rest)| rest))?;
    let id = rest.split([':', ' ', '\n']).next()?;
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    valid.then_some(id)
}

/// An addon whose changes failed to apply to an F1 25 older than
/// [`LATEST_GAME_VERSION`]: it needs that update. None when `error` isn't an
/// addon's changes failing, or when `version`, the game's own, is unknown or
/// not older.
pub fn outdated_game(
    error: &str,
    version: Option<&str>,
    names: &BTreeMap<String, String>,
) -> Option<Diagnosis> {
    let version = version?.trim();
    if older_game(version, LATEST_GAME_VERSION) != Some(true) {
        return None;
    }
    let id = failed_addon(error)?;
    let name = names.get(id).map_or(id, String::as_str);
    Some(Diagnosis {
        id: "outdated_game",
        title: format!("{name} needs the latest F1 25"),
        what: format!(
            "{name} needs F1 25 {LATEST_GAME_VERSION}, the latest update. Your F1 25 is {version}."
        ),
        steps: vec!["Update F1 25 in Steam or the EA app, then press Play again.".to_owned()],
    })
}

/// An F1 25 older than [`LATEST_GAME_VERSION`], before a modded launch: the
/// mod loader can't hook into it, so addons may not load. None when the
/// version is unknown or not older.
pub fn outdated_exe(version: Option<&str>) -> Option<Diagnosis> {
    let version = version?.trim();
    if older_game(version, LATEST_GAME_VERSION) != Some(true) {
        return None;
    }
    Some(Diagnosis {
        id: "outdated_exe",
        title: "F1 25 needs the latest update".to_owned(),
        what: format!(
            "Your F1 25 is {version}. Mods are made for {LATEST_GAME_VERSION}, the latest update, and may not \
             load on an older one."
        ),
        steps: vec![format!(
            "Update F1 25 in Steam or the EA app. If Steam says it is up to date, verify the game files \
             ({}), then press Play again.",
            crate::game_folder::stock::STEAM_VERIFY_URL
        )],
    })
}
