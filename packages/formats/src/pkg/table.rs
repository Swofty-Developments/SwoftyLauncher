use std::collections::HashMap;

use crate::pkg::tree::Value;
use crate::pkg::{STRI, Tag};
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Table {
    pub ty: Tag,
    pub bpd: u32,
    pub raw: Vec<u8>,
    index: Option<HashMap<Value, u32>>,
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Value::Str(s) => s.hash(state),
            Value::Raw(b) => b.hash(state),
        }
    }
}

impl Eq for Value {}

impl Table {
    pub(super) fn new(ty: Tag, bpd: u32, raw: Vec<u8>) -> Self {
        Self {
            ty,
            bpd,
            raw,
            index: None,
        }
    }

    pub fn count(&self) -> usize {
        if self.bpd == 0 {
            0
        } else {
            self.raw.len() / self.bpd as usize
        }
    }

    pub fn get(&self, idx: u32) -> Result<Value> {
        let idx = idx as usize;
        if self.ty == STRI {
            let n = *self.raw.get(idx).ok_or(Error::Eof(idx))? as usize;
            let s = self.raw.get(idx + 1..idx + 1 + n).ok_or(Error::Eof(idx))?;
            return Ok(Value::Str(String::from_utf8_lossy(s).into_owned()));
        }
        let bpd = self.bpd as usize;
        let b = self
            .raw
            .get(idx * bpd..(idx + 1) * bpd)
            .ok_or(Error::Eof(idx * bpd))?;
        Ok(Value::Raw(b.to_vec()))
    }

    fn build_index(&self) -> HashMap<Value, u32> {
        let mut index = HashMap::new();
        if self.ty == STRI {
            let mut o = 0;
            while o < self.raw.len() {
                let n = self.raw[o] as usize;
                let end = (o + 1 + n).min(self.raw.len());
                let s = String::from_utf8_lossy(&self.raw[o + 1..end]).into_owned();
                index.entry(Value::Str(s)).or_insert(o as u32);
                o += n + 2;
            }
        } else {
            for i in 0..self.count() {
                let bpd = self.bpd as usize;
                index
                    .entry(Value::Raw(self.raw[i * bpd..(i + 1) * bpd].to_vec()))
                    .or_insert(i as u32);
            }
        }
        index
    }

    pub(super) fn intern(&mut self, value: &Value) -> Result<u32> {
        if self.index.is_none() {
            self.index = Some(self.build_index());
        }
        if let Some(&o) = self.index.as_ref().and_then(|i| i.get(value)) {
            return Ok(o);
        }
        let o = match value {
            Value::Str(s) => {
                let b = s.as_bytes();
                if b.len() > 255 {
                    return Err(Error::Format(format!("string longer than 255 bytes: {s}")));
                }
                let o = self.raw.len() as u32;
                self.raw.push(b.len() as u8);
                self.raw.extend_from_slice(b);
                self.raw.push(0);
                o
            }
            Value::Raw(b) => {
                let mut b = b.clone();
                b.resize(self.bpd as usize, 0);
                let i = self.count() as u32;
                self.raw.extend_from_slice(&b);
                i
            }
        };
        self.index.as_mut().expect("built above").insert(value.clone(), o);
        Ok(o)
    }
}
