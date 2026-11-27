//! The image templates of the home and career tiles: a ui.tile record that names an image gets its tile built
//! from `addon_tile_image_default` (home) or `addon_tile_image_row` (the career Addons row) instead of the stock
//! button template, and the core sets the template's `image` leaf.
//!
//! The stock button templates have no image element. The one stock tile that binds an `image` leaf is the F1 World
//! era tile (`era_select_tile`), whose `image` node binds `textureOverride` to `image` over the `image_1` element of
//! its component `4x12_era_select.idf`. Both templates are copies of it laid out in the home or career slot, with
//! the `title_localised` binding and the `enabled` switch of `fe_tile_button_default` added. That component lives in
//! flow_f1_world.erp only, so its resources are copied into flow_frontend.erp; the tile units it draws from are in
//! common_front_end.erp, which the frontend flow loads with it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use swofty_formats::erp::{ErpFile, Resource};
use swofty_formats::pkg::{Node, Pkg};

use super::archives::read_frag;
use super::career_select::{ADDON_SLOT, set_layout};
use super::node::*;
use super::{Writer, era, hub, ui_file};
use crate::Result;

pub const FLAG: &str = "tile_images.flag";
pub const HOME_TEMPLATE: &str = "addon_tile_image_default";
pub const ROW_TEMPLATE: &str = "addon_tile_image_row";
pub const COMPONENT: &str = "eaid://ui_package/art/components/vehicle_type/spawned/4x12_era_select.idf";
pub const TEMPLATES: &str = "eaid://ui_package/screens/fe_home.ui";
pub const TEMPLATES_FRAGMENT: usize = 1;
pub const SOURCE_ARCHIVE: &str = era::ARCHIVES[0];
pub const SHARED: [&str; 3] = ["common_front_end.erp", "persistent.erp", "flow_persistent.erp"];
pub const PLACES: [&str; 2] = ["home", "career"];

const SOURCE: &str = "era_select_tile";
const STOCK: &str = "fe_tile_button_default";
const HOME_SLOT: &str = "spawner_stacker_items";
const TITLE: &str = "title";
const TITLE_LOCALISED: &str = "title_localised";
const LOCALISE: &str = "localise";
const SWITCH: &str = "enabled";
const PRESSED: &str = "blokPressedEven";
const TEMPLATE_SLOTS: [(&str, &str); 2] = [(HOME_TEMPLATE, HOME_SLOT), (ROW_TEMPLATE, ADDON_SLOT)];

/// What the UI build writes for the image templates.
pub struct Plan {
    /// The era screen's template fragment, the source of `era_select_tile`.
    pub era_ui: Vec<u8>,
    /// The component's resources flow_frontend.erp lacks.
    pub add: Vec<Resource>,
}

/// Whether a place with an enabled ui.tile record naming an image calls for the templates.
pub fn wanted(places: &[String]) -> bool {
    places.iter().any(|p| PLACES.contains(&p.as_str()))
}

fn templates(doc: &mut Pkg) -> Result<&mut Vec<Node>> {
    doc.root.o_mut()?.at_mut("Templates")?.a_mut()
}

pub fn has_templates(data: &[u8]) -> Result<bool> {
    let mut doc = Pkg::parse(data)?;
    Ok(templates(&mut doc)?
        .iter()
        .any(|t| t.is_node() && TEMPLATE_SLOTS.iter().any(|(n, _)| *n == t.name())))
}

fn named<'a>(list: &'a [Node], name: &str, from: &str) -> Result<&'a Node> {
    list.iter()
        .find(|t| t.is_node() && t.name() == name)
        .ok_or_else(|| err(format!("{from} has no {name}")))
}

fn image_template(era: &Node, stock: &Node, name: &str, slot: &str) -> Result<Node> {
    let mut tpl = era.clone();
    rename_refs(&mut tpl, SOURCE, name);
    rename(&mut tpl, name)?;
    set_layout(&mut tpl, slot)?;
    if let Some(p) = stock.props()?.get(PRESSED) {
        tpl.props_mut()?.set(PRESSED, p.clone());
    }
    set_prop(
        tpl.child_mut(TITLE)?,
        LOCALISE,
        prop(boolv(false), Some(TITLE_LOCALISED)),
    )?;
    let mut switch = stock.child(SWITCH)?.clone();
    rename_refs(&mut switch, STOCK, name);
    tpl.slot_mut(CHILDREN)?.push(switch);
    let mut commands = stock.slot(COMMANDS)?.to_vec();
    rename_refs_all(&mut commands, name);
    for c in &mut commands {
        if let Some(signals) = c
            .as_obj_mut()
            .and_then(|o| o.get_mut("signals"))
            .and_then(Node::as_arr_mut)
        {
            signals
                .items
                .retain(|s| sig_to_name(s).is_some_and(|to| to.ends_with(&format!(".{SWITCH}"))));
        }
    }
    *tpl.slot_mut(COMMANDS)? = commands;
    fix_ids(&mut tpl, None);
    Ok(tpl)
}

fn rename_refs_all(list: &mut [Node], name: &str) {
    for n in list {
        rename_refs(n, STOCK, name);
    }
}

fn sig_to_name(s: &Node) -> Option<&str> {
    s.as_obj()?.get("to_object_name")?.as_val()?.as_str()
}

/// fe_home.ui's template list with the two image templates added, shared by the home screen and the career
/// content select.
pub fn build_templates(data: &[u8], era_ui: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(data)?;
    if doc.to_bytes()? != data {
        return Err(err(format!(
            "{TEMPLATES} fragment {TEMPLATES_FRAGMENT} does not round-trip"
        )));
    }
    let mut source = Pkg::parse(era_ui)?;
    let era = named(templates(&mut source)?, SOURCE, era::UI)?.clone();
    let list = templates(&mut doc)?;
    if list
        .iter()
        .any(|t| t.is_node() && TEMPLATE_SLOTS.iter().any(|(n, _)| *n == t.name()))
    {
        return Err(err(format!("{TEMPLATES} already has the image templates")));
    }
    let stock = named(list, STOCK, TEMPLATES)?.clone();
    for (name, slot) in TEMPLATE_SLOTS {
        list.push(image_template(&era, &stock, name, slot)?);
    }
    let out = doc.to_bytes()?;
    if Pkg::parse(&out)?.to_bytes()? != out {
        return Err(err(format!("the patched {TEMPLATES} does not round-trip")));
    }
    Ok(out)
}

/// The era template and the component resources a frontend archive needs for the image templates; fails when
/// the archive has the component already.
pub fn plan(input: &dyn Fn(&str) -> PathBuf, frontend: &ErpFile) -> Result<Plan> {
    let source = input(&ui_file(SOURCE_ARCHIVE));
    let from = ErpFile::open(&source)?;
    let mut have: BTreeSet<String> = frontend.erp.resources.iter().map(|r| r.name.clone()).collect();
    for rel in SHARED {
        let f = ErpFile::open(&input(&ui_file(rel)))?;
        have.extend(f.erp.resources.iter().map(|r| r.name.clone()));
    }
    let copy = hub::copy_plan(&from, &have, &[COMPONENT])?;
    if copy.add.is_empty() {
        return Err(err(format!("{COMPONENT} is in the frontend archives already")));
    }
    Ok(Plan {
        era_ui: read_frag(&source, era::UI, era::UI_FRAGMENT)?,
        add: copy.add,
    })
}

/// The plan for the frontend archive at `frontend`, with the reason the templates are left out when it can't be made
/// or when another addon's copy of the home screen's templates is in use.
pub fn plan_for(
    input: &dyn Fn(&str) -> PathBuf,
    frontend: &std::path::Path,
    writer: &Writer,
) -> std::result::Result<Plan, String> {
    if let Some(addon) = writer(&ui_file("flow_frontend.erp"), TEMPLATES) {
        return Err(format!(
            "{addon} has its own copy of the home screen's templates, which Swofty can't edit"
        ));
    }
    ErpFile::open(frontend)
        .map_err(crate::Error::from)
        .and_then(|f| plan(input, &f))
        .map_err(|e| e.to_string())
}
