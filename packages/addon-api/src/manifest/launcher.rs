use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::launcher::CommandSpec;
use crate::manifest::options::AddonOption;
use crate::manifest::validate::valid_screen_name;
use crate::view::SectionSpec;

/// Most options a hub tab shows: one per cell of the laptop's stat grid.
pub const HUB_TAB_OPTIONS: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HubTab {
    pub name: String,
    pub title: String,
    pub options: Vec<String>,
    pub data_group: String,
}

impl HubTab {
    pub fn template(&self) -> String {
        format!("swofty_hub_{}", self.name)
    }

    pub fn view_model(&self) -> String {
        format!("view_model_data.hud_swofty_hub_{}", self.name)
    }

    pub(crate) fn validate(&self, options: &[AddonOption]) -> std::result::Result<(), String> {
        if !valid_screen_name(&self.name) || self.name.len() > 40 {
            return Err(format!(
                "name {:?} must be [a-z0-9_], at most 40 characters",
                self.name
            ));
        }
        if self.title.trim().is_empty() {
            return Err("title is empty".into());
        }
        if !valid_screen_name(&self.data_group) {
            return Err(format!("data_group {:?} must be [a-z0-9_]", self.data_group));
        }
        if self.options.is_empty() || self.options.len() > HUB_TAB_OPTIONS {
            return Err(format!("options must name 1 to {HUB_TAB_OPTIONS} options"));
        }
        own_options(&self.options, options)
    }
}

fn own_options(ids: &[String], options: &[AddonOption]) -> std::result::Result<(), String> {
    for (i, id) in ids.iter().enumerate() {
        if !options.iter().any(|o| &o.id == id) {
            return Err(format!("option {id} is not one of the addon's options"));
        }
        if ids[..i].contains(id) {
            return Err(format!("option {id} is listed twice"));
        }
    }
    Ok(())
}

/// The wizard screen a career setup page's CONFIRM opens unless `next` names another.
pub const CAREER_SETUP_NEXT: &str = "career_wizard_season_length_select";

/// The screen a career setup page's Back opens.
pub const CAREER_SETUP_BACK: &str = "fe_career_dlc_select";

/// Longest `career_setup.route`.
pub const CAREER_SETUP_ROUTE_LEN: usize = 48;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CareerSetup {
    pub route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub options: Vec<String>,
    #[serde(default = "career_setup_next", skip_serializing_if = "is_career_setup_next")]
    pub next: String,
}

fn career_setup_next() -> String {
    CAREER_SETUP_NEXT.into()
}

fn is_career_setup_next(next: &String) -> bool {
    next == CAREER_SETUP_NEXT
}

impl CareerSetup {
    pub fn screen(&self) -> String {
        format!("swofty_setup_{}", self.route)
    }

    pub fn back_route(&self) -> String {
        format!("back_{}", self.route)
    }

    pub(crate) fn validate(&self, options: &[AddonOption]) -> std::result::Result<(), String> {
        if !valid_screen_name(&self.route) || self.route.len() > CAREER_SETUP_ROUTE_LEN {
            return Err(format!(
                "route {:?} must be [a-z0-9_], at most {CAREER_SETUP_ROUTE_LEN} characters",
                self.route
            ));
        }
        if !valid_screen_name(&self.next) {
            return Err(format!("next {:?} must be [a-z0-9_]", self.next));
        }
        if self.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Err("title is empty".into());
        }
        if self.options.is_empty() {
            return Err("options names no option".into());
        }
        own_options(&self.options, options)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Saves {
    #[serde(default)]
    pub affects: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Saves {
    pub(crate) fn is_unset(&self) -> bool {
        *self == Self::default()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref().map(str::trim).filter(|m| !m.is_empty())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    pub program: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SectionSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub build: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandSpec>,
    // Ask the helper which dropped files and folders it takes, by running it as
    // `<program> claims <path>`, instead of matching `imports` against extensions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub claims: bool,
    // Send the addon's first section a `startup` event once per launcher start.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub startup: bool,
}

impl Launcher {
    pub fn program(&self) -> Option<&str> {
        self.program.get(std::env::consts::OS).map(String::as_str)
    }
}
