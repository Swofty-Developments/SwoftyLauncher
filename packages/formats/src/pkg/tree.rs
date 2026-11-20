use crate::pkg::{STRI, Tag};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Raw(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Val {
    pub ty: Tag,
    pub value: Value,
    pub(super) slot: Option<(u32, u32)>,
}

impl Val {
    pub fn str(s: impl Into<String>) -> Self {
        Self {
            ty: STRI,
            value: Value::Str(s.into()),
            slot: None,
        }
    }

    pub fn raw(ty: Tag, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            ty,
            value: Value::Raw(bytes.into()),
            slot: None,
        }
    }

    pub fn u32(v: u32) -> Self {
        Self::raw(*b"ui32", v.to_le_bytes())
    }

    pub fn bool(v: bool) -> Self {
        Self::raw(*b"bool", [v as u8])
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Str(s) => Some(s),
            Value::Raw(_) => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match &self.value {
            Value::Raw(b) if b.len() >= 4 => Some(u32::from_le_bytes(b[..4].try_into().ok()?)),
            _ => None,
        }
    }

    pub fn set_str(&mut self, s: impl Into<String>) {
        self.value = Value::Str(s.into());
    }

    pub fn set_u32(&mut self, v: u32) {
        self.value = Value::Raw(v.to_le_bytes().to_vec());
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Val(Val),
    Obj(Obj),
    Arr(Arr),
    Iar([u8; 8]),
}

impl Node {
    pub fn as_obj(&self) -> Option<&Obj> {
        if let Node::Obj(o) = self { Some(o) } else { None }
    }

    pub fn as_obj_mut(&mut self) -> Option<&mut Obj> {
        if let Node::Obj(o) = self { Some(o) } else { None }
    }

    pub fn as_arr(&self) -> Option<&Arr> {
        if let Node::Arr(a) = self { Some(a) } else { None }
    }

    pub fn as_arr_mut(&mut self) -> Option<&mut Arr> {
        if let Node::Arr(a) = self { Some(a) } else { None }
    }

    pub fn as_val(&self) -> Option<&Val> {
        if let Node::Val(v) = self { Some(v) } else { None }
    }

    pub fn as_val_mut(&mut self) -> Option<&mut Val> {
        if let Node::Val(v) = self { Some(v) } else { None }
    }
}

#[derive(Debug, Clone)]
pub struct Obj {
    pub items: Vec<(Val, Node)>,
    pub version: u8,
}

impl Obj {
    pub fn new(items: Vec<(Val, Node)>) -> Self {
        Self { items, version: 1 }
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|(k, _)| k.as_str())
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        self.items
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        self.items
            .iter_mut()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    pub fn set(&mut self, key: &str, node: Node) {
        match self.get_mut(key) {
            Some(slot) => *slot = node,
            None => self.items.push((Val::str(key), node)),
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Node> {
        let i = self.items.iter().position(|(k, _)| k.as_str() == Some(key))?;
        Some(self.items.remove(i).1)
    }
}

#[derive(Debug, Clone)]
pub struct Arr {
    pub items: Vec<Node>,
    pub tag: Tag,
}

impl Arr {
    pub fn new(items: Vec<Node>) -> Self {
        Self { items, tag: *b"!ili" }
    }
}
