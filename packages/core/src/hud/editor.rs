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

use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Pkg, Value};

use super::dials::TEMPLATE_UIPRE_KEYS;
use super::scene::Plan;
use crate::ui::idf::detached;
use crate::ui::node::{self, NodeExt, ObjExt};
use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

fn model<'a>(els: &'a [Node], key: u32, ty: [u8; 4]) -> Option<(&'a Node, &'a Node)> {
    for el in els {
        let Some(setters) = el.as_obj().and_then(|o| o.get("Setters")).and_then(Node::as_arr) else {
            continue;
        };
        for st in &setters.items {
            let o = st.as_obj()?;
            let k = o.get("Key").and_then(Node::as_val).and_then(|v| v.as_u32());
            let t = o.get("Value").and_then(Node::as_val).map(|v| v.ty);
            if k == Some(key) && t == Some(ty) {
                return Some((el, st));
            }
        }
    }
    None
}

/// The `.uipre` of the editor's scene with the stock `hide` parts hidden and each `(element,
/// text)` set. Every setter is a copy of a stock one.
pub fn uipre(stock: &[u8], scene: &str, hide: &[String], texts: &[(String, String)]) -> Result<Vec<u8>> {
    let mut p = Pkg::parse(stock)?;
    let target = match &p.root.o()?.at("TargetId")?.v()?.value {
        Value::Raw(b) if b.len() >= 8 => u64::from_le_bytes(b[..8].try_into().expect("8 bytes")),
        _ => 0,
    };
    if target != node::component_id(scene) {
        return Err(err(format!("the .uipre is not the preview data of {scene}")));
    }
    let (text_key, visible_key) = TEMPLATE_UIPRE_KEYS;
    let elements = p.root.o_mut()?.at_mut("Elements")?.a_mut()?;
    let mut have: BTreeSet<u32> = elements
        .iter()
        .filter_map(|e| e.as_obj()?.get("Id")?.as_val()?.as_u32())
        .collect();
    let mut add = Vec::new();
    let jobs = hide
        .iter()
        .map(|n| (n.as_str(), visible_key, *b"bool", None))
        .chain(texts.iter().map(|(e, t)| (e.as_str(), text_key, *b"stri", Some(t.as_str()))));
    for (name, key, ty, text) in jobs {
        let (el_m, st_m) = model(elements, key, ty)
            .ok_or_else(|| err(format!("no stock {} setter with key {key} to copy", String::from_utf8_lossy(&ty))))?;
        let (mut el, mut st) = (detached(el_m), detached(st_m));
        let id = djb2(name);
        if !have.insert(id) {
            return Err(err(format!("the preview element id of {name} is already used")));
        }
        el.o_mut()?.at_mut("Id")?.v_mut()?.set_u32(id);
        let v = st.o_mut()?.at_mut("Value")?.v_mut()?;
        match text {
            Some(t) => v.set_str(t),
            None => v.value = Value::Raw(vec![0]),
        }
        *el.o_mut()?.at_mut("Setters")?.a_mut()? = vec![st];
        add.push(el);
    }
    elements.extend(add);
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
    if target.slot(node::CHILDREN)?.iter().any(|c| c.is_node() && c.name() == name) {
        return Err(err(format!("{parent} already has a {name}")));
    }
    let mut grp = control(plan, show_stock);
    node::fix_ids(&mut grp, Some(parent));
    target.slot_mut(node::CHILDREN)?.push(grp);
    Ok(())
}
