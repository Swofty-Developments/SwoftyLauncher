//! The `swofty/settings.json` document the in-game core reads: each addon's options, its screens and the commands that set them.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::content::track_package::TrackPackage;
use crate::game_db::owned::OwnedItem;
use crate::manifest::{AddonManifest, Binds, CareerSetup, HubTab, OptionType, ScreenOp};
use crate::profile::Profile;
use crate::{Error, Result};

use super::{OnAddons, RESTART_COMMAND, SCHEMA, djb2};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSettings {
    pub schema: u32,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<String>,
    pub restart_command: String,
    pub addons: Vec<SessionAddon>,
    #[serde(default)]
    pub maps: Vec<SessionMap>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<SessionMod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<SessionUi>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_items: Vec<OwnedItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub early_plugins: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<String>,
    /// Present when the profile uses launcher saves: the core serves the game's profile save from `remote`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher_saves: Option<SessionSaves>,
    /// The plain store the core writes the profile's save containers to and reads put-backs from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_saves: Option<SessionPlainSaves>,
    /// What each career in the save slots needs and whether this session has it: the in-game Resume Career list gates on it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_needs: Vec<crate::stdlib::careers::CareerNeeds>,
    /// The rows of the in-game Resume Career list screen, newest first; the core maps a pressed row's index to
    /// its save slot by `key`. Empty when the list screen is off (`SWOFTY_RESUME_LIST=0`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_list: Vec<crate::stdlib::careers::ResumeRow>,
    /// The Career list screen is built, even with no careers to list (it then holds the New Career row alone).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resume_list_on: bool,
    /// The places (home, career) with an enabled ui.tile record that names an image: the UI build adds the image
    /// templates and writes `ui\tile_images.flag` only when there is one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tile_images: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionPlainSaves {
    pub root: String,
    pub schema: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSaves {
    pub profile: String,
    pub remote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionUi {
    pub register: bool,
    pub strict: bool,
    pub recover: bool,
    pub screens: Vec<SessionScreen>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<SessionRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRoute {
    pub id: String,
    pub addon: String,
    pub file: String,
    pub clone_of: String,
    pub opens: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionScreen {
    pub name: String,
    /// `options` for a screen the launcher generates from its addon's options page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The addon a generated page shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addon: Option<String>,
    pub file: String,
    pub flow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default)]
    pub view_model: bool,
}

pub fn screen_file(name: &str) -> String {
    format!("ui/{name}.ui")
}

pub fn screens<'a>(
    order: impl IntoIterator<Item = &'a AddonManifest>,
) -> Result<Vec<(&'a AddonManifest, &'a ScreenOp)>> {
    let mut out: Vec<(&AddonManifest, &ScreenOp)> = Vec::new();
    for m in order {
        for s in &m.operations.screens {
            if let Some((other, _)) = out.iter().find(|(_, o)| o.name == s.name) {
                return Err(Error::Other(format!(
                    "screen {} is declared by {} and {}",
                    s.name, other.id, m.id
                )));
            }
            out.push((m, s));
        }
    }
    Ok(out)
}

pub fn routes<'a>(order: impl IntoIterator<Item = &'a AddonManifest>) -> Result<Vec<SessionRoute>> {
    let mut out: Vec<SessionRoute> = Vec::new();
    for m in order {
        for r in &m.operations.routes {
            if let Some(other) = out.iter().find(|o| o.id == r.id) {
                return Err(Error::Other(format!(
                    "route {} is declared by {} and {}",
                    r.id, other.addon, m.id
                )));
            }
            out.push(SessionRoute {
                id: r.id.clone(),
                addon: m.id.clone(),
                file: r.file.clone(),
                clone_of: r.clone_of.clone(),
                opens: r.opens.clone(),
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMap {
    pub code: String,
    pub name: String,
    /// The addon that brings the track package: its ADDON SETTINGS tile shows the track.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub addon: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMod {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionAddon {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub enabled: bool,
    pub locked: bool,
    /// A track addon: its category is Tracks or it brings a track package, as the launcher shows it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub track: bool,
    /// The launcher folder the addon is in, by name; ADDON SETTINGS lists a row per folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default)]
    pub dlls: Vec<String>,
    pub enable_commands: [String; 2],
    #[serde(default)]
    pub options: Vec<SessionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub career_setup: Option<CareerSetup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_tab: Option<HubTab>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfd: Option<SessionMfd>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMfd {
    pub title: String,
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionOption {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: OptionType,
    pub binds: Binds,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub current: usize,
    pub states: Vec<SessionState>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    pub value: Value,
    pub text: String,
    pub command: String,
}

impl SessionSettings {
    /// The build's track packages, each with the addon that brings it, which is then a track.
    pub fn set_maps<'a>(&mut self, packages: impl Iterator<Item = (&'a TrackPackage, &'a str)>) {
        self.maps = packages
            .map(|(p, addon)| SessionMap {
                code: p.track_code.clone(),
                name: p.track_name.clone().unwrap_or_else(|| p.track_code.clone()),
                addon: addon.to_owned(),
                version: p.version.clone().unwrap_or_default(),
                author: p.author_name.clone().unwrap_or_default(),
                description: p
                    .extra
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect();
        for a in &mut self.addons {
            a.track |= self.maps.iter().any(|m| m.addon == a.id);
        }
    }

    pub fn set_mods(&mut self, items: impl Iterator<Item = SessionMod>) {
        self.mods = items.collect();
    }

    pub fn build(
        profile: &Profile,
        installed: &BTreeMap<String, AddonManifest>,
        requests: Option<String>,
    ) -> Result<Self> {
        if profile.is_vanilla() {
            return Err(Error::Other(format!(
                "{} plays without addons and has no addon settings",
                profile.name
            )));
        }
        let mut order: Vec<&AddonManifest> = profile
            .addons
            .iter()
            .filter_map(|m| installed.get(&m.id))
            .collect();
        let mut rest: Vec<&AddonManifest> = installed
            .values()
            .filter(|m| !order.iter().any(|o| o.id == m.id))
            .collect();
        rest.sort_by_key(|m| m.name.to_lowercase());
        order.extend(rest);
        let enabled: Vec<&str> = profile.enabled().collect();
        let on = OnAddons::of(profile, installed);
        let screens: Vec<SessionScreen> =
            screens(order.iter().copied().filter(|m| enabled.contains(&m.id.as_str())))?
                .into_iter()
                .map(|(m, s)| SessionScreen {
                    name: s.name.clone(),
                    page: s.page.clone(),
                    addon: s.page.as_ref().map(|_| m.id.clone()),
                    file: screen_file(&s.name),
                    flow: s.flow.clone(),
                    anchor: s.anchor.clone(),
                    view_model: s.view_model,
                })
                .collect();
        let routes = routes(order.iter().copied().filter(|m| enabled.contains(&m.id.as_str())))?;
        let ui = (!screens.is_empty() || !routes.is_empty()).then_some(SessionUi {
            register: true,
            strict: false,
            recover: true,
            screens,
            routes,
        });
        let early_plugins = order
            .iter()
            .filter(|m| enabled.contains(&m.id.as_str()))
            .flat_map(|m| &m.operations.native)
            .filter(|n| n.early)
            .map(|n| n.dll.rsplit('/').next().unwrap_or(&n.dll).to_owned())
            .collect();
        let addons = order
            .into_iter()
            .map(|m| SessionAddon {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.to_string(),
                author: m.author.clone(),
                description: m.description.clone().unwrap_or_default(),
                enabled: enabled.contains(&m.id.as_str()),
                locked: m.is_builtin() || crate::updates::managed::is_managed(&m.id),
                track: m.category == Some(crate::manifest::Category::Tracks),
                folder: None,
                dlls: m
                    .operations
                    .native
                    .iter()
                    .map(|n| n.dll.rsplit('/').next().unwrap_or(&n.dll).to_owned())
                    .collect(),
                enable_commands: [0, 1].map(|b| format!("swofty_enable:{}:{b}", m.id)),
                options: m
                    .options
                    .iter()
                    .map(|o| SessionOption {
                        id: o.id.clone(),
                        kind: o.kind,
                        binds: o.binds,
                        label: o.title().to_owned(),
                        description: o.description.clone().unwrap_or_default(),
                        current: o.index(&on.value(profile, m, o)).unwrap_or(0),
                        states: o
                            .states()
                            .into_iter()
                            .enumerate()
                            .map(|(i, s)| SessionState {
                                value: s.value,
                                text: s.text,
                                command: format!("swofty_set:{}:{}:{i}", m.id, o.id),
                            })
                            .collect(),
                        hidden: o.hidden || on.missing(o).is_some(),
                    })
                    .collect(),
                career_setup: m.career_setup.clone(),
                hub_tab: m.hub_tab.clone(),
                mfd: m.hud.as_ref().and_then(|h| h.mfd.as_ref()).map(|f| SessionMfd {
                    title: f.title.clone().unwrap_or_else(|| m.name.clone()),
                    rows: f.rows.clone(),
                }),
            })
            .collect();
        let out = Self {
            schema: SCHEMA,
            profile: profile.name.clone(),
            requests,
            restart_command: RESTART_COMMAND.into(),
            addons,
            maps: Vec::new(),
            mods: Vec::new(),
            ui,
            owned_items: Vec::new(),
            early_plugins,
            plugins: Vec::new(),
            launcher_saves: None,
            plain_saves: None,
            resume_needs: Vec::new(),
            resume_list: Vec::new(),
            resume_list_on: false,
            tile_images: Vec::new(),
        };
        out.check_commands()?;
        Ok(out)
    }

    pub fn commands(&self) -> impl Iterator<Item = &str> {
        let addons = self.addons.iter().flat_map(|m| {
            let options = m
                .options
                .iter()
                .flat_map(|o| o.states.iter().map(|s| s.command.as_str()));
            m.enable_commands.iter().map(String::as_str).chain(options)
        });
        std::iter::once(self.restart_command.as_str()).chain(addons)
    }

    fn check_commands(&self) -> Result<()> {
        let mut seen: HashMap<u32, &str> = HashMap::new();
        for c in self.commands() {
            if let Some(other) = seen.insert(djb2(c), c) {
                return Err(Error::Other(format!(
                    "commands {other} and {c} share the djb2 hash {:08x}; rename an option",
                    djb2(c)
                )));
            }
        }
        Ok(())
    }
}
