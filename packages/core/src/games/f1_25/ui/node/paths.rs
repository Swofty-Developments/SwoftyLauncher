//! Finding nodes by their dotted path, and the view-model bindings a tree holds.

use swofty_formats::pkg::{Node, Val};

use super::CHILDREN;
use super::ext::NodeExt;
use super::values::err;
use crate::Result;

/// One bound property of a template node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub node: String,
    pub prop: String,
    pub tag: Option<[u8; 4]>,
    pub db: String,
}

/// Every bound property under `x`, skipping the subtrees named in `skip`.
pub fn bindings(x: &Node, skip: &[&str], out: &mut Vec<Binding>) {
    if x.is_node() {
        if skip.contains(&x.name()) {
            return;
        }
        if let Ok(p) = x.props() {
            for (k, v) in &p.items {
                let Some(o) = v.as_obj() else { continue };
                let Some(db) = o.get("dbentry").and_then(Node::as_val).and_then(Val::as_str) else {
                    continue;
                };
                out.push(Binding {
                    node: x.name().to_owned(),
                    prop: k.as_str().unwrap_or_default().to_owned(),
                    tag: o.get("value").and_then(Node::as_val).map(|v| v.ty),
                    db: db.to_owned(),
                });
            }
        }
        if let Ok(kids) = x.slot(CHILDREN) {
            for c in kids {
                bindings(c, skip, out);
            }
        }
        return;
    }
    match x {
        Node::Arr(a) => a.items.iter().for_each(|c| bindings(c, skip, out)),
        Node::Obj(o) => o.items.iter().for_each(|(_, v)| bindings(v, skip, out)),
        _ => {}
    }
}

/// Every UI node under `x` with its dotted path (root name first).
pub fn node_paths_of<'a>(x: &'a Node, trail: &str, out: &mut Vec<(String, &'a Node)>) {
    if x.is_node() {
        let path = if trail.is_empty() {
            x.name().to_owned()
        } else {
            format!("{trail}.{}", x.name())
        };
        out.push((path.clone(), x));
        if let Ok(kids) = x.slot(CHILDREN) {
            for c in kids {
                node_paths_of(c, &path, out);
            }
        }
        return;
    }
    match x {
        Node::Arr(a) => a.items.iter().for_each(|c| node_paths_of(c, trail, out)),
        Node::Obj(o) => o.items.iter().for_each(|(_, v)| node_paths_of(v, trail, out)),
        _ => {}
    }
}

pub(super) fn locate(x: &Node, trail: &str, want: &str, at: &mut Vec<usize>, hits: &mut Vec<Vec<usize>>) {
    if x.is_node() {
        let path = if trail.is_empty() {
            x.name().to_owned()
        } else {
            format!("{trail}.{}", x.name())
        };
        if path == want {
            hits.push(at.clone());
        }
        if (want == path || want.starts_with(&format!("{path}.")))
            && let Ok(kids) = x.slot(CHILDREN)
        {
            for (i, c) in kids.iter().enumerate() {
                at.extend([CHILDREN, i]);
                locate(c, &path, want, at, hits);
                at.truncate(at.len() - 2);
            }
        }
        return;
    }
    match x {
        Node::Arr(a) => {
            for (i, c) in a.items.iter().enumerate() {
                at.push(i);
                locate(c, trail, want, at, hits);
                at.pop();
            }
        }
        Node::Obj(o) => {
            for (i, (_, v)) in o.items.iter().enumerate() {
                at.push(i);
                locate(v, trail, want, at, hits);
                at.pop();
            }
        }
        _ => {}
    }
}

/// The index path of the one UI node at the dotted `path` (root name first); its id must be the
/// djb2 of the path.
pub fn find_path(root: &Node, path: &str) -> Result<Vec<usize>> {
    let mut hits = Vec::new();
    locate(root, "", path, &mut Vec::new(), &mut hits);
    let [at] = &hits[..] else {
        return Err(err(format!("{path}: {} UI nodes there", hits.len())));
    };
    Ok(at.clone())
}

pub(super) fn step(x: &mut Node, i: usize, kids: bool) -> Result<&mut Node> {
    let miss = || err("the UI tree has changed since the node was found");
    if kids {
        return x.slot_mut(CHILDREN)?.get_mut(i).ok_or_else(miss);
    }
    match x {
        Node::Arr(a) => a.items.get_mut(i).ok_or_else(miss),
        Node::Obj(o) => o.items.get_mut(i).map(|(_, v)| v).ok_or_else(miss),
        _ => Err(miss()),
    }
}

/// The node at an index path from [`find_path`].
pub fn node_at_mut<'a>(root: &'a mut Node, at: &[usize]) -> Result<&'a mut Node> {
    let mut n = root;
    let mut i = 0;
    while i < at.len() {
        if n.is_node() {
            n = step(n, *at.get(i + 1).ok_or_else(|| err("short UI path"))?, true)?;
            i += 2;
        } else {
            n = step(n, at[i], false)?;
            i += 1;
        }
    }
    Ok(n)
}

/// The node at an index path from [`find_path`].
pub fn node_at<'a>(root: &'a Node, at: &[usize]) -> Result<&'a Node> {
    let mut n = root;
    let mut i = 0;
    let miss = || err("the UI tree has changed since the node was found");
    while i < at.len() {
        if n.is_node() {
            n = n
                .slot(CHILDREN)?
                .get(*at.get(i + 1).ok_or_else(miss)?)
                .ok_or_else(miss)?;
            i += 2;
        } else {
            n = match n {
                Node::Arr(a) => a.items.get(at[i]).ok_or_else(miss)?,
                Node::Obj(o) => o.items.get(at[i]).map(|(_, v)| v).ok_or_else(miss)?,
                _ => return Err(miss()),
            };
            i += 1;
        }
    }
    Ok(n)
}
