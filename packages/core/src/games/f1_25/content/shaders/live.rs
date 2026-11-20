//! What the running game reads: the resolved active config, `active.json` and the in-game toggle key.

use std::path::Path;

use swofty_shaders::{Active, Catalog, Severity};

use crate::profile::Profile;
use crate::store::Store;
use crate::{Error, Result};

use super::configs::{active, forced, name_of, read};
use super::{ConfigRef, catalog};

pub(super) const SETTINGS: &str = "settings.json";
pub(super) const DEFAULT_KEY: &str = "B";
pub(super) const NO_KEY: &str = "None";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    toggle_key: Option<String>,
}

/// The in-game on/off key, `"B"` by default; `"None"` is no key.
pub fn toggle_key(store: &Store) -> String {
    std::fs::read(store.shaders_dir().join(SETTINGS))
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .and_then(|s| s.toggle_key)
        .filter(|k| key_ok(k))
        .unwrap_or_else(|| DEFAULT_KEY.into())
}

pub(super) fn key_ok(key: &str) -> bool {
    key == NO_KEY || swofty_addon::manifest::postfx_key_ok(key)
}

pub fn set_toggle_key(store: &Store, key: &str) -> Result<()> {
    let key = key.trim();
    let key = if key.eq_ignore_ascii_case(NO_KEY) {
        NO_KEY.to_owned()
    } else {
        key.to_ascii_uppercase()
    };
    if !key_ok(&key) {
        return Err(Error::Other(format!(
            "{key} is not a key: use A-Z, 0-9, F1-F24 or {NO_KEY}"
        )));
    }
    crate::write_json(
        &store.shaders_dir().join(SETTINGS),
        &Settings {
            toggle_key: Some(key),
        },
    )
}

/// The active config resolved for the game: `active` is what `active.json` holds, `problem` says
/// why a chosen config is not running.
pub struct Resolved {
    pub active: Active,
    pub config: Option<ConfigRef>,
    pub problem: Option<String>,
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn resolve_active(store: &Store, profile: &Profile, catalog: &Catalog) -> Resolved {
    let forced = forced(store, profile);
    let (config, forced_by) = match forced {
        Some((r, addon)) => (Some(r), addon),
        None => (active(store, profile), String::new()),
    };
    let mut out = Resolved {
        active: Active {
            version: 1,
            serial: now_ms(),
            name: String::new(),
            source: String::new(),
            forced_by,
            toggle_key: toggle_key(store),
            instances: Vec::new(),
            rules: Vec::new(),
        },
        config: config.clone(),
        problem: None,
    };
    let Some(r) = config else { return out };
    out.active.source = r.to_string();
    let text = match read(store, &r) {
        Ok(t) => t,
        Err(e) => {
            out.problem = Some(e.to_string());
            return out;
        }
    };
    out.active.name = name_of(&text, &r.to_string());
    let lint = swofty_shaders::lint(&text, catalog);
    match &lint.config {
        Some(config) => {
            out.active.instances = swofty_shaders::resolve(config, catalog);
            out.active.rules = swofty_shaders::resolve_rules(config, catalog);
        }
        None => {
            let first = lint.diagnostics.iter().find(|d| d.severity == Severity::Error);
            out.problem = Some(match first {
                Some(d) => format!(
                    "{} line {}: {} ({} error(s))",
                    out.active.name,
                    d.start.line + 1,
                    d.message,
                    lint.errors()
                ),
                None => format!("{} does not pass the checks", out.active.name),
            });
        }
    }
    out
}

/// What the build and `push_live` write as `active.json`.
pub fn active_file(store: &Store, profile: &Profile) -> Active {
    resolve_active(store, profile, &catalog(store)).active
}

/// Writes `<game>/swofty/shaders/active.json` atomically, for the running game to pick up.
pub fn push_live(game_dir: &Path, active: &Active) -> Result<()> {
    crate::write_json_shared(&game_dir.join("swofty").join(ACTIVE_FILE), active)
}

pub(super) const ACTIVE_FILE: &str = "shaders/active.json";

/// The game path of the live config file. The launcher rewrites it while a session runs, so its bytes
/// differ from what the build placed; it is the launcher's own whatever it holds.
pub const LIVE_FILE: &str = "swofty/shaders/active.json";
