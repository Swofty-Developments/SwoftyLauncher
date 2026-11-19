//! The values a ui node is built from: strings, numbers, flags, objects, arrays, properties and fourcc codes.

use md5::{Digest, Md5};
use swofty_formats::pkg::{Arr, Node, Obj, Val};

use crate::Error;

pub fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("menus: {msg}"))
}

pub fn fourcc(s: &str) -> u32 {
    u32::from_le_bytes(s.as_bytes()[..4].try_into().expect("four characters"))
}

pub fn fourcc_str(v: u32) -> String {
    String::from_utf8_lossy(&v.to_le_bytes()).into_owned()
}

pub fn s(v: &str) -> Node {
    Node::Val(Val::str(v))
}

pub fn u32v(v: u32) -> Node {
    Node::Val(Val::u32(v))
}

pub fn boolv(v: bool) -> Node {
    Node::Val(Val::bool(v))
}

pub fn f32v(v: f32) -> Node {
    Node::Val(Val::raw(*b"fp32", v.to_le_bytes()))
}

pub fn i32v(v: i32) -> Node {
    Node::Val(Val::raw(*b"si32", v.to_le_bytes()))
}

pub fn obj(pairs: Vec<(&str, Node)>) -> Node {
    Node::Obj(Obj::new(
        pairs.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
    ))
}

pub fn arr(items: Vec<Node>) -> Node {
    Node::Arr(Arr::new(items))
}

pub fn prop(v: Node, db: Option<&str>) -> Node {
    let mut items = vec![("value", v)];
    if let Some(db) = db {
        items.push(("dbentry", s(db)));
    }
    obj(items)
}

pub fn component_id(path: &str) -> u64 {
    let d = Md5::digest(path.as_bytes());
    u64::from_be_bytes(d[..8].try_into().expect("8 bytes"))
}
