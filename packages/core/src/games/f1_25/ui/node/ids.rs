//! Renaming nodes and keeping their djb2 ids, signals and references in step with the new names.

use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Obj, Val};

use super::ext::NodeExt;
use super::ext::set_str;
use super::{CHILDREN, COMMANDS, EVENTS, LAYOUT, PROPS};
use crate::Result;

pub fn rename(n: &mut Node, name: &str) -> Result<()> {
    set_str(n.props_mut()?, "name", name)
}

pub(super) fn fix_signals(list: &mut [Node]) {
    for e in list {
        let Some(o) = e.as_obj_mut() else { continue };
        let hash_of = |o: &Obj, k: &str| o.get(k).and_then(Node::as_val).and_then(Val::as_str).map(djb2);
        if let Some(h) = hash_of(o, "eventName")
            && let Some(v) = o.get_mut("event").and_then(Node::as_val_mut)
        {
            v.set_u32(h);
        }
        if let Some(h) = hash_of(o, "commandName")
            && let Some(v) = o.get_mut("command").and_then(Node::as_val_mut)
        {
            v.set_u32(h);
        }
        let Some(Node::Arr(sigs)) = o.get_mut("signals") else {
            continue;
        };
        for s in &mut sigs.items {
            let Some(s) = s.as_obj_mut() else { continue };
            for (name, hash) in [("to_object_name", "to_object"), ("to_pin_name", "to_pin")] {
                if let Some(h) = hash_of(s, name)
                    && let Some(v) = s.get_mut(hash).and_then(Node::as_val_mut)
                {
                    v.set_u32(h);
                }
            }
        }
    }
}

pub fn fix_ids(n: &mut Node, parent: Option<&str>) {
    let name = n.name().to_owned();
    let path = match parent {
        Some(p) => format!("{p}.{name}"),
        None => name.clone(),
    };
    let Some(items) = n.as_arr_mut().map(|a| &mut a.items) else {
        return;
    };
    if let Some(p) = items[PROPS].as_obj_mut() {
        if let Some(v) = p.get_mut("id").and_then(Node::as_val_mut) {
            v.set_u32(djb2(&path));
        }
        if let Some(v) = p.get_mut("nameId").and_then(Node::as_val_mut) {
            v.set_u32(djb2(&name));
        }
    }
    for i in [COMMANDS, EVENTS] {
        if let Some(a) = items[i].as_arr_mut() {
            fix_signals(&mut a.items);
        }
    }
    if let Some(lay) = items[LAYOUT].as_arr_mut() {
        for grp in &mut lay.items {
            let Some(grp) = grp.as_arr_mut() else { continue };
            for b in &mut grp.items {
                let Some(b) = b.as_obj_mut() else { continue };
                let Some(h) = b
                    .get("debugName")
                    .and_then(Node::as_val)
                    .and_then(Val::as_str)
                    .map(djb2)
                else {
                    continue;
                };
                if let Some(v) = b.get_mut("nodeId").and_then(Node::as_val_mut) {
                    v.set_u32(h);
                }
            }
        }
    }
    if let Some(kids) = items[CHILDREN].as_arr_mut() {
        for c in &mut kids.items {
            if c.is_node() {
                fix_ids(c, Some(&path));
            }
        }
    }
}

pub fn rename_refs(n: &mut Node, old: &str, new: &str) {
    match n {
        Node::Val(v) => {
            if let Some(s) = v.as_str()
                && v.ty == *b"stri"
                && (s == old || s.strip_prefix(old).is_some_and(|r| r.starts_with('.')))
            {
                let renamed = format!("{new}{}", &s[old.len()..]);
                v.set_str(renamed);
            }
        }
        Node::Obj(o) => o.items.iter_mut().for_each(|(_, v)| rename_refs(v, old, new)),
        Node::Arr(a) => a.items.iter_mut().for_each(|v| rename_refs(v, old, new)),
        Node::Iar(_) => {}
    }
}

pub fn replace_strings(n: &mut Node, map: &dyn Fn(&str) -> Option<String>) {
    match n {
        Node::Val(v) if v.ty == *b"stri" => {
            if let Some(new) = v.as_str().and_then(map) {
                v.set_str(new);
            }
        }
        Node::Obj(o) => o.items.iter_mut().for_each(|(_, v)| replace_strings(v, map)),
        Node::Arr(a) => a.items.iter_mut().for_each(|v| replace_strings(v, map)),
        _ => {}
    }
}

pub fn clone_node(src: &Node, parent: &str, new_name: &str) -> Result<Node> {
    let mut n = src.clone();
    rename_refs(
        &mut n,
        &format!("{parent}.{}", src.name()),
        &format!("{parent}.{new_name}"),
    );
    rename(&mut n, new_name)?;
    fix_ids(&mut n, Some(parent));
    Ok(n)
}
