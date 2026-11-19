//! Nodes of a given type with the behaviours that type needs: groups, labels, colours, textures, fractions
//! and the view-model command nodes.

use swofty_formats::pkg::{Node, Val};

use super::build::node;
use super::ext::{NodeExt, ObjExt};
use super::signals::{command, sig};
use super::values::{boolv, err, f32v, i32v, obj, prop, s, u32v};
use super::{CHILDREN, COMMANDS, GROUP_BEHAVIOURS};
use crate::Result;

pub const LABEL_BEHAVIOURS: &[&str] = &["NBEL", "NBMG", "NBOS", "NBXA", "NBMO"];

pub const FRACTION_BEHAVIOURS: &[&str] = &[
    "NBFC", "NBXA", "NBEL", "NBDC", "NBMG", "NBVM", "NBAN", "NBAC", "NBOS", "NBMO",
];

pub(super) fn tagged(tag: [u8; 4], v: &str) -> Node {
    match &tag {
        b"stri" => s(v),
        b"ui32" => u32v(v.parse().unwrap_or(0)),
        b"si32" => i32v(v.parse().unwrap_or(0)),
        b"fp32" => f32v(v.parse().unwrap_or(0.0)),
        b"bool" => boolv(v == "true"),
        b"rgba" => Node::Val(Val::raw(
            *b"rgba",
            u32::from_str_radix(v.trim_start_matches('#'), 16)
                .unwrap_or(0xffff_ffff)
                .to_be_bytes(),
        )),
        _ => s(v),
    }
}

/// `{ value, dbentry }` of a property whose value has `tag`, with the stock labels' defaults
/// (ui32 1, text empty, fp32 0).
pub fn typed_prop(tag: [u8; 4], default: &str, db: &str) -> Node {
    prop(tagged(tag, default), Some(db))
}

/// A bool property: `value` the default while the binding is absent, `dbentry` the binding and
/// `dbentrynegate` first when it reads as not.
pub fn bool_prop(default: bool, db: Option<&str>, negate: bool) -> Node {
    let mut items = vec![("value", boolv(default))];
    if let Some(db) = db {
        items.push(("dbentry", s(db)));
    }
    if negate {
        items.insert(0, ("dbentrynegate", boolv(true)));
    }
    obj(items)
}

pub(super) fn with_kids(mut n: Node, kids: Vec<Node>) -> Node {
    if let Ok(slot) = n.slot_mut(CHILDREN) {
        slot.extend(kids);
    }
    n
}

/// A group (`UIGP`) node over the scene element `layout`, with its properties and children.
pub fn group_node(name: &str, props: Vec<(&str, Node)>, layout: &[&str], kids: Vec<Node>) -> Node {
    with_kids(node(name, "UIGP", props, vec![], layout, GROUP_BEHAVIOURS), kids)
}

/// The node that shows or hides a scene element from a bound bool.
pub fn visible_node(name: &str, element: &str, default: bool, db: Option<&str>, negate: bool) -> Node {
    group_node(
        name,
        vec![("visible", bool_prop(default, db, negate))],
        &[element],
        vec![],
    )
}

/// The node that tints a scene element with a bound colour; `default` holds while the value is
/// absent (the stock nodes use white).
pub fn colour_node(name: &str, element: &str, db: &str, default: [u8; 4]) -> Node {
    group_node(
        name,
        vec![(
            "colourOverride",
            obj(vec![
                ("value", Node::Val(Val::raw(*b"rgba", default))),
                ("dbentry", s(db)),
            ]),
        )],
        &[element],
        vec![],
    )
}

/// The node that tints a scene element with one fixed colour (a text's own colour, a quad's tint).
pub fn static_colour_node(name: &str, element: &str, colour: [u8; 4]) -> Node {
    group_node(
        name,
        vec![(
            "colourOverride",
            obj(vec![("value", Node::Val(Val::raw(*b"rgba", colour)))]),
        )],
        &[element],
        vec![],
    )
}

/// The node that sets a scene element's texture from a bound value: `textureOverride` on `layout`
/// (an eXRef, then the quad inside its unit) with the tag the stock node binds it with.
pub fn texture_node(name: &str, layout: &[&str], tag: [u8; 4], default: &str, db: &str) -> Node {
    group_node(
        name,
        vec![("textureOverride", typed_prop(tag, default, db))],
        layout,
        vec![],
    )
}

/// The label (`UILB`) over the scene text `element` showing a bound value of `tag`, or a literal
/// when `db` is `None`.
pub fn label_node(
    name: &str,
    element: &str,
    tag: [u8; 4],
    db: Option<&str>,
    literal: &str,
    visible: Option<Node>,
) -> Node {
    let mut props = Vec::new();
    if let Some(v) = visible {
        props.push(("visible", v));
    }
    let default = if tag == *b"ui32" { "1" } else { literal };
    props.push((
        "text",
        match db {
            Some(db) => typed_prop(tag, default, db),
            None => prop(s(literal), None),
        },
    ));
    if db.is_none() {
        props.push(("localise", prop(boolv(false), None)));
    }
    node(name, "UILB", props, vec![], &[element], LABEL_BEHAVIOURS)
}

/// The node that drives the clip of the scene element `element` from a bound fraction.
pub fn fraction_node(name: &str, element: &str, db: &str) -> Node {
    node(
        name,
        "UIXA",
        vec![("fraction", typed_prop(*b"fp32", "0", db))],
        vec![],
        &[element],
        FRACTION_BEHAVIOURS,
    )
}

/// A multi-value decision node (`MVCD`) on `db`: one event per state, each with its signals.
pub fn mvcd_node(name: &str, db: &str, events: Vec<Node>) -> Node {
    node(
        name,
        "MVCD",
        vec![("dbEntry", prop(s(db), None))],
        events,
        &[],
        &[],
    )
}

/// A single-value decision node (`SVCD`): events `yes` and `no` by whether the value on `db`
/// equals `value` (no comparison operator means equal).
pub fn svcd_node(name: &str, tag: [u8; 4], value: &str, db: &str, events: Vec<Node>) -> Node {
    node(
        name,
        "SVCD",
        vec![
            ("value", prop(tagged(tag, value), None)),
            ("dbEntry", prop(s(db), None)),
        ],
        events,
        &[],
        &[],
    )
}

/// Adds the `trigger` and `untrigger` commands that register and query (or unregister) the
/// decision nodes `names` under `base`, the stock DRS widget's pattern.
pub fn decision_commands(group: &mut Node, base: &str, names: &[String]) -> Result<()> {
    let mut trig = Vec::new();
    let mut untrig = Vec::new();
    for n in names {
        let path = format!("{base}.{n}");
        trig.push(sig(&path, "register", "trigger"));
        trig.push(sig(&path, "query", "trigger"));
        untrig.push(sig(&path, "unregister", "untrigger"));
    }
    let cmds = group.slot_mut(COMMANDS)?;
    cmds.push(command("trigger", trig));
    cmds.push(command("untrigger", untrig));
    Ok(())
}

/// Adds a signal from `target`'s own `show` or `hide` command to `base`'s `trigger` or
/// `untrigger` pin. The command must exist exactly once.
pub fn wire_command(target: &mut Node, command_name: &str, base: &str, pin: &str) -> Result<()> {
    let owner = target.name().to_owned();
    let mut hits = target.slot_mut(COMMANDS)?.iter_mut().filter(|c| {
        c.as_obj()
            .and_then(|o| o.get("commandName"))
            .and_then(Node::as_val)
            .and_then(Val::as_str)
            == Some(command_name)
    });
    let c = hits
        .next()
        .ok_or_else(|| err(format!("{owner} has no {command_name} command")))?;
    if hits.next().is_some() {
        return Err(err(format!("{owner} has two {command_name} commands")));
    }
    c.o_mut()?
        .at_mut("signals")?
        .a_mut()?
        .push(sig(base, pin, command_name));
    Ok(())
}
