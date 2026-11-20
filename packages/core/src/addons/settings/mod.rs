mod render;
mod requests;
mod session;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::manifest::{AddonManifest, AddonNeed, AddonOption};
use crate::profile::Profile;

pub use render::{render_ops, render_text};
pub use requests::{Merge, Requests, merge};
pub use session::{
    SessionAddon, SessionMap, SessionMfd, SessionMod, SessionOption, SessionPlainSaves, SessionSaves,
    SessionScreen, SessionSettings, SessionState, SessionUi, screen_file, screens,
};
pub use swofty_addon::option::{MAX_CHOICES, MAX_ID_LEN, MAX_SLIDER_STEPS, State, valid_option_id};

pub const SCHEMA: u32 = 1;
pub const SETTINGS_FILE: &str = "swofty/settings.json";
pub const RESTART_COMMAND: &str = "swofty_restart";
pub const UI_DIR: &str = "swofty/ui";

/// The addons on in a build, each with its version: what an op's `when.addon` and an option's `needs`
/// are checked against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnAddons {
    on: BTreeMap<String, semver::Version>,
    off: BTreeSet<(String, String)>,
}

impl OnAddons {
    pub fn new<'a>(on: impl IntoIterator<Item = &'a AddonManifest>) -> Self {
        Self {
            on: on
                .into_iter()
                .map(|m| (m.id.clone(), m.version.clone()))
                .collect(),
            off: BTreeSet::new(),
        }
    }

    /// Turns toggle `option` of `addon` off for this build, whatever the profile holds.
    pub fn force_off(&mut self, addon: &str, option: &str) {
        self.off.insert((addon.to_owned(), option.to_owned()));
    }

    /// The installed addons the profile has on.
    pub fn of(profile: &Profile, installed: &BTreeMap<String, AddonManifest>) -> Self {
        Self::new(profile.enabled().filter_map(|id| installed.get(id)))
    }

    pub fn has(&self, need: &AddonNeed) -> bool {
        self.on
            .get(&need.id)
            .is_some_and(|v| crate::addons::version::satisfies(&need.version, v))
    }

    /// The first addon the option needs that is off, or on at a version out of its range.
    pub fn missing<'a>(&self, o: &'a AddonOption) -> Option<&'a AddonNeed> {
        o.needs.iter().find(|n| !self.has(n))
    }

    /// The option's value in a build: the profile's, or off while an addon it needs is off or the build
    /// turned it off.
    pub fn value(&self, profile: &Profile, m: &AddonManifest, o: &AddonOption) -> Value {
        match self.missing(o).is_some() || self.off.contains(&(m.id.clone(), o.id.clone())) {
            true => Value::Bool(false),
            false => profile.option(m, o),
        }
    }
}

pub fn djb2(s: &str) -> u32 {
    s.bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(u32::from(b)))
}
