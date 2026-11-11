use swofty_formats::pkg::{Arr, Node, Obj, Pkg, Val, Value};

use super::idf::{elem_name, relookup};
use super::node::NodeExt;
use crate::Result;

/// Which kind of UI resource a merge works on: it decides the key of a scene's entries and the lookup
/// table a scene carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Template,
    Scene,
}

impl Kind {
    /// The kind of a resource by its name, when it is a UI resource the merge can work on.
    pub fn of(resource: &str) -> Option<Self> {
        if resource.ends_with(".ui") {
            Some(Self::Template)
        } else if resource.ends_with(".idf") {
            Some(Self::Scene)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
struct Seg {
    key: String,
    shown: bool,
    label: String,
}

/// A path through a UI resource: the keys that find the node again, and how it reads in a warning.
#[derive(Debug, Clone, Default)]
pub struct Path(Vec<Seg>);

impl Path {
    pub fn keys(&self) -> Vec<String> {
        self.0.iter().map(|s| s.key.clone()).collect()
    }

    /// The dotted names of the UI nodes on the path, then what inside the last one is meant.
    pub fn show(&self) -> String {
        let mut names: Vec<&str> = Vec::new();
        let mut inside: Vec<&str> = Vec::new();
        for s in &self.0 {
            if s.shown {
                names.push(&s.label);
                inside.clear();
            } else {
                inside.push(&s.label);
            }
        }
        match (names.is_empty(), inside.is_empty()) {
            (_, true) => names.join("."),
            (true, false) => inside.join("/"),
            (false, false) => format!("{} [{}]", names.join("."), inside.join("/")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub path: Path,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

pub struct Merged {
    pub pkg: Pkg,
    pub conflicts: Vec<Conflict>,
    pub stats: Stats,
}

struct Cx {
    kind: Kind,
    path: Vec<Seg>,
    conflicts: Vec<Conflict>,
    stats: Stats,
}

impl Cx {
    fn conflict(&mut self) {
        self.conflicts.push(Conflict {
            path: Path(self.path.clone()),
        });
    }

    fn within<T>(&mut self, seg: Seg, f: impl FnOnce(&mut Self) -> T) -> T {
        self.path.push(seg);
        let out = f(self);
        self.path.pop();
        out
    }
}

pub fn same(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Val(x), Node::Val(y)) => same_val(x, y),
        (Node::Obj(x), Node::Obj(y)) => {
            x.version == y.version
                && x.items.len() == y.items.len()
                && x.items
                    .iter()
                    .zip(&y.items)
                    .all(|((kx, vx), (ky, vy))| same_val(kx, ky) && same(vx, vy))
        }
        (Node::Arr(x), Node::Arr(y)) => {
            x.tag == y.tag
                && x.items.len() == y.items.len()
                && x.items.iter().zip(&y.items).all(|(p, q)| same(p, q))
        }
        (Node::Iar(x), Node::Iar(y)) => format!("{x:?}") == format!("{y:?}"),
        _ => false,
    }
}

fn same_val(a: &Val, b: &Val) -> bool {
    a.ty == b.ty && a.value == b.value
}

fn val_key(v: &Val) -> String {
    match &v.value {
        Value::Str(s) => s.clone(),
        Value::Raw(b) => b.iter().map(|x| format!("{x:02x}")).collect(),
    }
}

fn key_of(n: &Node) -> Option<String> {
    if n.is_node() {
        return Some(n.name().to_owned());
    }
    n.as_obj()?.get("name")?.as_val().map(val_key)
}

fn keyed(items: &[Node]) -> Option<Vec<String>> {
    let keys: Vec<String> = items.iter().map(key_of).collect::<Option<_>>()?;
    let mut seen = std::collections::BTreeSet::new();
    keys.iter().all(|k| seen.insert(k.as_str())).then_some(keys)
}

const SLOTS: [&str; 7] = [
    "props",
    "children",
    "commands",
    "events",
    "layout",
    "component",
    "behaviours",
];

fn slot_label(i: usize) -> String {
    SLOTS
        .get(i)
        .map_or_else(|| format!("slot {i}"), |s| (*s).to_owned())
}

fn is_slotted(n: &Node) -> bool {
    n.is_node()
}

/// The node under `root` that `keys` (from [`Path::keys`]) lead to.
pub fn lookup<'a>(root: &'a Node, keys: &[String]) -> Option<&'a Node> {
    let mut n = root;
    for k in keys {
        n = match n {
            Node::Obj(o) => o.get(k)?,
            Node::Arr(a) => match k.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) {
                Some(i) if is_slotted(n) => a.items.get(i)?,
                _ => a.items.iter().find(|c| key_of(c).as_deref() == Some(k))?,
            },
            _ => return None,
        };
    }
    Some(n)
}

fn take(n: &Node) -> Node {
    super::idf::detached(n)
}

fn merge3(cx: &mut Cx, base: Option<&Node>, a: Option<&Node>, b: Option<&Node>) -> Option<Node> {
    match (a, b) {
        (None, None) => None,
        (Some(a), Some(b)) if same(a, b) => Some(a.clone()),
        (Some(a), Some(b)) => match base {
            Some(base) if same(base, a) => {
                cx.stats.changed += 1;
                Some(take(b))
            }
            Some(base) if same(base, b) => Some(a.clone()),
            Some(base) => Some(merge_both(cx, base, a, b)),
            None => Some(atomic(cx, b)),
        },
        (None, Some(b)) => match base {
            None => {
                cx.stats.added += 1;
                Some(take(b))
            }
            Some(base) if same(base, b) => None,
            Some(_) => Some(atomic(cx, b)),
        },
        (Some(a), None) => match base {
            None => Some(a.clone()),
            Some(base) if same(base, a) => {
                cx.stats.removed += 1;
                None
            }
            Some(_) => {
                cx.conflict();
                cx.stats.removed += 1;
                None
            }
        },
    }
}

fn atomic(cx: &mut Cx, b: &Node) -> Node {
    cx.conflict();
    take(b)
}

fn seg(key: &str, shown: bool, label: &str) -> Seg {
    Seg {
        key: key.to_owned(),
        shown,
        label: label.to_owned(),
    }
}

fn merge_both(cx: &mut Cx, base: &Node, a: &Node, b: &Node) -> Node {
    match (base, a, b) {
        (Node::Arr(xbase), Node::Arr(xa), Node::Arr(xb))
            if is_slotted(base) && is_slotted(a) && is_slotted(b) =>
        {
            if xa.items.len() != xbase.items.len() || xb.items.len() != xbase.items.len() {
                return atomic(cx, b);
            }
            let mut out = xa.clone();
            for i in 0..xbase.items.len() {
                let at = seg(&format!("#{i}"), false, &slot_label(i));
                let (pb, pa, pc) = (&xbase.items[i], &xa.items[i], &xb.items[i]);
                if let Some(m) = cx.within(at, |cx| merge3(cx, Some(pb), Some(pa), Some(pc))) {
                    out.items[i] = m;
                }
            }
            Node::Arr(out)
        }
        (Node::Obj(xbase), Node::Obj(xa), Node::Obj(xb)) => merge_obj(cx, xbase, xa, xb, b),
        (Node::Arr(xbase), Node::Arr(xa), Node::Arr(xb)) if xbase.tag == xa.tag && xa.tag == xb.tag => {
            merge_list(cx, xbase, xa, xb, b)
        }
        _ => atomic(cx, b),
    }
}

fn merge_obj(cx: &mut Cx, base: &Obj, a: &Obj, b: &Obj, whole: &Node) -> Node {
    if base.version != a.version || a.version != b.version {
        return atomic(cx, whole);
    }
    let mut out = Vec::new();
    for (k, va) in &a.items {
        let Some(name) = k.as_str() else {
            out.push((k.clone(), va.clone()));
            continue;
        };
        if cx.kind == Kind::Scene && name == "nodeidlookup" {
            out.push((k.clone(), va.clone()));
            continue;
        }
        let at = seg(name, false, name);
        if let Some(m) = cx.within(at, |cx| merge3(cx, base.get(name), Some(va), b.get(name))) {
            out.push((k.clone(), m));
        }
    }
    for (k, vb) in &b.items {
        let Some(name) = k.as_str() else { continue };
        if a.get(name).is_some() {
            continue;
        }
        let at = seg(name, false, name);
        if let Some(m) = cx.within(at, |cx| merge3(cx, base.get(name), None, Some(vb))) {
            out.push((fresh_key(k), m));
        }
    }
    Node::Obj(Obj {
        items: out,
        version: a.version,
    })
}

fn fresh_key(k: &Val) -> Val {
    match super::idf::detached(&Node::Val(k.clone())) {
        Node::Val(v) => v,
        _ => k.clone(),
    }
}

fn merge_list(cx: &mut Cx, base: &Arr, a: &Arr, b: &Arr, whole: &Node) -> Node {
    let (Some(kbase), Some(ka), Some(kb)) = (keyed(&base.items), keyed(&a.items), keyed(&b.items)) else {
        return atomic(cx, whole);
    };
    let pick =
        |keys: &[String], items: &[Node], k: &str| keys.iter().position(|x| x == k).map(|i| items[i].clone());
    let mut out = Vec::new();
    for (item, k) in a.items.iter().zip(&ka) {
        let at = item_seg(cx.kind, item, k);
        let merged = cx.within(at, |cx| {
            let (bi, ci) = (pick(&kbase, &base.items, k), pick(&kb, &b.items, k));
            merge3(cx, bi.as_ref(), Some(item), ci.as_ref())
        });
        out.extend(merged);
    }
    for (item, k) in b.items.iter().zip(&kb) {
        if ka.contains(k) {
            continue;
        }
        let at = item_seg(cx.kind, item, k);
        let merged = cx.within(at, |cx| {
            merge3(cx, pick(&kbase, &base.items, k).as_ref(), None, Some(item))
        });
        out.extend(merged);
    }
    Node::Arr(Arr {
        items: out,
        tag: a.tag,
    })
}

fn item_seg(kind: Kind, item: &Node, key: &str) -> Seg {
    if item.is_node() {
        return seg(key, true, key);
    }
    let obj = item.as_obj();
    if kind == Kind::Scene
        && let Some(o) = obj
    {
        if o.get("elementId").is_some() || o.get("children").is_some() {
            let n = elem_name(item);
            return seg(key, true, if n.is_empty() { key } else { n });
        }
        if let Some(scene) = o.get("scene") {
            let n = elem_name(scene);
            return seg(key, true, if n.is_empty() { key } else { n });
        }
    }
    seg(key, false, key)
}

/// Merges two addons' whole replacements of one UI resource against the game's own copy: the nodes
/// either addon added stay, an addon's change to a stock node stays, a deletion stays unless the other
/// addon edited the node, and a node the two changed differently is a [`Conflict`] that `theirs`
/// (the later addon) wins. The result is `ours` with `theirs`' changes laid in.
pub fn merge(kind: Kind, base: &Pkg, ours: &Pkg, theirs: &Pkg) -> Result<Merged> {
    let mut cx = Cx {
        kind,
        path: Vec::new(),
        conflicts: Vec::new(),
        stats: Stats::default(),
    };
    let root = merge3(&mut cx, Some(&base.root), Some(&ours.root), Some(&theirs.root))
        .unwrap_or_else(|| ours.root.clone());
    let mut pkg = ours.clone();
    pkg.root = root;
    if kind == Kind::Scene
        && let Some(list) = pkg
            .root
            .as_obj_mut()
            .and_then(|o| o.get_mut("scenelist"))
            .and_then(Node::as_arr_mut)
    {
        for entry in list.items.iter_mut().filter(|e| e.as_obj().is_some()) {
            relookup(entry)?;
        }
    }
    Ok(Merged {
        pkg,
        conflicts: cx.conflicts,
        stats: cx.stats,
    })
}
