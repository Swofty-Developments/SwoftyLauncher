//! Changes the in-game settings screen asks for, merged into the profile.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::AddonManifest;
use crate::profile::Profile;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Requests {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub options: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,
    #[serde(default)]
    pub relaunch: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Merge {
    pub changed: Vec<String>,
    pub skipped: Vec<String>,
    pub relaunch: bool,
}

pub fn merge(profile: &mut Profile, installed: &BTreeMap<String, AddonManifest>, req: &Requests) -> Merge {
    let mut out = Merge {
        relaunch: req.relaunch,
        ..Merge::default()
    };
    for (addon_id, values) in &req.options {
        let Some(m) = installed.get(addon_id) else {
            out.skipped.push(format!("{addon_id} is not installed"));
            continue;
        };
        for (id, v) in values {
            match profile.set_option(m, id, v.clone()) {
                Ok(true) => {
                    let text = m.options.iter().find(|o| &o.id == id).map(|o| o.text(v));
                    out.changed
                        .push(format!("{addon_id}.{id} = {}", text.unwrap_or_default()));
                }
                Ok(false) => {}
                Err(e) => out.skipped.push(e.to_string()),
            }
        }
    }
    for (addon_id, &on) in &req.enabled {
        if !installed.contains_key(addon_id) {
            out.skipped.push(format!("{addon_id} is not installed"));
            continue;
        }
        if profile.enabled().any(|e| e == addon_id) == on {
            continue;
        }
        match profile.set_enabled(addon_id, on) {
            Ok(()) => out
                .changed
                .push(format!("{addon_id} {}", if on { "enabled" } else { "disabled" })),
            Err(e) => out.skipped.push(e.to_string()),
        }
    }
    out
}
