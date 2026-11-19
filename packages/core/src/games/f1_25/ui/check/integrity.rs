//! The djb2 hashes inside one ui file: each node's id and nameId, its layout entries, and its event, command
//! and signal names, each matching the path or name it is the hash of, and every signal's target in the file.

use std::collections::BTreeSet;

use swofty_formats::hash::djb2;
use swofty_formats::pkg::Node;

use super::findings::Finding;
use super::flows::str_of;
use crate::ui::node::*;

/// The code of a finding where a name no longer matches the djb2 hash stored
/// beside it, as after a rename that kept the hash.
pub const STALE_HASH: &str = "stale-hash";

pub(super) fn integrity_walk<'a>(n: &'a Node, parent: Option<&str>, out: &mut Vec<(String, &'a Node)>) {
    let path = match parent {
        Some(p) => format!("{p}.{}", n.name()),
        None => n.name().to_owned(),
    };
    out.push((path.clone(), n));
    for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
        integrity_walk(c, Some(&path), out);
    }
}

pub(super) fn u32_at(o: &swofty_formats::pkg::Obj, key: &str) -> Option<u32> {
    o.get(key)?.as_val()?.as_u32()
}

pub fn integrity(data: &[u8]) -> crate::Result<(usize, usize, Vec<String>)> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let mut nodes = Vec::new();
    for key in ["Screens", "Templates"] {
        for top in root
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            integrity_walk(top, None, &mut nodes);
        }
    }
    let paths: BTreeSet<&str> = nodes.iter().map(|(p, _)| p.as_str()).collect();
    let (mut bad, mut signals) = (Vec::new(), 0);
    for (path, n) in &nodes {
        let props = n.props()?;
        if props.get("id").is_some() && u32_at(props, "id") != Some(djb2(path)) {
            bad.push(format!("{path}: id"));
        }
        if props.get("nameId").is_some() && u32_at(props, "nameId") != Some(djb2(n.name())) {
            bad.push(format!("{path}: nameId"));
        }
        for grp in n.slot(LAYOUT).into_iter().flatten().filter_map(Node::as_arr) {
            for b in grp.items.iter().filter_map(Node::as_obj) {
                if let (Some(debug), Some(id)) = (str_of(b, "debugName"), u32_at(b, "nodeId"))
                    && id != djb2(debug)
                {
                    bad.push(format!("{path}: layout {debug}"));
                }
            }
        }
        for (slot, key, hash) in [
            (EVENTS, "eventName", "event"),
            (COMMANDS, "commandName", "command"),
        ] {
            for e in n.slot(slot).into_iter().flatten().filter_map(Node::as_obj) {
                let Some(name) = str_of(e, key) else { continue };
                if e.get(hash).is_some() && u32_at(e, hash) != Some(djb2(name)) {
                    bad.push(format!("{path}: {key} {name}"));
                }
                for s in e
                    .get("signals")
                    .and_then(Node::as_arr)
                    .map(|a| a.items.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Node::as_obj)
                {
                    signals += 1;
                    let to = str_of(s, "to_object_name").unwrap_or_default();
                    let pin = str_of(s, "to_pin_name").unwrap_or_default();
                    if u32_at(s, "to_object") != Some(djb2(to)) || u32_at(s, "to_pin") != Some(djb2(pin)) {
                        bad.push(format!("{path}: hash {to}.{pin}"));
                    }
                    if !paths.contains(to) {
                        bad.push(format!("{path}: on {name} -> missing {to}.{pin}"));
                    }
                }
            }
        }
    }
    Ok((nodes.len(), signals, bad))
}

pub(super) fn integrity_findings(data: &[u8], frag: &str) -> crate::Result<Vec<Finding>> {
    let p = super::parsed(data)?;
    let root = p.root.o()?;
    let tops: BTreeSet<&str> = ["Screens", "Templates"]
        .iter()
        .filter_map(|k| root.get(k).and_then(Node::as_arr))
        .flat_map(|a| a.items.iter().filter(|t| t.is_node()).map(|t| t.name()))
        .collect();
    let (_, _, bad) = integrity(data)?;
    Ok(bad
        .into_iter()
        .filter(|b| match b.split_once(" -> missing ") {
            Some((_, to)) => tops.contains(to.split('.').next().unwrap_or_default()),
            None => true,
        })
        .map(|b| {
            let (at, msg) = b.split_once(": ").unwrap_or((&b, ""));
            let code = match msg.contains(" -> missing ") {
                true => "integrity",
                false => STALE_HASH,
            };
            Finding::hard(
                code,
                &format!("{frag}: {at}"),
                msg.to_owned(),
                "format: the loader resolves nodes, events and signals by these djb2 hashes",
            )
        })
        .collect())
}
