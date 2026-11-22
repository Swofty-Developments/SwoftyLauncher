mod layout;
mod read;
mod table;
mod tree;
mod write;

use crate::Result;

pub use layout::Layout;
pub use table::Table;
pub use tree::{Arr, Node, Obj, Val, Value};

pub type Tag = [u8; 4];

pub const STRI: Tag = *b"stri";
const CHUNK: u32 = 0x80;
const OFFSET_MASK: u32 = 0x00ff_ffff;

fn align_of(ty: &Tag) -> usize {
    match ty {
        b"ui64" | b"si64" => 8,
        b"vec3" | b"vec4" | b"quat" | b"woid" | b"shnm" | b"mat4" => 16,
        b"bbox" => 32,
        _ => 4,
    }
}

fn default_bpd(ty: &Tag) -> u32 {
    match ty {
        b"stri" => 0,
        b"mat4" => 64,
        b"bbox" => 32,
        b"ui64" | b"si64" => 8,
        b"ui16" => 2,
        b"ui08" | b"bool" => 1,
        t if align_of(t) >= 16 => 16,
        _ => 4,
    }
}

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: Tag,
    pub root: Node,
    pub vca: Vec<Table>,
    pub layout: Option<Layout>,
}

pub fn is_pkg(data: &[u8]) -> bool {
    data.starts_with(b"!pkg")
}

impl Pkg {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if let Some(out) = self.write(self.layout.as_ref())? {
            return Ok(out);
        }
        Ok(self.write(None)?.expect("a canonical layout always fits"))
    }
}

#[cfg(test)]
mod tests;
