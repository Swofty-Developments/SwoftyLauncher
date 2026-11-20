//! Building nodes and screens, and taking parts out of them.

use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Obj, Val};

use super::ext::{NodeExt, ObjExt};
use super::ext::{set_prop, set_prop_value, set_str};
use super::ids::{rename, rename_refs};
use super::signals::{command, drop_events_to, sig_with};
use super::values::{arr, boolv, component_id, fourcc, obj, prop, s, u32v};
use super::{CHILDREN, COMMANDS, DECISIONS, EVENTS, GROUP_BEHAVIOURS, SCREEN_BEHAVIOURS};
use crate::Result;

pub fn behaviours(codes: &[&str]) -> Node {
    arr(codes
        .iter()
        .map(|c| obj(vec![("type", u32v(fourcc(c)))]))
        .collect())
}

pub fn layout_path(names: &[&str]) -> Node {
    arr(vec![arr(names
        .iter()
        .map(|n| obj(vec![("nodeId", u32v(djb2(n))), ("debugName", s(n))]))
        .collect())])
}

pub fn node(
    name: &str,
    kind: &str,
    props: Vec<(&str, Node)>,
    events: Vec<Node>,
    layout: &[&str],
    behav: &[&str],
) -> Node {
    let mut p = vec![
        ("id", u32v(0)),
        ("version", u32v(0)),
        ("type", u32v(fourcc(kind))),
        ("nameId", u32v(djb2(name))),
        ("name", s(name)),
    ];
    p.extend(props);
    arr(vec![
        obj(p),
        arr(vec![]),
        arr(vec![]),
        arr(events),
        if layout.is_empty() {
            arr(vec![])
        } else {
            layout_path(layout)
        },
        arr(vec![]),
        behaviours(behav),
        arr(vec![]),
    ])
}

pub fn literal_label(n: &mut Node, text: &str) -> Result<()> {
    let p = n.props_mut()?;
    match p.get_mut("text") {
        None => p.set("text", prop(s(text), None)),
        Some(t) => {
            let t = t.o_mut()?;
            set_str(t, "value", text)?;
            t.drop_key("dbentry");
            t.drop_key("isloctag");
        }
    }
    set_prop_value(n, "localise", Val::bool(false))
}

pub fn literal_text(n: &mut Node, text: &str) -> Result<()> {
    let t = n.props_mut()?.at_mut("text")?.o_mut()?;
    set_str(t, "value", text)?;
    t.drop_key("dbentry");
    t.drop_key("isloctag");
    set_prop_value(n, "localise", Val::bool(false))
}

pub fn hidden(name: &str, layout: &[&str]) -> Node {
    node(
        name,
        "UIGP",
        vec![("visible", prop(boolv(false), None))],
        vec![],
        layout,
        GROUP_BEHAVIOURS,
    )
}

pub fn set_bool(n: &mut Node, key: &str, on: bool) -> Result<()> {
    set_prop(n, key, prop(boolv(on), None))
}

pub fn strip_data(n: &mut Node) -> Result<()> {
    let p = n.props_mut()?;
    p.drop_key("allowVMBinding");
    for (_, v) in &mut p.items {
        let Some(o) = v.as_obj_mut() else { continue };
        let local = o
            .get("dbentry")
            .and_then(Node::as_val)
            .and_then(Val::as_str)
            .is_some_and(|d| !d.starts_with('#'));
        if local {
            o.drop_key("dbentry");
            o.drop_key("isloctag");
        }
    }
    n.slot_mut(COMMANDS)?.clear();
    n.slot_mut(CHILDREN)?
        .retain(|c| !(c.is_node() && DECISIONS.contains(&c.type_code().as_str())));
    for c in n.slot_mut(CHILDREN)? {
        if c.is_node() {
            strip_data(c)?;
        }
    }
    Ok(())
}

pub fn instance(
    template: &Node,
    path: &str,
    new_name: &str,
    texts: &[(&str, &str)],
    keep_events: bool,
) -> Result<Node> {
    let mut t = template.clone();
    rename_refs(&mut t, template.name(), &format!("{path}.{new_name}"));
    rename(&mut t, new_name)?;
    strip_data(&mut t)?;
    drop_events_to(&mut t, &["command_vmContextValid"])?;
    if !keep_events {
        t.slot_mut(EVENTS)?.clear();
    }
    for (sub, text) in texts {
        literal_label(t.find_mut(sub)?, text)?;
    }
    Ok(t)
}

pub fn screen(name: &str, layout: &str, props_from: &Obj) -> Result<Node> {
    let mut p = props_from.clone();
    p.items
        .retain(|(k, _)| !matches!(k.as_str(), Some("allowVMBinding" | "sceneAttributes")));
    set_str(&mut p, "name", name)?;
    let lay = arr(vec![arr(vec![obj(vec![
        (
            "componentId",
            Node::Val(Val::raw(*b"ui64", component_id(layout).to_le_bytes())),
        ),
        ("isComponentId", boolv(true)),
        ("debugName", s(&format!("{layout}.idf"))),
    ])])]);
    let route = |key: &str| {
        command(
            key,
            vec![sig_with(
                &format!("{name}.Action"),
                "route",
                key,
                &[("key", key.into())],
            )],
        )
    };
    Ok(arr(vec![
        Node::Obj(p),
        arr(vec![]),
        arr(vec![route("open"), route("close")]),
        arr(vec![]),
        lay,
        arr(vec![]),
        behaviours(SCREEN_BEHAVIOURS),
        arr(vec![]),
    ]))
}
