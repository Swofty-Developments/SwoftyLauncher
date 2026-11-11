//! The HUD editor's side: the preview `.uipre` setters, the template group that keeps the static
//! preview copy out of the game, and the selection box.
//!
//! The editor shows `dial_complete.idf` with no template running: the scene's own state plus its
//! `.uipre` preview setters. So each record gets a static copy of its elements (built from
//! `editor.preview` by [`super::scene::Builder::add_record`] in preview mode), the `.uipre` hides
//! the stock parts and fills the copy's texts, and in game a `<prefix>_pv_ctl` group hides the copy.
//! Setter keys and the element id rule (`djb2` of the name) are Project Halo's measurements of the
//! game's own `.uipre` files; Uses work from Project Halo by nyannoying
//! (https://github.com/nyannoying1337/f1-25-project-halo).

use std::collections::BTreeSet;

use swofty_formats::pkg::Node;
use swofty_formats::uipre::{self, Uipre};

use super::scene::Plan;
use crate::ui::idf;
use crate::ui::node::{self, NodeExt};
use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

/// The `.uipre` of the editor's scene with the stock `hide` parts hidden and each `(element,
/// text)` set. The setters are the shape of the stock ones (`Id` and `Setters` of `Key`, `Value`).
pub fn uipre(stock: &[u8], scene: &str, hide: &[String], texts: &[(String, String)]) -> Result<Vec<u8>> {
    let mut p = Uipre::parse(stock)?;
    if p.target_id() != Some(idf::scene_id(scene)) {
        return Err(err(format!("the .uipre is not the preview data of {scene}")));
    }
    let have: BTreeSet<u32> = p.element_ids().into_iter().collect();
    let mut added = BTreeSet::new();
    for name in hide.iter().chain(texts.iter().map(|(e, _)| e)) {
        let id = uipre::element_id(name);
        if have.contains(&id) || !added.insert(id) {
            return Err(err(format!("the preview element id of {name} is already used")));
        }
    }
    for name in hide {
        p.set_visible(name, false)?;
    }
    for (el, text) in texts {
        p.set_text(el, text)?;
    }
    Ok(p.to_bytes()?)
}

/// The group under a dial that hides the preview copy of a record in game. Stock parts it should
/// show again (the `.uipre` may apply in game too) get a static visible node each.
pub fn control(plan: &Plan, show_stock: &[String]) -> Node {
    let mut kids = vec![node::hidden(&format!("v_{}", plan.root), &[&plan.root])];
    for part in show_stock {
        kids.push(node::visible_node(&format!("v_{part}"), part, true, None, false));
    }
    node::group_node(&plan.ctl_name(), vec![], &[], kids)
}

/// Adds [`control`] under `parent`.
pub fn attach_control(template: &mut Node, parent: &str, plan: &Plan, show_stock: &[String]) -> Result<()> {
    let at = node::find_path(template, parent)?;
    let target = node::node_at_mut(template, &at)?;
    let name = plan.ctl_name();
    if target
        .slot(node::CHILDREN)?
        .iter()
        .any(|c| c.is_node() && c.name() == name)
    {
        return Err(err(format!("{parent} already has a {name}")));
    }
    let mut grp = control(plan, show_stock);
    node::fix_ids(&mut grp, Some(parent));
    target.slot_mut(node::CHILDREN)?.push(grp);
    Ok(())
}
