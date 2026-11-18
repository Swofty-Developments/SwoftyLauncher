//! A whole ui file: its bytes, its ui data entries, its canonical form, an edit of it, and its first screen.

use swofty_formats::pkg::{Node, Obj, Pkg, Val};

use super::ext::{NodeExt, ObjExt};
use super::values::{arr, boolv, err, obj, s};
use crate::Result;

pub fn ui_bytes(screens: Vec<Node>) -> Result<Vec<u8>> {
    let p = Pkg {
        name: [0; 4],
        root: obj(vec![("Screens", arr(screens))]),
        vca: Vec::new(),
        layout: None,
    };
    Ok(p.to_bytes()?)
}

pub fn uidb_var(name: &str, kind: &str, value: Node) -> Node {
    obj(vec![
        ("ValueType", s(kind)),
        ("Name", s(name)),
        ("ReadOnly", boolv(false)),
        ("Value", value),
        ("DBType", s("Variable")),
    ])
}

pub fn uidb_dict(name: &str, entries: Vec<(String, Node)>) -> Node {
    obj(vec![
        ("Name", s(name)),
        (
            "Contents",
            Node::Obj(Obj::new(
                entries.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
            )),
        ),
        ("DBType", s("Dictionary")),
    ])
}

pub fn canonical(n: &Node) -> String {
    let list = |items: Vec<String>| items.join(",");
    match n {
        Node::Val(v) => format!("{:?}:{:?}", v.ty, v.value),
        Node::Obj(o) => format!(
            "{{{} {}}}",
            o.version,
            list(
                o.items
                    .iter()
                    .map(|(k, v)| format!("{:?}={}", k.value, canonical(v)))
                    .collect()
            )
        ),
        Node::Arr(a) => format!("[{:?} {}]", a.tag, list(a.items.iter().map(canonical).collect())),
        Node::Iar(r) => format!("{r:?}"),
    }
}

pub fn edit(data: &[u8], f: impl FnOnce(&mut Pkg) -> Result<()>) -> Result<Vec<u8>> {
    let mut p = Pkg::parse(data)?;
    f(&mut p)?;
    Ok(p.to_bytes()?)
}

pub fn root_obj(p: &mut Pkg) -> Result<&mut Obj> {
    p.root.o_mut()
}

pub fn first_screen(p: &mut Pkg) -> Result<&mut Node> {
    p.root
        .o_mut()?
        .at_mut("Screens")?
        .a_mut()?
        .first_mut()
        .ok_or_else(|| err("the UI file has no screens"))
}

pub fn first_screen_of(data: &[u8]) -> Result<Node> {
    let mut p = Pkg::parse(data)?;
    Ok(first_screen(&mut p)?.clone())
}
