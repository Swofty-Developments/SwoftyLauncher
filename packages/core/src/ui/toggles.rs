//! The session settings that turn the launcher's menu changes on and off, and the core version the menus
//! show.

use crate::addons::builtin::CORE_ID;
use crate::addons::settings::SessionSettings;

pub(super) fn panel_shown(settings: &SessionSettings) -> bool {
    core_toggle(settings, "show_launcher_panel")
}

// SwoftyMod's split_season_picker: off leaves the era picker with the game's single row.
pub(super) fn season_picker_split(settings: &SessionSettings) -> bool {
    core_toggle(settings, "split_season_picker")
}

// A SwoftyMod toggle, on unless the profile turns it off.
pub(super) fn core_toggle(settings: &SessionSettings, id: &str) -> bool {
    settings
        .addons
        .iter()
        .filter(|m| m.id == CORE_ID)
        .flat_map(|m| &m.options)
        .find(|o| o.id == id)
        .and_then(|o| o.states.get(o.current))
        .is_none_or(|s| s.value != serde_json::Value::Bool(false))
}
