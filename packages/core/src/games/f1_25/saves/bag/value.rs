use serde::{Deserialize, Serialize};

use super::scalar::{Float32, Float64, Hex, Hex16, Text};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Bag {
    pub version: u64,
    pub entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct List {
    pub version: u64,
    pub items: Vec<Value>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub key: Text,
    pub value: Value,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    #[serde(rename = "pbag")]
    Bag(Bag),
    #[serde(rename = "plst")]
    List(List),
    #[serde(rename = "si08")]
    I8(i8),
    #[serde(rename = "ui08")]
    U8(u8),
    #[serde(rename = "si16")]
    I16(i16),
    #[serde(rename = "ui16")]
    U16(u16),
    #[serde(rename = "si32")]
    I32(i32),
    #[serde(rename = "ui32")]
    U32(u32),
    #[serde(rename = "si64")]
    I64(i64),
    #[serde(rename = "ui64")]
    U64(u64),
    #[serde(rename = "bool")]
    Bool(bool),
    #[serde(rename = "fp32")]
    F32(Float32),
    #[serde(rename = "fp64")]
    F64(Float64),
    #[serde(rename = "rgba")]
    Rgba([u8; 4]),
    #[serde(rename = "vec2")]
    Vec2([Float32; 2]),
    #[serde(rename = "vec3")]
    Vec3([Float32; 3]),
    #[serde(rename = "vec4")]
    Vec4([Float32; 4]),
    #[serde(rename = "mat4")]
    Mat4([Float32; 16]),
    #[serde(rename = "quat")]
    Quat([Float32; 4]),
    #[serde(rename = "shnm")]
    Shnm(Hex16),
    #[serde(rename = "st32")]
    St32(Text),
    #[serde(rename = "st64")]
    St64(Text),
    #[serde(rename = "s128")]
    S128(Text),
    #[serde(rename = "s256")]
    S256(Text),
    #[serde(rename = "auid")]
    Auid(u64),
    #[serde(rename = "woid")]
    Woid(Hex16),
    #[serde(rename = "sstr")]
    Sstr(Text),
    #[serde(rename = "matp")]
    Matp(Hex16),
    #[serde(rename = "dstr")]
    Dstr(Text),
    #[serde(rename = "bffr")]
    Bffr(Hex),
    #[serde(rename = "blwt")]
    Blwt(Float32),
    #[serde(rename = "uist")]
    Uist(Text),
    #[serde(rename = "dbrf")]
    Dbrf(Option<Vec<u32>>),
}

impl Bag {
    pub fn new() -> Self {
        Bag {
            version: super::FORMAT_VERSION,
            entries: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|e| e.key.0 == key.as_bytes())
            .map(|e| &e.value)
    }

    pub fn push(&mut self, key: &str, value: Value) {
        self.entries.push(Entry {
            key: Text::new(key),
            value,
        });
    }
}

impl Default for Bag {
    fn default() -> Self {
        Bag::new()
    }
}

impl List {
    pub fn new() -> Self {
        List {
            version: super::FORMAT_VERSION,
            items: Vec::new(),
        }
    }
}

impl Default for List {
    fn default() -> Self {
        List::new()
    }
}

pub(super) const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}

impl Value {
    pub fn type_id(&self) -> u32 {
        fourcc(match self {
            Value::Bag(_) => b"pbag",
            Value::List(_) => b"plst",
            Value::I8(_) => b"si08",
            Value::U8(_) => b"ui08",
            Value::I16(_) => b"si16",
            Value::U16(_) => b"ui16",
            Value::I32(_) => b"si32",
            Value::U32(_) => b"ui32",
            Value::I64(_) => b"si64",
            Value::U64(_) => b"ui64",
            Value::Bool(_) => b"bool",
            Value::F32(_) => b"fp32",
            Value::F64(_) => b"fp64",
            Value::Rgba(_) => b"rgba",
            Value::Vec2(_) => b"vec2",
            Value::Vec3(_) => b"vec3",
            Value::Vec4(_) => b"vec4",
            Value::Mat4(_) => b"mat4",
            Value::Quat(_) => b"quat",
            Value::Shnm(_) => b"shnm",
            Value::St32(_) => b"st32",
            Value::St64(_) => b"st64",
            Value::S128(_) => b"s128",
            Value::S256(_) => b"s256",
            Value::Auid(_) => b"auid",
            Value::Woid(_) => b"woid",
            Value::Sstr(_) => b"sstr",
            Value::Matp(_) => b"matp",
            Value::Dstr(_) => b"dstr",
            Value::Bffr(_) => b"bffr",
            Value::Blwt(_) => b"blwt",
            Value::Uist(_) => b"uist",
            Value::Dbrf(_) => b"dbrf",
        })
    }
}
