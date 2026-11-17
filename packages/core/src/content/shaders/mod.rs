//! Shader configs and effects in the store (DESIGN.md §1, §5, §6): personal configs in
//! `<data>/shaders/`, addon configs and effects from installed addons, the active config per profile,
//! and the files the game reads.

mod compile;
mod configs;
mod live;

#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use swofty_shaders::Origin;

use crate::addons::builtin::CORE_ID;
use crate::profile::Profile;
use crate::store::Store;
use crate::{Error, Result};

pub use compile::{BuildFiles, build_files, compile_all};
pub use configs::{
    active, copy_to_personal, create, delete, export, forced, import, list, read, rename, save, set_active,
};
pub use live::{LIVE_FILE, Resolved, active_file, push_live, resolve_active, set_toggle_key, toggle_key};
pub use swofty_shaders::{Active, ActiveInstance, Catalog, CompiledEffect, Effect, Target};

mod table {
    include!(concat!(env!("OUT_DIR"), "/shader_library.rs"));
}

const LIBRARY_EFFECTS: &str = "effects/";
const STARTER: &str = "starter.swfx";

/// The files of the core's effect library, by path under `addons/swoftymod/shaders/`: the
/// `effects/<id>.effect.json` declarations with their HLSL and images, and `starter.swfx`.
pub fn library() -> &'static [(&'static str, &'static [u8])] {
    table::FILES
}

/// The text "New config" starts from.
pub fn starter() -> Option<&'static str> {
    library()
        .iter()
        .find(|(path, _)| *path == STARTER)
        .and_then(|(_, bytes)| std::str::from_utf8(bytes).ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConfigRef {
    /// `personal:<slug>`: `<data>/shaders/<slug>.swfx`.
    Personal(String),
    /// `addon:<id>/<file stem>`.
    Addon { addon: String, file: String },
}

impl fmt::Display for ConfigRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Personal(slug) => write!(f, "personal:{slug}"),
            Self::Addon { addon, file } => write!(f, "addon:{addon}/{file}"),
        }
    }
}

impl FromStr for ConfigRef {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some(slug) = s.strip_prefix("personal:") {
            return Ok(Self::Personal(slug.into()));
        }
        if let Some((addon, file)) = s.strip_prefix("addon:").and_then(|r| r.split_once('/')) {
            return Ok(Self::Addon {
                addon: addon.into(),
                file: file.into(),
            });
        }
        Err(Error::Other(format!("{s:?} is not a shader config")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEntry {
    pub r#ref: ConfigRef,
    pub name: String,
    /// "Personal", or the addon's name.
    pub source: String,
    pub editable: bool,
    pub path: PathBuf,
}

/// Where a catalog effect's HLSL and images live.
#[derive(Debug, Clone)]
enum Home {
    Library,
    Addon(PathBuf),
}

struct Loaded {
    catalog: Catalog,
    homes: BTreeMap<String, Home>,
    warnings: Vec<String>,
}

/// Installed addons that ship shader configs or effects, in load order: the profile's order, then
/// the rest by id.
fn shader_addons(store: &Store, profile: Option<&Profile>) -> Vec<swofty_addon::manifest::AddonManifest> {
    let Ok(mut all) = store.addons() else {
        return Vec::new();
    };
    all.remove(CORE_ID);
    all.retain(|id, _| !store.is_builtin(id));
    let mut out = Vec::new();
    if let Some(p) = profile {
        for a in &p.addons {
            if let Some(m) = all.remove(&a.id) {
                out.push(m);
            }
        }
    }
    out.extend(all.into_values());
    out.retain(|m| !m.shaders.is_empty() || !m.effects.is_empty());
    out
}

fn active_profile(store: &Store) -> Option<Profile> {
    store.profile(&store.active_profile()).ok()
}

fn load(store: &Store) -> Loaded {
    let mut out = Loaded {
        catalog: Catalog::default(),
        homes: BTreeMap::new(),
        warnings: Vec::new(),
    };
    let add = |out: &mut Loaded, what: String, json: &str, origin: Origin, home: Home| {
        match swofty_shaders::parse_effect(json, origin) {
            Ok(effect) => {
                if let Some(first) = out.catalog.get(&effect.id) {
                    out.warnings.push(format!(
                        "{what}: the effect {} is ignored; {} already provides it",
                        effect.id,
                        match &first.origin {
                            Origin::Library => "the library".to_owned(),
                            Origin::Addon { name, .. } => name.clone(),
                        }
                    ));
                    return;
                }
                out.warnings
                    .extend(effect.warnings.iter().map(|w| format!("{what}: {w}")));
                out.homes.insert(effect.id.clone(), home);
                out.catalog.effects.push(effect);
            }
            Err(e) => out.warnings.push(format!("{what}: {e}")),
        }
    };
    for (path, bytes) in library() {
        if path.starts_with(LIBRARY_EFFECTS) && path.ends_with(".effect.json") {
            match std::str::from_utf8(bytes) {
                Ok(json) => add(
                    &mut out,
                    format!("library {path}"),
                    json,
                    Origin::Library,
                    Home::Library,
                ),
                Err(_) => out.warnings.push(format!("library {path}: not UTF-8")),
            }
        }
    }
    let profile = active_profile(store);
    for m in shader_addons(store, profile.as_ref()) {
        let root = store.addons_dir().join(&m.id);
        for rel in &m.effects {
            let path = root.join(rel);
            let what = format!("{}: {rel}", m.name);
            match std::fs::read_to_string(&path) {
                Ok(json) => {
                    let origin = Origin::Addon {
                        id: m.id.clone(),
                        name: m.name.clone(),
                    };
                    let dir = path.parent().unwrap_or(&root).to_path_buf();
                    add(&mut out, what, &json, origin, Home::Addon(dir));
                }
                Err(e) => out.warnings.push(format!("{what}: {e}")),
            }
        }
    }
    out
}

/// The library's effects and every installed addon's, whether the addon is on or off.
pub fn catalog(store: &Store) -> Catalog {
    load(store).catalog
}
