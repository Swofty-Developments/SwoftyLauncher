//! Reading and changing a node's parts in place: its name, type, props and slots, and an object's values.

use swofty_formats::pkg::{Node, Obj, Val, Value};

use super::CHILDREN;
use super::values::{err, fourcc_str, obj};
use crate::Result;

pub trait NodeExt {
    fn o(&self) -> Result<&Obj>;
    fn o_mut(&mut self) -> Result<&mut Obj>;
    fn a(&self) -> Result<&Vec<Node>>;
    fn a_mut(&mut self) -> Result<&mut Vec<Node>>;
    fn v(&self) -> Result<&Val>;
    fn v_mut(&mut self) -> Result<&mut Val>;
    fn is_node(&self) -> bool;
    fn slot(&self, i: usize) -> Result<&Vec<Node>>;
    fn slot_mut(&mut self, i: usize) -> Result<&mut Vec<Node>>;
    fn props(&self) -> Result<&Obj>;
    fn props_mut(&mut self) -> Result<&mut Obj>;
    fn name(&self) -> &str;
    fn child(&self, name: &str) -> Result<&Node>;
    fn child_mut(&mut self, name: &str) -> Result<&mut Node>;
    fn find_mut(&mut self, path: &str) -> Result<&mut Node>;
    fn type_code(&self) -> String;
}

impl NodeExt for Node {
    fn o(&self) -> Result<&Obj> {
        self.as_obj().ok_or_else(|| err("expected an object"))
    }

    fn o_mut(&mut self) -> Result<&mut Obj> {
        self.as_obj_mut().ok_or_else(|| err("expected an object"))
    }

    fn a(&self) -> Result<&Vec<Node>> {
        self.as_arr()
            .map(|a| &a.items)
            .ok_or_else(|| err("expected a list"))
    }

    fn a_mut(&mut self) -> Result<&mut Vec<Node>> {
        self.as_arr_mut()
            .map(|a| &mut a.items)
            .ok_or_else(|| err("expected a list"))
    }

    fn v(&self) -> Result<&Val> {
        self.as_val().ok_or_else(|| err("expected a value"))
    }

    fn v_mut(&mut self) -> Result<&mut Val> {
        self.as_val_mut().ok_or_else(|| err("expected a value"))
    }

    fn is_node(&self) -> bool {
        self.as_arr().is_some_and(|a| {
            a.items.len() == 8 && a.items[0].as_obj().is_some_and(|o| o.get("name").is_some())
        })
    }

    fn slot(&self, i: usize) -> Result<&Vec<Node>> {
        self.a()?.get(i).ok_or_else(|| err("short UI node"))?.a()
    }

    fn slot_mut(&mut self, i: usize) -> Result<&mut Vec<Node>> {
        self.a_mut()?
            .get_mut(i)
            .ok_or_else(|| err("short UI node"))?
            .a_mut()
    }

    fn props(&self) -> Result<&Obj> {
        self.a()?.first().ok_or_else(|| err("empty UI node"))?.o()
    }

    fn props_mut(&mut self) -> Result<&mut Obj> {
        self.a_mut()?
            .first_mut()
            .ok_or_else(|| err("empty UI node"))?
            .o_mut()
    }

    fn name(&self) -> &str {
        self.props()
            .ok()
            .and_then(|p| p.get("name"))
            .and_then(Node::as_val)
            .and_then(Val::as_str)
            .unwrap_or_default()
    }

    fn child(&self, name: &str) -> Result<&Node> {
        self.slot(CHILDREN)?
            .iter()
            .find(|c| c.is_node() && c.name() == name)
            .ok_or_else(|| err(format!("{} has no child {name}", self.name())))
    }

    fn child_mut(&mut self, name: &str) -> Result<&mut Node> {
        let parent = self.name().to_owned();
        self.slot_mut(CHILDREN)?
            .iter_mut()
            .find(|c| c.is_node() && c.name() == name)
            .ok_or_else(|| err(format!("{parent} has no child {name}")))
    }

    fn find_mut(&mut self, path: &str) -> Result<&mut Node> {
        let mut n = self;
        for part in path.split('.') {
            n = n.child_mut(part)?;
        }
        Ok(n)
    }

    fn type_code(&self) -> String {
        self.props()
            .ok()
            .and_then(|p| p.get("type"))
            .and_then(Node::as_val)
            .and_then(Val::as_u32)
            .map(fourcc_str)
            .unwrap_or_else(|| "?".into())
    }
}

pub trait ObjExt {
    fn at(&self, key: &str) -> Result<&Node>;
    fn at_mut(&mut self, key: &str) -> Result<&mut Node>;
    fn str_at(&self, key: &str) -> Result<&str>;
    fn drop_key(&mut self, key: &str);
}

impl ObjExt for Obj {
    fn at(&self, key: &str) -> Result<&Node> {
        self.get(key).ok_or_else(|| err(format!("missing key {key}")))
    }

    fn at_mut(&mut self, key: &str) -> Result<&mut Node> {
        self.get_mut(key).ok_or_else(|| err(format!("missing key {key}")))
    }

    fn str_at(&self, key: &str) -> Result<&str> {
        self.at(key)?
            .v()?
            .as_str()
            .ok_or_else(|| err(format!("{key} is not a string")))
    }

    fn drop_key(&mut self, key: &str) {
        self.items.retain(|(k, _)| k.as_str() != Some(key));
    }
}

pub fn set_value(o: &mut Obj, key: &str, value: Value) -> Result<()> {
    o.at_mut(key)?.v_mut()?.value = value;
    Ok(())
}

pub fn set_str(o: &mut Obj, key: &str, value: &str) -> Result<()> {
    set_value(o, key, Value::Str(value.into()))
}

pub fn set_prop_value(n: &mut Node, key: &str, value: Val) -> Result<()> {
    let p = n.props_mut()?;
    match p.get_mut(key) {
        Some(o) => o.o_mut()?.at_mut("value")?.v_mut()?.value = value.value,
        None => p.set(key, obj(vec![("value", Node::Val(value))])),
    }
    Ok(())
}

pub fn set_text_value(n: &mut Node, text: &str) -> Result<()> {
    set_str(n.props_mut()?.at_mut("text")?.o_mut()?, "value", text)
}

pub fn set_prop(n: &mut Node, key: &str, value: Node) -> Result<()> {
    n.props_mut()?.set(key, value);
    Ok(())
}
