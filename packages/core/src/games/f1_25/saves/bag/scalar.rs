use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

fn json_float_roundtrips(v: f64) -> bool {
    serde_json::to_string(&v)
        .ok()
        .and_then(|s| serde_json::from_str::<f64>(&s).ok())
        .is_some_and(|p| p.to_bits() == v.to_bits())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Float32(pub u32);

impl Float32 {
    pub fn value(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl Serialize for Float32 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = f64::from(self.value());
        if v.is_finite() && json_float_roundtrips(v) {
            s.serialize_f64(v)
        } else {
            let mut m = s.serialize_map(Some(1))?;
            m.serialize_entry("bits", &self.0)?;
            m.end()
        }
    }
}

struct F32Visitor;

impl<'de> Visitor<'de> for F32Visitor {
    type Value = Float32;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number or {\"bits\": u32}")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Float32, E> {
        Ok(Float32((v as f32).to_bits()))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Float32, E> {
        Ok(Float32((v as f32).to_bits()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Float32, E> {
        Ok(Float32((v as f32).to_bits()))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Float32, A::Error> {
        match m.next_entry::<String, u32>()? {
            Some((k, v)) if k == "bits" => Ok(Float32(v)),
            _ => Err(de::Error::custom("expected {\"bits\": u32}")),
        }
    }
}

impl<'de> Deserialize<'de> for Float32 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(F32Visitor)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Float64(pub u64);

impl Float64 {
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl Serialize for Float64 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = self.value();
        if v.is_finite() && json_float_roundtrips(v) {
            s.serialize_f64(v)
        } else {
            let mut m = s.serialize_map(Some(1))?;
            m.serialize_entry("bits", &self.0)?;
            m.end()
        }
    }
}

struct F64Visitor;

impl<'de> Visitor<'de> for F64Visitor {
    type Value = Float64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number or {\"bits\": u64}")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Float64, E> {
        Ok(Float64(v.to_bits()))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Float64, E> {
        Ok(Float64((v as f64).to_bits()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Float64, E> {
        Ok(Float64((v as f64).to_bits()))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Float64, A::Error> {
        match m.next_entry::<String, u64>()? {
            Some((k, v)) if k == "bits" => Ok(Float64(v)),
            _ => Err(de::Error::custom("expected {\"bits\": u64}")),
        }
    }
}

impl<'de> Deserialize<'de> for Float64 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(F64Visitor)
    }
}

pub(super) fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

pub(super) fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Hex(pub Vec<u8>);

impl Serialize for Hex {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        from_hex(&s)
            .map(Hex)
            .ok_or_else(|| de::Error::custom("expected a hex string"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Hex16(pub [u8; 16]);

impl Serialize for Hex16 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for Hex16 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        from_hex(&s)
            .and_then(|v| <[u8; 16]>::try_from(v).ok())
            .map(Hex16)
            .ok_or_else(|| de::Error::custom("expected 32 hex digits"))
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Text(pub Vec<u8>);

impl Text {
    pub fn new(s: &str) -> Self {
        Text(s.as_bytes().to_vec())
    }

    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.as_str() {
            Some(t) => s.serialize_str(t),
            None => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("hex", &to_hex(&self.0))?;
                m.end()
            }
        }
    }
}

struct TextVisitor;

impl<'de> Visitor<'de> for TextVisitor {
    type Value = Text;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string or {\"hex\": string}")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Text, E> {
        Ok(Text::new(v))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Text, A::Error> {
        match m.next_entry::<String, String>()? {
            Some((k, v)) if k == "hex" => from_hex(&v)
                .map(Text)
                .ok_or_else(|| de::Error::custom("expected a hex string")),
            _ => Err(de::Error::custom("expected {\"hex\": string}")),
        }
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(TextVisitor)
    }
}
