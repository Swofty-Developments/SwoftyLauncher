//! View-model properties: the roots a screen binds at run time, and the properties an edit dropped from the
//! stock screen.

use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::pkg::Node;

use super::findings::Finding;
use super::integrity::integrity_walk;
use crate::ui::catalog::Engine;
use crate::ui::node::*;

pub(super) fn vm_keys(n: &Node, out: &mut BTreeSet<String>) {
    let Ok(props) = n.props() else { return };
    for (k, v) in &props.items {
        let Some(o) = v.as_obj() else { continue };
        let bound = o.get("dbentry").and_then(Node::as_val).and_then(|v| v.as_str());
        let named = (k.as_str() == Some("dbEntry"))
            .then(|| o.get("value").and_then(Node::as_val).and_then(|v| v.as_str()))
            .flatten();
        for key in bound.into_iter().chain(named) {
            if !key.is_empty()
                && !key.starts_with('#')
                && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            {
                out.insert(key.split('.').next().unwrap_or(key).to_owned());
            }
        }
    }
    for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
        vm_keys(c, out);
    }
}

pub(super) fn roots(data: &[u8]) -> crate::Result<BTreeMap<String, BTreeSet<String>>> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let mut out = BTreeMap::new();
    for key in ["Screens", "Templates"] {
        for t in root
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
            .iter()
            .filter(|t| t.is_node())
        {
            let mut keys = BTreeSet::new();
            vm_keys(t, &mut keys);
            out.insert(t.name().to_owned(), keys);
        }
    }
    Ok(out)
}

pub(super) fn prop_index(data: &[u8]) -> crate::Result<BTreeMap<String, (String, BTreeSet<String>)>> {
    let p = super::parsed(data)?;
    let mut nodes = Vec::new();
    for key in ["Screens", "Templates"] {
        for top in p
            .root
            .o()?
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            if top.is_node() {
                integrity_walk(top, None, &mut nodes);
            }
        }
    }
    Ok(nodes
        .into_iter()
        .map(|(path, n)| {
            let keys = n
                .props()
                .map(|p| p.keys().map(str::to_owned).collect())
                .unwrap_or_default();
            (path, (n.type_code(), keys))
        })
        .collect())
}

pub fn dropped_props(
    stock: &[u8],
    edited: &[u8],
    frag: &str,
    engine: &Engine,
) -> crate::Result<Vec<Finding>> {
    let (old, new) = (prop_index(stock)?, prop_index(edited)?);
    let mut out = Vec::new();
    for (path, (class, keys)) in &new {
        let Some((_, before)) = old.get(path) else {
            continue;
        };
        for k in before.difference(keys) {
            let read_by = engine
                .rules
                .iter()
                .find(|r| r.props.iter().any(|p| &p.key == k))
                .map(|r| format!("rule {}", r.id))
                .or_else(|| {
                    engine
                        .classes
                        .get(class)
                        .filter(|c| c.props_read_by_init.contains(k))
                        .map(|c| format!("the {class} init {}", c.init))
                })
                .or_else(|| {
                    engine
                        .element_base_props
                        .contains(k)
                        .then(|| "the element base init".to_owned())
                });
            if let Some(by) = read_by {
                out.push(Finding::stock_diff(
                    "prop-dropped",
                    class,
                    k,
                    &format!("{frag}: {path} [{class}]"),
                    format!("stock sets {k} here and {by} reads it; the edit drops it"),
                ));
            }
        }
    }
    Ok(out)
}

pub fn runtime_bindings(stock: &[u8], edited: &[u8], frag: &str) -> crate::Result<Vec<Finding>> {
    let (old, new) = (roots(stock)?, roots(edited)?);
    let mut out = Vec::new();
    for (name, keys) in &new {
        let Some(before) = old.get(name) else { continue };
        for k in keys.difference(before) {
            out.push(Finding::stock_diff(
                "vm-key-new",
                "",
                k,
                &format!("{frag}: {name}"),
                format!(
                    "binds view-model key {k}, which no stock {name} node binds; stock {name} only binds {}. The binding resolves only if native code has published {k} when the template binds, so the screen depends on that order",
                    before.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
    }
    Ok(out)
}
