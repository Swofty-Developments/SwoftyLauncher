use super::FORMAT_VERSION;
use super::MAX_COUNT;
use super::decode::Limits;
use super::error::EncodeError;
use super::value::{Bag, List, Value};

struct Writer {
    out: Vec<u8>,
    max_depth: usize,
}

impl Writer {
    fn text8(&mut self, what: &'static str, bytes: &[u8]) -> Result<(), EncodeError> {
        let n = u8::try_from(bytes.len()).map_err(|_| EncodeError::TooLong {
            what,
            len: bytes.len(),
            max: 0xFF,
        })?;
        self.out.push(n);
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    fn text16(&mut self, what: &'static str, bytes: &[u8]) -> Result<(), EncodeError> {
        let n = u16::try_from(bytes.len()).map_err(|_| EncodeError::TooLong {
            what,
            len: bytes.len(),
            max: 0xFFFF,
        })?;
        self.out.extend_from_slice(&n.to_le_bytes());
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    fn text32(&mut self, what: &'static str, bytes: &[u8]) -> Result<(), EncodeError> {
        let n = u32::try_from(bytes.len()).map_err(|_| EncodeError::TooLong {
            what,
            len: bytes.len(),
            max: u32::MAX as usize,
        })?;
        self.out.extend_from_slice(&n.to_le_bytes());
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    fn header(&mut self, what: &'static str, version: u64, len: usize) -> Result<(), EncodeError> {
        if version > FORMAT_VERSION {
            return Err(EncodeError::UnsupportedVersion { found: version });
        }
        if len as u64 > MAX_COUNT {
            return Err(EncodeError::TooMany {
                what,
                len,
                max: MAX_COUNT as usize,
            });
        }
        self.out.extend_from_slice(&version.to_le_bytes());
        self.out.extend_from_slice(&(len as u64).to_le_bytes());
        Ok(())
    }

    fn bag(&mut self, bag: &Bag, depth: usize) -> Result<(), EncodeError> {
        if depth > self.max_depth {
            return Err(EncodeError::TooDeep { max: self.max_depth });
        }
        self.header("a bag", bag.version, bag.entries.len())?;
        for e in &bag.entries {
            self.text8("key", &e.key.0)?;
            self.value(&e.value, depth)?;
        }
        Ok(())
    }

    fn list(&mut self, list: &List, depth: usize) -> Result<(), EncodeError> {
        if depth > self.max_depth {
            return Err(EncodeError::TooDeep { max: self.max_depth });
        }
        self.header("a list", list.version, list.items.len())?;
        for v in &list.items {
            self.value(v, depth)?;
        }
        Ok(())
    }

    fn floats(&mut self, f: &[super::Float32]) {
        for x in f {
            self.out.extend_from_slice(&x.0.to_le_bytes());
        }
    }

    fn value(&mut self, v: &Value, depth: usize) -> Result<(), EncodeError> {
        self.out.extend_from_slice(&v.type_id().to_le_bytes());
        match v {
            Value::Bag(b) => self.bag(b, depth + 1)?,
            Value::List(l) => self.list(l, depth + 1)?,
            Value::I8(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::U8(x) => self.out.push(*x),
            Value::I16(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::U16(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::I32(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::U32(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::I64(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::U64(x) | Value::Auid(x) => self.out.extend_from_slice(&x.to_le_bytes()),
            Value::Bool(x) => self.out.push(u8::from(*x)),
            Value::F32(x) | Value::Blwt(x) => self.out.extend_from_slice(&x.0.to_le_bytes()),
            Value::F64(x) => self.out.extend_from_slice(&x.0.to_le_bytes()),
            Value::Rgba(x) => self.out.extend_from_slice(x),
            Value::Vec2(x) => self.floats(x),
            Value::Vec3(x) => self.floats(x),
            Value::Vec4(x) | Value::Quat(x) => self.floats(x),
            Value::Mat4(x) => self.floats(x),
            Value::Shnm(x) | Value::Woid(x) | Value::Matp(x) => self.out.extend_from_slice(&x.0),
            Value::St32(t) => self.text8("st32", &t.0)?,
            Value::St64(t) => self.text8("st64", &t.0)?,
            Value::S128(t) => self.text8("s128", &t.0)?,
            Value::S256(t) => self.text16("s256", &t.0)?,
            Value::Sstr(t) => self.text16("sstr", &t.0)?,
            Value::Dstr(t) => self.text32("dstr", &t.0)?,
            Value::Uist(t) => self.text32("uist", &t.0)?,
            Value::Bffr(b) => self.text32("bffr", &b.0)?,
            Value::Dbrf(None) => self.out.push(0),
            Value::Dbrf(Some(ids)) => {
                let n = u8::try_from(ids.len()).map_err(|_| EncodeError::TooManyIds {
                    len: ids.len(),
                    max: 0xFF,
                })?;
                self.out.push(1);
                self.out.push(n);
                for id in ids {
                    self.out.extend_from_slice(&id.to_le_bytes());
                }
            }
        }
        Ok(())
    }
}

pub fn encode(bag: &Bag) -> Result<Vec<u8>, EncodeError> {
    let mut w = Writer {
        out: Vec::new(),
        max_depth: Limits::default().max_depth,
    };
    w.bag(bag, 0)?;
    Ok(w.out)
}
