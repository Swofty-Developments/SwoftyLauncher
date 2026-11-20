//! Events, commands and the signals between nodes.

use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Obj, Val};

use super::ext::{NodeExt, ObjExt};
use super::values::{arr, boolv, f32v, obj, s, u32v};
use super::{CHILDREN, EVENTS};
use crate::Result;

#[derive(Debug, Clone)]
pub enum Attr {
    Str(String),
    Bool(bool),
    F32(f32),
    U32(u32),
    Map(Vec<(String, Attr)>),
}

impl From<&str> for Attr {
    fn from(v: &str) -> Self {
        Attr::Str(v.into())
    }
}

impl From<String> for Attr {
    fn from(v: String) -> Self {
        Attr::Str(v)
    }
}

pub(super) fn attr(v: &Attr) -> Node {
    match v {
        Attr::Map(items) => Node::Obj(Obj::new(
            items
                .iter()
                .map(|(k, x)| (Val::str(k.as_str()), attr(x)))
                .collect(),
        )),
        Attr::Bool(b) => obj(vec![("value", boolv(*b))]),
        Attr::F32(f) => obj(vec![("value", f32v(*f))]),
        Attr::U32(u) => obj(vec![("value", u32v(*u))]),
        Attr::Str(t) => obj(vec![("value", s(t))]),
    }
}

pub fn sig_with(to: &str, pin: &str, from: &str, attrs: &[(&str, Attr)]) -> Node {
    let mut items = vec![
        ("to_object", u32v(djb2(to))),
        ("to_object_name", s(to)),
        ("to_pin_name", s(pin)),
        ("to_pin", u32v(djb2(pin))),
        ("from_pin_name", s(from)),
    ];
    if !attrs.is_empty() {
        items.push((
            "attributes",
            obj(attrs.iter().map(|(k, v)| (*k, attr(v))).collect()),
        ));
    }
    obj(items)
}

pub fn sig(to: &str, pin: &str, from: &str) -> Node {
    sig_with(to, pin, from, &[])
}

pub fn event(name: &str, signals: Vec<Node>) -> Node {
    obj(vec![
        ("eventName", s(name)),
        ("signals", arr(signals)),
        ("event", u32v(djb2(name))),
    ])
}

pub fn command(name: &str, signals: Vec<Node>) -> Node {
    obj(vec![
        ("commandName", s(name)),
        ("command", u32v(djb2(name))),
        ("signals", arr(signals)),
    ])
}

pub(super) fn event_name(e: &Node) -> Option<&str> {
    e.as_obj()?.get("eventName")?.as_val()?.as_str()
}

pub fn add_events(n: &mut Node, evs: Vec<Node>) -> Result<()> {
    let events = n.slot_mut(EVENTS)?;
    for ev in evs {
        let name = event_name(&ev).unwrap_or_default().to_owned();
        match events.iter_mut().find(|e| event_name(e) == Some(&name)) {
            Some(same) => {
                let new = ev.o()?.at("signals")?.a()?.clone();
                same.o_mut()?.at_mut("signals")?.a_mut()?.extend(new);
            }
            None => events.push(ev),
        }
    }
    Ok(())
}

pub fn raise_cmd(screen: &str, from: &str, cmd: &str, data: &str) -> Node {
    let c = Attr::Map(vec![("name".into(), cmd.into()), ("data".into(), data.into())]);
    sig_with(screen, "raise_command", from, &[("command", c)])
}

pub fn drop_events_to(n: &mut Node, pins: &[&str]) -> Result<()> {
    let events = n.slot_mut(EVENTS)?;
    for e in events.iter_mut() {
        let sigs = e.o_mut()?.at_mut("signals")?.a_mut()?;
        sigs.retain(|s| {
            !s.as_obj()
                .and_then(|o| o.get("to_pin_name"))
                .and_then(Node::as_val)
                .and_then(Val::as_str)
                .is_some_and(|p| pins.contains(&p))
        });
    }
    events.retain(|e| {
        e.as_obj()
            .and_then(|o| o.get("signals"))
            .and_then(Node::as_arr)
            .is_some_and(|a| !a.items.is_empty())
    });
    for c in n.slot_mut(CHILDREN)? {
        if c.is_node() {
            drop_events_to(c, pins)?;
        }
    }
    Ok(())
}
