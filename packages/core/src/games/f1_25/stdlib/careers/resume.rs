//! What each career needs, as the in-game Resume Career list reads it from the session settings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Career, is_guess};
use crate::manifest::AddonManifest;
use crate::stdlib::range::Range;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumeNeed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub on: bool,
    pub at_version: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CareerNeeds {
    pub key: String,
    pub career: String,
    pub addons: Vec<ResumeNeed>,
}

fn at_version(a: &super::CareerAddon, installed: &AddonManifest) -> bool {
    match a.need.as_deref().and_then(|n| Range::parse(n).ok()) {
        Some(range) => range.matches(&installed.version),
        None => a
            .version
            .as_deref()
            .is_none_or(|made_with| made_with == installed.version.to_string()),
    }
}

/// One entry per career the save slots back, keyed by the save slot key the core sees; the addons are the
/// ones the career needs, each with whether profile `order` has it on and at the version the career was
/// made with. An addon recorded only because it was on is not a need.
pub fn resume_needs(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Vec<CareerNeeds> {
    list.iter()
        .filter(|c| !c.is_gone() && c.in_slots())
        .filter_map(|c| {
            let key = c.field("slot")?.to_owned();
            let addons = c
                .addons
                .iter()
                .filter(|a| !is_guess(a))
                .map(|a| match installed.get(&a.id) {
                    None => ResumeNeed {
                        id: a.id.clone(),
                        name: a.id.clone(),
                        version: a.version.clone().unwrap_or_default(),
                        on: false,
                        at_version: false,
                    },
                    Some(m) => ResumeNeed {
                        id: a.id.clone(),
                        name: m.name.clone(),
                        version: m.version.to_string(),
                        on: order.contains(&a.id),
                        at_version: at_version(a, m),
                    },
                })
                .collect();
            Some(CareerNeeds {
                key,
                career: c.career.clone(),
                addons,
            })
        })
        .collect()
}
