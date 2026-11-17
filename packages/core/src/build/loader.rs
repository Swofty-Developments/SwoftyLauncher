use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{ADDONS_DIR, Built};
use crate::builtin::{CORE_ID, loader_files};
use crate::install::LOADER_DIR;
use crate::manifest::AddonManifest;
use crate::settings::{SessionSettings, SessionUi, screen_file, screens};
use crate::store::Store;
use crate::{Error, Result};

// Page screens have no file: the ui build generates them.
pub fn screen_files(order: &[&AddonManifest], addons_dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
    Ok(screens(order.iter().copied())?
        .into_iter()
        .filter(|(_, s)| s.page.is_none())
        .map(|(m, s)| {
            (
                format!("{LOADER_DIR}/{}", screen_file(&s.name)),
                addons_dir.join(&m.id).join(&s.file),
            )
        })
        .collect())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plugins {
    pub files: BTreeMap<String, PathBuf>,
    pub dlls: Vec<String>,
}

pub fn plugin_files<'a>(
    store: &Store,
    order: impl IntoIterator<Item = &'a AddonManifest>,
    taken: impl IntoIterator<Item = String>,
) -> Result<Plugins> {
    let mut out = Plugins::default();
    let mut names: BTreeMap<String, String> = taken
        .into_iter()
        .map(|n| (n.to_ascii_lowercase(), CORE_ID.to_owned()))
        .collect();
    for m in order
        .into_iter()
        .filter(|m| !m.operations.native.is_empty() && !store.is_builtin(&m.id))
    {
        let dir = store.addons_dir().join(&m.id);
        for n in &m.operations.native {
            let name = n.dll.rsplit('/').next().unwrap_or(&n.dll);
            if let Some(other) = names.insert(name.to_ascii_lowercase(), m.id.clone()) {
                return Err(Error::Other(format!(
                    "{}: plugin {name} has the same file name as a plugin of {other}; rename one",
                    m.id
                )));
            }
            out.dlls.push(format!("{ADDONS_DIR}/{}/{}", m.id, n.dll));
            for rel in std::iter::once(&n.dll).chain(&n.files) {
                let rel = rel.trim_end_matches('/');
                let src = dir.join(rel);
                let found = if src.is_dir() {
                    loader_files(&src)?
                        .into_iter()
                        .filter(|(r, _)| !r.split('/').any(|p| p.starts_with('.')))
                        .map(|(r, p)| (format!("{rel}/{r}"), p))
                        .collect()
                } else if src.is_file() {
                    BTreeMap::from([(rel.to_owned(), src)])
                } else {
                    return Err(Error::Other(format!(
                        "{}: {rel} is missing; build the plugin and install the addon again",
                        m.id
                    )));
                };
                for (rel, src) in found {
                    out.files
                        .insert(format!("{LOADER_DIR}/{ADDONS_DIR}/{}/{rel}", m.id), src);
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn loader_line(exe_dir: Option<&Path>, loader: Option<&Path>) -> String {
    let show = |p: Option<&Path>| p.map_or_else(|| "none".to_owned(), |p| p.display().to_string());
    format!("loader: {}; swofty.exe in {}", show(loader), show(exe_dir))
}

pub fn build_menus(
    built: &mut Built,
    session: &mut SessionSettings,
    original: &dyn Fn(&str) -> PathBuf,
    writer: &crate::ui::Writer,
    lang: bool,
) -> Result<Vec<(String, String)>> {
    let input = |rel: &str| built.plan.data.get(rel).cloned().unwrap_or_else(|| original(rel));
    let writer = |erp: &str, res: &str| {
        built
            .plan
            .data
            .contains_key(erp)
            .then(|| writer(erp, res))
            .flatten()
    };
    let menus = crate::ui::build(session, &input, &writer, &built.overlay, lang)?;
    built.lines.push(format!(
        "ui: ADDON SETTINGS with {} screen(s) under {LOADER_DIR}/ui, {} game archive(s) patched",
        menus.session.len(),
        menus.erps.len()
    ));
    if !menus.strings.is_empty() {
        built.lines.push(format!(
            "ui: the online services popup reads its text from {} language string(s)",
            menus.strings.len()
        ));
    }
    built.lines.extend(menus.lines);
    built.warnings.extend(menus.warnings);
    built.plan.data.extend(menus.erps);
    for (rel, src) in menus.screens {
        if built.plan.loader.insert(rel.clone(), src).is_some() {
            return Err(Error::Other(format!("{rel} is placed twice")));
        }
    }
    let ui = session.ui.get_or_insert_with(|| SessionUi {
        register: true,
        strict: false,
        recover: true,
        screens: Vec::new(),
    });
    if let Some(s) = ui
        .screens
        .iter()
        .find(|s| menus.session.iter().any(|m| m.name == s.name))
    {
        return Err(Error::Other(format!(
            "screen {} is reserved for ADDON SETTINGS; rename it in its addon",
            s.name
        )));
    }
    ui.screens.retain(|s| s.page.is_none());
    ui.screens.splice(0..0, menus.session);
    Ok(menus.strings)
}
