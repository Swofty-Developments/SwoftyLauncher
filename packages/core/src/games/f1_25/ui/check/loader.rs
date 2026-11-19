//! An edited ui file's node records, checked the way the game's loader reads them, against the engine's
//! rules.

use std::collections::BTreeSet;

use swofty_formats::pkg::Node;

use super::findings::Finding;
use super::flows::str_of;
use crate::ui::catalog::{Catalog, Engine};
use crate::ui::node::*;

pub(super) fn u64_of(n: Option<&Node>) -> Option<u64> {
    match &n?.as_val()?.value {
        swofty_formats::pkg::Value::Raw(b) if b.len() >= 8 => {
            Some(u64::from_le_bytes(b[..8].try_into().ok()?))
        }
        _ => None,
    }
}

pub(super) struct Loader<'a> {
    pub(super) engine: &'a Engine,
    pub(super) frag: &'a str,
    pub(super) out: Vec<Finding>,
}

impl Loader<'_> {
    fn flag(&mut self, id: &str, path: &str, message: String) {
        if let Some(rule) = self.engine.rule(id) {
            self.out
                .push(Finding::engine(rule, &format!("{}: {path}", self.frag), message));
        }
    }

    fn node(&mut self, n: &Node, parent: &str) {
        let Some(items) = n.as_arr().map(|a| &a.items) else {
            self.flag("node.record", parent, "a child is not a node record".into());
            return;
        };
        let props = items.first().and_then(Node::as_obj);
        let name = props.and_then(|p| str_of(p, "name"));
        let ty = props
            .and_then(|p| p.get("type"))
            .and_then(Node::as_val)
            .and_then(|v| v.as_u32());
        let path = match (parent, name) {
            ("", Some(nm)) => nm.to_owned(),
            (_, Some(nm)) => format!("{parent}.{nm}"),
            (_, None) => format!("{parent}.?"),
        };
        let slots_ok = (6..=8).contains(&items.len()) && items[1..].iter().all(|s| s.as_arr().is_some());
        let (Some(_), Some(ty), true) = (name, ty, slots_ok) else {
            self.flag(
                "node.record",
                &path,
                "the node must be [props with name and type, children, commands, events, layout, components(, behaviours, slot7)]".into(),
            );
            return;
        };
        let class = fourcc_str(ty);
        let here = format!("{path} [{class}]");
        let slot = |i: usize| {
            items
                .get(i)
                .and_then(Node::as_arr)
                .map(|a| a.items.as_slice())
                .unwrap_or_default()
        };
        let engine = self.engine;
        let Some(info) = engine.classes.get(&class) else {
            self.flag(
                "node.class_registered",
                &here,
                format!("no node class is registered for {class}"),
            );
            return;
        };
        if info.init_calls_element_base {
            if let Some(first) = slot(4).first() {
                match first.as_arr() {
                    Some(a) if a.items.is_empty() => self.flag(
                        "layout.path_nonempty",
                        &here,
                        "the layout path is an empty list".into(),
                    ),
                    Some(a) => {
                        for step in &a.items {
                            let Some(o) = step.as_obj() else {
                                self.flag("layout.step_keys", &here, "a layout step is not an object".into());
                                continue;
                            };
                            if o.get("isComponentId").is_some() {
                                if u64_of(o.get("componentId")).is_none() {
                                    self.flag(
                                        "layout.step_keys",
                                        &here,
                                        "a component layout step has no u64 componentId".into(),
                                    );
                                }
                            } else if o
                                .get("nodeId")
                                .and_then(Node::as_val)
                                .and_then(|v| v.as_u32())
                                .is_none()
                            {
                                self.flag(
                                    "layout.step_keys",
                                    &here,
                                    "a layout step has neither isComponentId nor a u32 nodeId".into(),
                                );
                            }
                        }
                    }
                    None => self.flag(
                        "layout.step_keys",
                        &here,
                        "the layout path is not a list of steps".into(),
                    ),
                }
            }
            if let Some(c) = slot(5).first() {
                let ok = c
                    .as_obj()
                    .is_some_and(|o| o.get("name").is_some() && u64_of(o.get("componentId")).is_some());
                if !ok {
                    self.flag(
                        "components.keys",
                        &here,
                        "the first component has no name or no u64 componentId".into(),
                    );
                }
            }
            for b in slot(6) {
                if b.as_obj().and_then(|o| o.get("type")).is_none() {
                    self.flag("behaviours.type", &here, "a behaviour has no type".into());
                }
            }
        }
        for (i, key, id) in [
            (COMMANDS, "command", "commands.entry"),
            (EVENTS, "event", "events.entry"),
        ] {
            for e in slot(i) {
                let o = e.as_obj();
                if !o.is_some_and(|o| {
                    o.get(key).is_some() && o.get("signals").and_then(Node::as_arr).is_some()
                }) {
                    let what = o
                        .and_then(|o| {
                            str_of(
                                o,
                                if key == "command" {
                                    "commandName"
                                } else {
                                    "eventName"
                                },
                            )
                        })
                        .unwrap_or("?");
                    self.flag(
                        id,
                        &here,
                        format!("{key} {what} needs a {key} hash and a signals list"),
                    );
                    continue;
                }
                let sigs = o
                    .and_then(|o| o.get("signals"))
                    .and_then(Node::as_arr)
                    .map(|a| a.items.as_slice())
                    .unwrap_or_default();
                for s in sigs {
                    let so = s.as_obj();
                    let ok = so.is_some_and(|s| s.get("to_pin").is_some() && s.get("to_object").is_some());
                    let maps_ok = so
                        .and_then(|s| s.get("mappings"))
                        .and_then(Node::as_arr)
                        .is_none_or(|m| {
                            m.items
                                .iter()
                                .all(|e| e.as_obj().is_some_and(|e| e.get("value").is_some()))
                        });
                    if !ok || !maps_ok {
                        self.flag(
                            "signals.entry",
                            &here,
                            "a signal needs to_pin and to_object, and every mapping a value".into(),
                        );
                    }
                }
            }
        }
        let p = props.expect("checked above");
        let behaviours: Vec<String> = slot(6)
            .iter()
            .filter_map(|b| b.as_obj()?.get("type")?.as_val()?.as_u32())
            .map(fourcc_str)
            .collect();
        for rule in engine.rules.iter().filter(|r| !r.props.is_empty()) {
            if !rule.applies_to.matches(&class, &behaviours) {
                continue;
            }
            for pr in &rule.props {
                if let Some(problem) = pr.problem(p.get(&pr.key)) {
                    let on = if rule.applies_to.behaviours.is_empty() {
                        class.clone()
                    } else {
                        format!("{class} with {}", rule.applies_to.behaviours.join(", "))
                    };
                    self.out.push(Finding::engine(
                        rule,
                        &format!("{}: {here}", self.frag),
                        format!("{on}: {problem}"),
                    ));
                }
            }
        }
        for c in slot(CHILDREN) {
            self.node(c, &path);
        }
    }
}

pub(super) fn root_names(data: &[u8]) -> crate::Result<Vec<(String, String)>> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let mut out = Vec::new();
    for key in ["Screens", "Templates"] {
        for t in root
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            if t.is_node() {
                out.push((key.to_owned(), t.name().to_owned()));
            }
        }
    }
    Ok(out)
}

pub fn loader_findings(
    resource: &str,
    frag: &str,
    data: &[u8],
    stock: Option<&[u8]>,
    cat: &Catalog,
    engine: &Engine,
) -> crate::Result<Vec<Finding>> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let mut l = Loader {
        engine,
        frag,
        out: Vec::new(),
    };
    for key in ["Screens", "Templates"] {
        for t in root
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            l.node(t, "");
        }
    }
    let names = root_names(data)?;
    let before: BTreeSet<(String, String)> = match stock {
        Some(s) => root_names(s)?.into_iter().collect(),
        None => BTreeSet::new(),
    };
    let mut seen = BTreeSet::new();
    for (kind, name) in &names {
        let id = if kind == "Templates" {
            "template.duplicate"
        } else {
            "screen.duplicate"
        };
        let what = if kind == "Templates" { "template" } else { "screen" };
        if !seen.insert((kind.clone(), name.clone())) {
            l.flag(id, name, format!("a second {what} named {name} in this file"));
            continue;
        }
        if before.contains(&(kind.clone(), name.clone())) {
            continue;
        }
        let elsewhere: Vec<&String> = cat
            .root_names
            .get(kind)
            .and_then(|m| m.get(name))
            .map(|v| v.iter().filter(|r| r.as_str() != resource).collect())
            .unwrap_or_default();
        if let Some(other) = elsewhere.first() {
            l.flag(
                id,
                name,
                format!("stock {other} already registers a {what} named {name}"),
            );
        }
    }
    Ok(l.out)
}
