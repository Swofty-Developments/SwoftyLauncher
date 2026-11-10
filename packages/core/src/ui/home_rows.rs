use swofty_formats::pkg::{Node, Pkg, Val};

use super::era::set_layout;
use super::idf::{self, elem_name};
use super::node::*;
use crate::Result;

pub const SCENE: &str = "eaid://ui_package/art/screens/frontend/fe_home.idf";
pub const TEMPLATES: &str = "eaid://ui_package/screens/fe_home.ui";
pub const TEMPLATES_FRAGMENT: usize = 1;
pub const ARCHIVE: &str = "flow_frontend.erp";
pub const FLAG: &str = "home_rows.flag";
pub const MENU_FLAG: &str = "home_menu.flag";

const STOCK_TEMPLATE: &str = "fe_tile_button_default";
const STOCK_SLOT: &str = "spawner_stacker_items";
const DESCRIPTION: &str = "text_stacker";
const PATH: &str = "transition_anim_items.transition_fade_items";

pub struct Row {
    pub stock: &'static str,
    pub template: &'static str,
    pub slot: &'static str,
    pub x: f64,
    pub y: f64,
    pub guid: &'static str,
}

const LEFT: f64 = -8.6;
const RIGHT: f64 = -4.0;
const THIRD: f64 = 0.6;

const fn row(template: &'static str, slot: &'static str, x: f64, y: f64, guid: &'static str) -> Row {
    Row {
        stock: STOCK_TEMPLATE,
        template,
        slot,
        x,
        y,
        guid,
    }
}

pub const ROWS: [Row; 9] = [
    row("swofty_home_top_left", "swofty_home_slot_top_left", LEFT, 3.2, "a1c3e5f7-1b2d-4f60-8a91-b3c5d7e9f101"),
    row("swofty_home_top_right", "swofty_home_slot_top_right", RIGHT, 3.2, "a2c3e5f7-1b2d-4f60-8a91-b3c5d7e9f102"),
    row("swofty_home_top_third", "swofty_home_slot_top_third", THIRD, 3.2, "a3c3e5f7-1b2d-4f60-8a91-b3c5d7e9f103"),
    row("swofty_home_modes_left", "swofty_home_slot_modes_left", LEFT, 2.45, "b1d4f6a8-2c3e-4a71-9ba2-c4d6e8f0a201"),
    row("swofty_home_modes_right", "swofty_home_slot_modes_right", RIGHT, 2.45, "b2d4f6a8-2c3e-4a71-9ba2-c4d6e8f0a202"),
    row("swofty_home_wide_a", "swofty_home_row_wide_a", LEFT, 1.7, "c3e5a7b9-3d4f-4b82-8cb3-d5e7f9a1b303"),
    row("swofty_home_wide_b", "swofty_home_row_wide_b", LEFT, 0.95, "d4f6b8c0-4e5a-4c93-9dc4-e6f8a0b2c404"),
    Row {
        stock: "fe_tile_button_divider",
        template: "swofty_home_divider",
        slot: "swofty_home_row_divider",
        x: LEFT,
        y: 0.5,
        guid: "f6b8d0e2-6a7c-4eb5-9fe6-a8b0c2d4e606",
    },
    row("swofty_home_wide_c", "swofty_home_row_wide_c", LEFT, 0.0, "e5a7c9d1-5f6b-4da4-8ed5-f7a9b1c3d505"),
];

const DESCRIPTION_Y: f64 = -1.1;

fn templates(doc: &mut Pkg) -> Result<&mut Vec<Node>> {
    doc.root.o_mut()?.at_mut("Templates")?.a_mut()
}

pub fn has_templates(data: &[u8]) -> Result<bool> {
    let mut doc = Pkg::parse(data)?;
    Ok(templates(&mut doc)?
        .iter()
        .any(|t| t.is_node() && ROWS.iter().any(|r| r.template == t.name())))
}

pub fn build_templates(data: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(data)?;
    if doc.to_bytes()? != data {
        return Err(err(format!("{TEMPLATES} fragment {TEMPLATES_FRAGMENT} does not round-trip")));
    }
    let list = templates(&mut doc)?;
    if list
        .iter()
        .any(|t| t.is_node() && ROWS.iter().any(|r| r.template == t.name()))
    {
        return Err(err(format!("{TEMPLATES} already has the main menu templates")));
    }
    for row in &ROWS {
        let mut tpl = template_named(list, row.stock)?.clone();
        rename_refs(&mut tpl, row.stock, row.template);
        rename(&mut tpl, row.template)?;
        set_layout(&mut tpl, &[row.slot])?;
        fix_ids(&mut tpl, None);
        list.push(tpl);
    }
    let out = doc.to_bytes()?;
    if Pkg::parse(&out)?.to_bytes()? != out {
        return Err(err(format!("the patched {TEMPLATES} does not round-trip")));
    }
    Ok(out)
}

fn template_named<'a>(list: &'a [Node], name: &str) -> Result<&'a Node> {
    list.iter()
        .find(|t| t.is_node() && t.name() == name)
        .ok_or_else(|| err(format!("{TEMPLATES} has no {name}")))
}

fn find_mut<'a>(e: &'a mut Node, name: &str) -> Option<&'a mut Node> {
    if elem_name(e) == name {
        return Some(e);
    }
    let kids = idf::children_mut(e).ok()?;
    kids.iter_mut().find_map(|k| find_mut(k, name))
}

fn set_position(e: &mut Node, x: f64, y: f64) -> Result<()> {
    e.o_mut()?.set("position", Node::Val(idf::vec3([x, y, 0.0, 0.0])));
    Ok(())
}

pub fn has_slots(data: &[u8]) -> Result<bool> {
    let doc = Pkg::parse(data)?;
    let scenes = idf::scenes(&doc)?;
    let root = scenes.first().ok_or_else(|| err("a scene file without scenes"))?.o()?.at("scene")?;
    Ok(ROWS.iter().any(|r| idf::find(root, r.slot).is_some()))
}

pub fn build_scene(data: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(data)?;
    let entry = idf::scenes_mut(&mut doc)?
        .first_mut()
        .ok_or_else(|| err(format!("{SCENE} has no scenes")))?;
    if idf::scene_name(entry) != Some(idf::scene_id(SCENE)) {
        return Err(err(format!("not {SCENE}")));
    }
    let root = entry.o_mut()?.at_mut("scene")?;
    if ROWS.iter().any(|r| idf::find(root, r.slot).is_some()) {
        return Err(err(format!("{SCENE} already has the main menu rows")));
    }
    let parent = find_mut(root, "transition_fade_items").ok_or_else(|| err(format!("{SCENE} has no transition_fade_items")))?;
    let kids = idf::children_mut(parent)?;
    let at = kids
        .iter()
        .position(|k| elem_name(k) == STOCK_SLOT)
        .ok_or_else(|| err(format!("{SCENE}: no {STOCK_SLOT}")))?;
    let mut added = Vec::new();
    for row in &ROWS {
        let mut slot = idf::detached(&kids[at]);
        slot.o_mut()?.set("name", s(row.slot));
        idf::set_param(&mut slot, "guid", Val::str(row.guid))?;
        idf::set_param(&mut slot, "stackerType", Val::str("t"))?;
        set_position(&mut slot, row.x, row.y)?;
        idf::recompute_ids(&mut slot, &format!("{PATH}.{}", row.slot))?;
        added.push(slot);
    }
    for (k, slot) in added.into_iter().enumerate() {
        kids.insert(at + 1 + k, slot);
    }
    let text = kids
        .iter_mut()
        .find(|k| elem_name(k) == DESCRIPTION)
        .ok_or_else(|| err(format!("{SCENE}: no {DESCRIPTION}")))?;
    set_position(text, LEFT, DESCRIPTION_Y)?;
    idf::relookup(entry)?;
    let out = doc.to_bytes()?;
    let back = Pkg::parse(&out)?;
    if back.to_bytes()? != out {
        return Err(err(format!("the patched {SCENE} does not round-trip")));
    }
    let mut bad = idf::xref_mismatches(&back)?;
    bad.extend(idf::id_problems(&back)?);
    if !bad.is_empty() {
        return Err(err(format!("{SCENE}: {}", bad.join("; "))));
    }
    Ok(out)
}

pub const PLAYERCARD: &str = "eaid://ui_package/screens/playercard.ui";
pub const PLAYERCARD_ARCHIVE: &str = "common_front_end.erp";
pub const STRIP_PATH: &str = "#ui_menu_data.swofty_strip_visible";
const STRIP: &str = "paddock_pass_panel";

pub fn has_strip_binding(data: &[u8]) -> Result<bool> {
    let mut doc = Pkg::parse(data)?;
    let screen = first_screen(&mut doc)?;
    let panel = screen
        .slot(CHILDREN)?
        .iter()
        .find(|k| k.is_node() && k.name() == STRIP)
        .ok_or_else(|| err(format!("{PLAYERCARD} has no {STRIP}")))?;
    Ok(panel.props()?.get("visible").is_some_and(|v| format!("{v:?}").contains("swofty_strip_visible")))
}

/// playercard.ui with the top-right strip's group shown or hidden by `ui_menu_data.swofty_strip_visible`,
/// which the core sets false while the main menu is up and true otherwise (true until it is set).
pub fn hide_strip(data: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(data)?;
    if doc.to_bytes()? != data {
        return Err(err(format!("{PLAYERCARD} does not round-trip")));
    }
    let screen = first_screen(&mut doc)?;
    let owner = screen.name().to_owned();
    let panel = screen
        .slot_mut(CHILDREN)?
        .iter_mut()
        .find(|k| k.is_node() && k.name() == STRIP)
        .ok_or_else(|| err(format!("{PLAYERCARD} has no {STRIP}")))?;
    set_prop(panel, "visible", prop(boolv(true), Some(STRIP_PATH)))?;
    let _ = owner;
    let out = doc.to_bytes()?;
    if Pkg::parse(&out)?.to_bytes()? != out {
        return Err(err(format!("the patched {PLAYERCARD} does not round-trip")));
    }
    Ok(out)
}

pub const PANEL_TEXT_PATH: &str = "#ui_menu_data.swofty_panel_text";
pub const PANEL_TITLE_PATH: &str = "#ui_menu_data.swofty_panel_title";
pub const ACCOUNT_PANEL: &str = "swofty_account_panel";
const ACCOUNT_SLOT: &str = "swofty_details_account";
const PANELS_PARENT: &str = "fe_home.contextual_panels";
const ACCOUNT_SOURCE: &str = "offline_contextual_panel";
const ACCOUNT_XREF: &str = "details_offline";
/// Where the two components sit on screen, in scene units from the scene's origin, fitted from the stock screenshot
/// (the list's left edge x -8.6 at 98 px, the description y -1.77 at 713 px, the logo's bottom y 3.7 at 168 px of 1914x1079).
pub const ACCOUNT_AT: (f64, f64) = (4.06, 3.91);
/// transition_anim_details, their parent in the scene, sits here, so a slot's own position is its place minus this.
const DETAILS_PARENT: (f64, f64) = (-8.6, -4.86);
const XREF_GUIDS: [&str; 2] = ["7a8b9c0d-1e2f-4a3b-8c4d-5e6f7a8b9c01", "7a8b9c0d-1e2f-4a3b-8c4d-5e6f7a8b9c02"];

fn bind_text(n: &mut Node, path: &str) -> Result<()> {
    set_prop(n, "text", prop(s(""), Some(path)))?;
    set_prop(n, "localise", prop(boolv(false), None))
}

fn hide(n: &mut Node) -> Result<()> {
    set_prop(n, "visible", prop(boolv(false), None))
}

pub fn has_native_panels_ui(data: &[u8]) -> Result<bool> {
    let mut doc = Pkg::parse(data)?;
    let screen = first_screen(&mut doc)?;
    let panels = screen.child_mut("contextual_panels")?;
    Ok(panels
        .slot(CHILDREN)?
        .iter()
        .any(|k| k.is_node() && k.name() == ACCOUNT_PANEL))
}

/// fe_home.ui with the account panel beside the stock ones in `contextual_panels`, a copy of the "Launched with"
/// card laid out in the scene slot `native_panels_scene` adds. It is always visible, and its text reads
/// `ui_menu_data.swofty_panel_text`.
pub fn native_panels_ui(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |doc| {
        let screen = first_screen(doc)?;
        let panels = screen.child_mut("contextual_panels")?;
        let account_src = panels.child_mut(ACCOUNT_SOURCE)?.clone();
        let mut account = clone_node(&account_src, PANELS_PARENT, ACCOUNT_PANEL)?;
        set_layout(&mut account, &[ACCOUNT_SLOT])?;
        hide_visibility_binding(&mut account)?;
        bind_text(account.child_mut("subtitle")?, PANEL_TEXT_PATH)?;
        hide(account.child_mut("tooltip_label")?)?;
        hide(account.child_mut("tooltip_extra_2")?)?;
        fix_ids(&mut account, Some(PANELS_PARENT));
        let kids = panels.slot_mut(CHILDREN)?;
        kids.push(account);
        Ok(())
    })
}

fn hide_visibility_binding(n: &mut Node) -> Result<()> {
    set_prop(n, "visible", prop(boolv(true), None))
}

pub fn has_native_panels_scene(data: &[u8]) -> Result<bool> {
    let doc = Pkg::parse(data)?;
    let scenes = idf::scenes(&doc)?;
    let root = scenes.first().ok_or_else(|| err("a scene file without scenes"))?.o()?.at("scene")?;
    Ok(idf::find(root, ACCOUNT_SLOT).is_some())
}

/// fe_home.idf with two component slots for the native panels, copies of `details_offline` and `details_news`,
/// under transition_fade_details.
pub fn native_panels_scene(data: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(data)?;
    let entry = idf::scenes_mut(&mut doc)?
        .first_mut()
        .ok_or_else(|| err(format!("{SCENE} has no scenes")))?;
    if idf::scene_name(entry) != Some(idf::scene_id(SCENE)) {
        return Err(err(format!("not {SCENE}")));
    }
    let root = entry.o_mut()?.at_mut("scene")?;
    if idf::find(root, ACCOUNT_SLOT).is_some() {
        return Err(err(format!("{SCENE} already has the native panels")));
    }
    let mut slots = Vec::new();
    for (source, name, at, guid) in [(ACCOUNT_XREF, ACCOUNT_SLOT, ACCOUNT_AT, XREF_GUIDS[0])] {
        let from = idf::find(root, source).ok_or_else(|| err(format!("{SCENE} has no {source}")))?;
        let mut slot = idf::detached(from);
        slot.o_mut()?.set("name", s(name));
        idf::set_param(&mut slot, "guid", Val::str(guid))?;
        set_position(&mut slot, at.0 - DETAILS_PARENT.0, at.1 - DETAILS_PARENT.1)?;
        idf::recompute_ids(&mut slot, &format!("transition_fade_details.transition_anim_details.{name}"))?;
        slots.push(slot);
    }
    let parent = find_mut(root, "transition_anim_details")
        .ok_or_else(|| err(format!("{SCENE} has no transition_anim_details")))?;
    idf::children_mut(parent)?.extend(slots);
    idf::relookup(entry)?;
    let out = doc.to_bytes()?;
    let back = Pkg::parse(&out)?;
    if back.to_bytes()? != out {
        return Err(err(format!("the patched {SCENE} does not round-trip")));
    }
    let mut bad = idf::xref_mismatches(&back)?;
    bad.extend(idf::id_problems(&back)?);
    if !bad.is_empty() {
        return Err(err(format!("{SCENE}: {}", bad.join("; "))));
    }
    Ok(out)
}
