//! An edited ui file's screens and templates against the stock catalog: the node types, properties, pins and
//! layouts the game's own files use.

use std::collections::BTreeMap;

use swofty_formats::pkg::Node;

use super::findings::Finding;
use super::flows::str_of;
use crate::ui::catalog::{Catalog, NodeType, Scenes, attr_shape, element_type, shape, tag};
use crate::ui::idf;
use crate::ui::node::*;

pub(super) const SKIP_PROPS: &[&str] = &["id", "nameId", "version", "name", "type"];

pub(super) const COMPONENTS: usize = 5;

pub(super) struct Stock<'a> {
    pub(super) cat: &'a Catalog,
    pub(super) scenes: &'a Scenes,
    pub(super) types: BTreeMap<String, String>,
    pub(super) out: Vec<Finding>,
}

pub(super) fn node_scene(n: &Node) -> Option<u64> {
    let comp = |o: &swofty_formats::pkg::Obj| match &o.get("componentId")?.as_val()?.value {
        swofty_formats::pkg::Value::Raw(b) if b.len() >= 8 => {
            Some(u64::from_le_bytes(b[..8].try_into().ok()?))
        }
        _ => None,
    };
    if let Some(id) = n
        .slot(COMPONENTS)
        .into_iter()
        .flatten()
        .filter_map(Node::as_obj)
        .find_map(comp)
    {
        return Some(id);
    }
    layout_groups(n)
        .into_iter()
        .flatten()
        .filter(|b| b.get("isComponentId").is_some())
        .find_map(comp)
}

pub(super) fn layout_groups(n: &Node) -> Vec<Vec<&swofty_formats::pkg::Obj>> {
    n.slot(LAYOUT)
        .into_iter()
        .flatten()
        .map(|grp| match grp {
            Node::Arr(a) => a.items.iter().filter_map(Node::as_obj).collect(),
            other => other.as_obj().into_iter().collect(),
        })
        .collect()
}

pub(super) fn seen<V>(m: &BTreeMap<String, V>) -> String {
    let keys: Vec<&str> = m.keys().map(String::as_str).take(8).collect();
    let more = if m.len() > 8 { ", …" } else { "" };
    format!("{}{more}", keys.join(", "))
}

impl Stock<'_> {
    fn visit(
        &mut self,
        n: &Node,
        path: &str,
        parent: Option<&str>,
        role: &str,
        ctx: Option<u64>,
        scopes: &[u64],
    ) {
        let t = n.type_code();
        let here = format!("{path} [{t}]");
        let own = node_scene(n);
        let kid_ctx = own.or(ctx);
        let mut kid_scopes = scopes.to_vec();
        if let Some(nt) = self.cat.node_types.get(&t) {
            self.node(n, &here, nt, &t, parent, role, ctx, scopes, &mut kid_scopes);
        } else {
            self.out.push(Finding::warn(
                "node-type",
                &t,
                &t,
                &here,
                format!("node type {t} is not in any stock .ui"),
            ));
        }
        for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
            let p = format!("{path}.{}", c.name());
            self.visit(c, &p, Some(&t), "child", kid_ctx, &kid_scopes);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        n: &Node,
        here: &str,
        nt: &NodeType,
        t: &str,
        parent: Option<&str>,
        role: &str,
        ctx: Option<u64>,
        scopes: &[u64],
        kid_scopes: &mut Vec<u64>,
    ) {
        let out = &mut self.out;
        if role != "child" && !nt.roles.contains_key(role) {
            out.push(Finding::warn(
                "root-type",
                t,
                role,
                here,
                format!("stock never uses a {t} as a {role}; it is a {}", seen(&nt.roles)),
            ));
        }
        if let Some(p) = parent
            && self.cat.node_types.contains_key(p)
            && !nt.parents.contains_key(p)
        {
            out.push(Finding::warn(
                "parent-type",
                t,
                p,
                here,
                format!(
                    "stock never puts a {t} under a {p}; its parents are {}",
                    seen(&nt.parents)
                ),
            ));
        }
        if let Ok(props) = n.props() {
            for (k, v) in &props.items {
                let key = k.as_str().unwrap_or_default();
                if SKIP_PROPS.contains(&key) {
                    continue;
                }
                match nt.props.get(key) {
                    None => out.push(Finding::warn(
                        "prop",
                        t,
                        key,
                        here,
                        format!("property {key} is never set on a stock {t}"),
                    )),
                    Some(p) => {
                        let sh = shape(v);
                        if !p.shapes.contains_key(&sh) {
                            out.push(Finding::warn(
                                "prop-shape",
                                t,
                                key,
                                here,
                                format!("property {key} is {sh}; stock {t} uses {}", seen(&p.shapes)),
                            ));
                        }
                    }
                }
            }
        }
        for (slot, key, kind, stock) in [
            (COMMANDS, "commandName", "command", &nt.commands),
            (EVENTS, "eventName", "event", &nt.events),
        ] {
            for e in n.slot(slot).into_iter().flatten().filter_map(Node::as_obj) {
                let name = str_of(e, key).unwrap_or("?");
                let sigs: Vec<&swofty_formats::pkg::Obj> = e
                    .get("signals")
                    .and_then(Node::as_arr)
                    .map(|a| a.items.iter().filter_map(Node::as_obj).collect())
                    .unwrap_or_default();
                match stock.get(name) {
                    None => out.push(Finding::warn(
                        kind,
                        t,
                        name,
                        here,
                        format!("{kind} {name} is never declared on a stock {t}"),
                    )),
                    Some(s) if sigs.is_empty() && s.no_signals == 0 => out.push(Finding::warn(
                        if kind == "event" {
                            "event-no-signals"
                        } else {
                            "command-no-signals"
                        },
                        t,
                        name,
                        here,
                        format!(
                            "{kind} {name} has no signals; all {} stock {t} {name} {kind}s have signals. The loader accepts an empty signals list, and raising the {kind} then does nothing ({}.entry; Vm_Raise_NodeEvent 0x1442468d0)",
                            s.count,
                            if kind == "event" { "events" } else { "commands" }
                        ),
                    )),
                    Some(s) if !sigs.is_empty() && s.no_signals == s.count => out.push(Finding::warn(
                        if kind == "event" {
                            "event-signals"
                        } else {
                            "command-signals"
                        },
                        t,
                        name,
                        here,
                        format!("{kind} {name} has signals; no stock {t} {name} {kind} has any"),
                    )),
                    _ => {}
                }
                for s in sigs {
                    let to = str_of(s, "to_object_name").unwrap_or_default();
                    let pin = str_of(s, "to_pin_name").unwrap_or_default();
                    let from = str_of(s, "from_pin_name").unwrap_or("null");
                    if !nt.pins_out.contains_key(from) {
                        out.push(Finding::warn(
                            "pin-out",
                            t,
                            from,
                            here,
                            format!("on {name} sends from pin {from}, which no stock {t} sends from"),
                        ));
                    }
                    let Some(tt) = self.types.get(to) else { continue };
                    let Some(target) = self.cat.node_types.get(tt) else {
                        continue;
                    };
                    let Some(p) = target.pins_in.get(pin) else {
                        out.push(Finding::warn(
                            "pin-in",
                            tt,
                            pin,
                            here,
                            format!(
                                "on {name} -> {to}.{pin}: no stock signal reaches pin {pin} on a {tt}{}",
                                pin_note(tt, pin)
                            ),
                        ));
                        continue;
                    };
                    if !nt.links.contains_key(&format!("{tt}.{pin}")) {
                        out.push(Finding::warn(
                            "link",
                            t,
                            &format!("{tt}.{pin}"),
                            here,
                            format!("on {name} -> {to}.{pin}: no stock {t} signals a {tt}.{pin}"),
                        ));
                    }
                    let attrs = s.get("attributes").map(attr_shape);
                    let with: u64 = p.attributes.values().sum();
                    match attrs {
                        Some(a) if with == 0 => out.push(Finding::warn("pin-attributes", tt, pin, here, format!(
                            "on {name} -> {to}.{pin}: sends attributes {a}; stock signals to {tt}.{pin} carry none"
                        ))),
                        Some(a) if !p.attributes.contains_key(&a) => out.push(Finding::warn("pin-attributes", tt, pin, here, format!(
                            "on {name} -> {to}.{pin}: attributes {a}; stock signals to {tt}.{pin} carry {}",
                            seen(&p.attributes)
                        ))),
                        None if with == p.count => out.push(Finding::warn("pin-attributes", tt, pin, here, format!(
                            "on {name} -> {to}.{pin}: no attributes; every stock signal to {tt}.{pin} carries {}",
                            seen(&p.attributes)
                        ))),
                        _ => {}
                    }
                }
            }
        }
        for grp in layout_groups(n) {
            let kinds: Vec<&str> = grp
                .iter()
                .map(|b| {
                    if b.get("isComponentId").is_some() {
                        "comp"
                    } else if b.get("nodeId").is_some() {
                        "node"
                    } else {
                        "?"
                    }
                })
                .collect();
            let key = format!("{role}:{}", kinds.join(">"));
            if !nt.layout.contains_key(&key) {
                out.push(Finding::warn(
                    "layout-shape",
                    t,
                    &kinds.join(">"),
                    here,
                    format!(
                        "layout shape {} as a {role} is never used by a stock {t}; it uses {}",
                        kinds.join(">"),
                        seen(&nt.layout)
                    ),
                ));
            }
            let names: Vec<&str> = grp
                .iter()
                .filter(|b| b.get("nodeId").is_some())
                .filter_map(|b| str_of(b, "debugName"))
                .collect();
            if role != "child" || names.is_empty() || names.len() != grp.len() {
                continue;
            }
            let hit = ctx
                .into_iter()
                .chain(scopes.iter().rev().copied())
                .filter(|sid| self.scenes.scenes.contains_key(sid))
                .find_map(|sid| self.scenes.resolve(&names, sid).ok());
            let Some((chain, xref)) = hit else { continue };
            let ch = chain.join(">");
            if chain[..chain.len() - 1].iter().any(|c| c != "eXRef") {
                out.push(Finding::warn("layout-through", t, &ch, here, format!(
                    "layout {} walks {ch}; in stock every element before the last in a layout path is an eXRef",
                    names.join(">")
                )));
            } else if !nt.layout_elements.contains_key(&ch) {
                out.push(Finding::warn(
                    "layout-element",
                    t,
                    &ch,
                    here,
                    format!(
                        "layout {} binds {ch}; stock {t} binds {}",
                        names.join(">"),
                        seen(&nt.layout_elements)
                    ),
                ));
            }
            if let Some(x) = xref {
                kid_scopes.push(x);
            }
        }
    }
}

pub(super) fn pin_note(class: &str, pin: &str) -> String {
    match crate::ui::catalog::engine().classes.get(class) {
        Some(c) if !c.native_pins.iter().any(|p| p == pin) => format!(
            "; the {class} pin handler ({}) has no case for it, so the signal does nothing unless the target declares an event or command named {pin}",
            c.deliver_pin
        ),
        _ => String::new(),
    }
}

pub(super) fn index_types(n: &Node, path: &str, out: &mut BTreeMap<String, String>) {
    out.insert(path.to_owned(), n.type_code());
    for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
        index_types(c, &format!("{path}.{}", c.name()), out);
    }
}

pub fn against_stock(data: &[u8], cat: &Catalog, scenes: &Scenes) -> crate::Result<Vec<Finding>> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let mut out = Vec::new();
    for (key, role) in [("Screens", "screen_root"), ("Templates", "template_root")] {
        let tops: Vec<&Node> = root
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.iter().filter(|t| t.is_node()).collect())
            .unwrap_or_default();
        let mut types = BTreeMap::new();
        for t in &tops {
            index_types(t, t.name(), &mut types);
        }
        let mut st = Stock {
            cat,
            scenes,
            types,
            out: Vec::new(),
        };
        for t in tops {
            st.visit(t, t.name(), None, role, node_scene(t), &[]);
        }
        out.extend(st.out);
    }
    Ok(out)
}

pub fn scene_against_stock(data: &[u8], cat: &Catalog) -> crate::Result<Vec<Finding>> {
    let p = super::parsed(data)?;
    let mut out = Vec::new();
    for entry in idf::scenes(&p)? {
        let Some(root) = entry.as_obj().and_then(|o| o.get("scene")) else {
            continue;
        };
        scene_walk(root, None, "", cat, &mut out);
    }
    Ok(out)
}

pub(super) fn scene_walk(e: &Node, parent: Option<&str>, path: &str, cat: &Catalog, out: &mut Vec<Finding>) {
    let t = element_type(e);
    let here = format!("{} [{t}]", if path.is_empty() { idf::elem_name(e) } else { path });
    match cat.scene_elements.get(&t) {
        None => out.push(Finding::warn(
            "element-type",
            &t,
            &t,
            &here,
            format!("element type {t} is not in any stock scene"),
        )),
        Some(et) => {
            if let Some(p) = parent
                && !et.parents.contains_key(p)
            {
                out.push(Finding::warn(
                    "element-parent",
                    &t,
                    p,
                    &here,
                    format!(
                        "stock never puts a {t} under a {p}; its parents are {}",
                        seen(&et.parents)
                    ),
                ));
            }
            for prm in e
                .as_obj()
                .and_then(|o| o.get("params"))
                .and_then(Node::as_arr)
                .map(|a| a.items.as_slice())
                .unwrap_or_default()
                .iter()
                .filter_map(Node::as_obj)
            {
                let name = str_of(prm, "name").unwrap_or_default();
                let ty = match prm.get("value") {
                    Some(Node::Val(v)) => tag(v.ty),
                    Some(other) => shape(other),
                    None => "none".into(),
                };
                match et.params.get(name) {
                    None => out.push(Finding::warn(
                        "param",
                        &t,
                        name,
                        &here,
                        format!("param {name} is never set on a stock {t}"),
                    )),
                    Some(sp) if !sp.types.contains_key(&ty) => out.push(Finding::warn(
                        "param-type",
                        &t,
                        name,
                        &here,
                        format!("param {name} is {ty}; stock {t} uses {}", seen(&sp.types)),
                    )),
                    _ => {}
                }
            }
        }
    }
    for c in e
        .as_obj()
        .and_then(|o| o.get("children"))
        .and_then(Node::as_arr)
        .map(|a| a.items.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|c| c.as_obj().is_some())
    {
        let name = idf::elem_name(c);
        let p = match (parent, path) {
            (None, _) => name.to_owned(),
            (_, "") => name.to_owned(),
            _ => format!("{path}.{name}"),
        };
        scene_walk(c, Some(&t), &p, cat, out);
    }
}
