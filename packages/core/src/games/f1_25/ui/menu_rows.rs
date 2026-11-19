//! The rows the build adds to the stock main menu, career menu, customisation screen and hub tabs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use swofty_formats::erp::ErpFile;

use super::archives::{ArchiveWriter, read_frag, write_archive_copying};
use super::node::err;
use super::{Writer, ui_file};
use super::{career_select, check, home_rows, hub, patch};
use crate::addons::settings::SessionSettings;
use crate::{Error, Result};

pub(super) type CareerRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

pub(super) fn customisation_built(
    home_routes: &[u8],
    customisation_routes: &[u8],
    error: Option<String>,
) -> std::result::Result<(), String> {
    if let Some(e) = error {
        return Err(e);
    }
    if !patch::has_route(home_routes, "fe_home", patch::CUSTOMISATION_ROUTE) {
        return Err(format!("fe_home has no {} route", patch::CUSTOMISATION_ROUTE));
    }
    if !patch::has_route(
        customisation_routes,
        patch::CUSTOMISATION_GROUP,
        patch::BACK_HOME_FROM_CONTAINER,
    ) {
        return Err(format!(
            "{} has no {} route",
            patch::CUSTOMISATION_GROUP,
            patch::BACK_HOME_FROM_CONTAINER
        ));
    }
    Ok(())
}

pub(super) fn main_menu_rows(
    src: &Path,
    templates: Option<&[u8]>,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let scene = read_frag(src, home_rows::SCENE, 0)?;
    let templates = match templates {
        Some(t) => t.to_vec(),
        None => read_frag(src, home_rows::TEMPLATES, home_rows::TEMPLATES_FRAGMENT)?,
    };
    let built = home_rows::has_slots(&scene)
        .and_then(|a| home_rows::has_templates(&templates).map(|b| a || b))
        .and_then(|taken| match taken {
            true => Ok(None),
            false => Ok(Some((
                home_rows::build_scene(&scene)?,
                home_rows::build_templates(&templates)?,
            ))),
        });
    match built {
        Ok(Some(edit)) => {
            lines.push("ui: the main menu gets rows of cards".to_owned());
            Ok(Some(edit))
        }
        Ok(None) => {
            lines.push("ui: the main menu already has rows; left as it is".to_owned());
            Ok(None)
        }
        Err(e) => {
            warnings.push(format!("ui: the main menu keeps its stock list ({e})"));
            Ok(None)
        }
    }
}

pub(super) fn career_row(
    src: &Path,
    writer: &Writer,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Option<CareerRow>> {
    let ui = read_frag(src, career_select::UI, 0)?;
    let type_ui = read_frag(src, career_select::TYPE_UI, 0)?;
    let scene = read_frag(src, career_select::SCENE, 0)?;
    let templates = read_frag(src, career_select::TEMPLATES, career_select::TEMPLATES_FRAGMENT)?;
    let edit = career_select::taken(&ui, &scene, &templates).and_then(|taken| match taken {
        true => Ok(None),
        false => Ok(Some((
            career_select::build_ui(&ui)?,
            career_select::build_type_ui(&type_ui)?,
            career_select::build_scene(&scene)?,
            career_select::build_templates(&templates)?,
        ))),
    });
    let edit = match edit {
        Ok(edit) => edit,
        Err(e) => {
            let rel = ui_file(career_select::ARCHIVE);
            let by = [
                career_select::UI,
                career_select::TYPE_UI,
                career_select::SCENE,
                career_select::TEMPLATES,
            ]
            .into_iter()
            .find_map(|res| writer(&rel, res));
            let Some(addon) = by else { return Err(e) };
            warnings.push(format!(
                "ui: {addon} has its own copy of the career content select, which Swofty can't edit; addon careers \
                 follow the stock ones there while it's on ({e})"
            ));
            return Ok(None);
        }
    };
    match edit {
        Some(edit) => {
            lines.push("ui: the career content select gets Built In and Addons rows".to_owned());
            Ok(Some(edit))
        }
        None => {
            lines.push(
                "ui: the career content select already has an Addons row from another addon; left as it is"
                    .to_owned(),
            );
            Ok(None)
        }
    }
}

pub(super) fn hub_tabs(
    settings: &SessionSettings,
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    archives: &ArchiveWriter,
) -> Result<Vec<&'static str>> {
    let mut pages = Vec::new();
    for m in settings.addons.iter().filter(|m| m.enabled) {
        let Some(tab) = &m.hub_tab else { continue };
        let rows = tab
            .options
            .iter()
            .map(|id| {
                let o = m.options.iter().find(|o| &o.id == id).ok_or_else(|| {
                    err(format!(
                        "{}: hub tab {} names option {id}, which it doesn't have",
                        m.id, tab.name
                    ))
                })?;
                Ok(hub::Row {
                    id,
                    label: &o.label,
                    description: &o.description,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        pages.push(hub::Page {
            addon: &m.id,
            tab,
            rows,
        });
    }
    if pages.is_empty() {
        return Ok(Vec::new());
    }
    let mut written = Vec::new();
    let options = input(&ui_file(hub::OPTIONS));
    let options_file = ErpFile::open(&options)?;
    let settings = read_frag(&options, hub::SETTINGS_UI, hub::SETTINGS_FRAGMENT)?;
    let mut shared = BTreeSet::new();
    for rel in hub::SHARED {
        let f = ErpFile::open(&input(&ui_file(rel)))?;
        shared.extend(f.erp.resources.iter().map(|r| r.name.clone()));
    }
    for rel in hub::ARCHIVES {
        let (src, dst) = (input(&ui_file(rel)), out.join(ui_file(rel)));
        let ui = match read_frag(&src, hub::UI, hub::UI_FRAGMENT)
            .and_then(|d| hub::build_ui(&d, &settings, &pages))
        {
            Ok(ui) => ui,
            Err(e) => {
                let Some(addon) = writer(&ui_file(rel), hub::UI) else {
                    return Err(e);
                };
                warnings.push(format!(
                    "ui: {addon} has its own copy of the laptop's overview page in {rel}, which Swofty can't edit; hub \
                     tabs are off there while it's on ({e})"
                ));
                continue;
            }
        };
        let mut have = shared.clone();
        have.extend(ErpFile::open(&src)?.erp.resources.iter().map(|r| r.name.clone()));
        let plan = hub::copy_plan(&options_file, &have, &hub::SCENES)?;
        lines.push(format!(
            "ui: hub tabs copy {} settings screen resources from {} into {rel}",
            plan.add.len(),
            hub::OPTIONS
        ));
        write_archive_copying(
            &src,
            &dst,
            vec![(hub::UI, hub::UI_FRAGMENT, Box::new(move |_| Ok(ui)))],
            Vec::new(),
            plan.add,
            archives,
        )?;
        written.push(rel);
    }
    let files: Vec<(String, PathBuf, PathBuf)> = written
        .iter()
        .map(|rel| (ui_file(rel), input(&ui_file(rel)), out.join(ui_file(rel))))
        .collect();
    let pairs: Vec<(&str, &Path, &Path)> = files
        .iter()
        .map(|(name, src, dst)| (name.as_str(), src.as_path(), dst.as_path()))
        .collect();
    let report = check::archive_files(&pairs)?;
    if !report.hard.is_empty() {
        let found: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
        return Err(Error::Other(format!(
            "the hub tab pages break the game's UI loader rules: {}",
            found.join("; ")
        )));
    }
    for p in &pages {
        lines.push(format!(
            "ui: {} has a hub tab template {} for {} careers in {} archive(s); its rows read {}",
            p.addon,
            p.tab.template(),
            p.tab.data_group,
            written.len(),
            p.tab.view_model()
        ));
    }
    Ok(written)
}
