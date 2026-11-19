use super::error::DecodeError;
use super::scalar::{Float32, Float64, Hex, Hex16, Text};
use super::value::{Bag, Entry, List, Value, fourcc};
use super::{FORMAT_VERSION, MAX_COUNT};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_depth: 64,
            max_nodes: 8_000_000,
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    nodes: usize,
    limits: Limits,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if n > self.remaining() {
            return Err(DecodeError::Truncated {
                at: self.pos,
                need: n - self.remaining(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn floats<const N: usize>(&mut self) -> Result<[Float32; N], DecodeError> {
        let mut out = [Float32(0); N];
        for f in &mut out {
            *f = Float32(self.u32()?);
        }
        Ok(out)
    }

    fn node(&mut self) -> Result<(), DecodeError> {
        self.nodes += 1;
        if self.nodes > self.limits.max_nodes {
            return Err(DecodeError::TooManyNodes {
                max: self.limits.max_nodes,
            });
        }
        Ok(())
    }

    fn version(&mut self) -> Result<u64, DecodeError> {
        let at = self.pos;
        let version = self.u64()?;
        if version > FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion { at, found: version });
        }
        Ok(version)
    }

    fn count(&mut self, min_item: usize) -> Result<usize, DecodeError> {
        let at = self.pos;
        let count = self.u64()?;
        let fits = count <= MAX_COUNT && (count as usize).saturating_mul(min_item) <= self.remaining();
        if !fits {
            return Err(DecodeError::CountTooLarge { at, count });
        }
        Ok(count as usize)
    }

    fn text8(&mut self) -> Result<Text, DecodeError> {
        let n = usize::from(self.u8()?);
        Ok(Text(self.take(n)?.to_vec()))
    }

    fn text16(&mut self) -> Result<Text, DecodeError> {
        let n = usize::from(self.u16()?);
        Ok(Text(self.take(n)?.to_vec()))
    }

    fn text32(&mut self) -> Result<Text, DecodeError> {
        let n = self.u32()? as usize;
        Ok(Text(self.take(n)?.to_vec()))
    }

    fn bag(&mut self, depth: usize) -> Result<Bag, DecodeError> {
        if depth > self.limits.max_depth {
            return Err(DecodeError::TooDeep {
                at: self.pos,
                max: self.limits.max_depth,
            });
        }
        let version = self.version()?;
        let count = self.count(5)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            self.node()?;
            let key = self.text8()?;
            let value = self.value(depth)?;
            entries.push(Entry { key, value });
        }
        Ok(Bag { version, entries })
    }

    fn list(&mut self, depth: usize) -> Result<List, DecodeError> {
        if depth > self.limits.max_depth {
            return Err(DecodeError::TooDeep {
                at: self.pos,
                max: self.limits.max_depth,
            });
        }
        let version = self.version()?;
        let count = self.count(4)?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            self.node()?;
            items.push(self.value(depth)?);
        }
        Ok(List { version, items })
    }

    fn value(&mut self, depth: usize) -> Result<Value, DecodeError> {
        let at = self.pos;
        let id = self.u32()?;
        let v = match id {
            x if x == fourcc(b"pbag") => Value::Bag(self.bag(depth + 1)?),
            x if x == fourcc(b"plst") => Value::List(self.list(depth + 1)?),
            x if x == fourcc(b"si08") => Value::I8(i8::from_le_bytes(self.array()?)),
            x if x == fourcc(b"ui08") => Value::U8(self.u8()?),
            x if x == fourcc(b"si16") => Value::I16(i16::from_le_bytes(self.array()?)),
            x if x == fourcc(b"ui16") => Value::U16(self.u16()?),
            x if x == fourcc(b"si32") => Value::I32(i32::from_le_bytes(self.array()?)),
            x if x == fourcc(b"ui32") => Value::U32(self.u32()?),
            x if x == fourcc(b"si64") => Value::I64(i64::from_le_bytes(self.array()?)),
            x if x == fourcc(b"ui64") => Value::U64(self.u64()?),
            x if x == fourcc(b"bool") => {
                let p = self.pos;
                match self.u8()? {
                    0 => Value::Bool(false),
                    1 => Value::Bool(true),
                    _ => return Err(DecodeError::NonCanonical { at: p, what: "bool" }),
                }
            }
            x if x == fourcc(b"fp32") => Value::F32(Float32(self.u32()?)),
            x if x == fourcc(b"fp64") => Value::F64(Float64(self.u64()?)),
            x if x == fourcc(b"rgba") => Value::Rgba(self.array()?),
            x if x == fourcc(b"vec2") => Value::Vec2(self.floats()?),
            x if x == fourcc(b"vec3") => Value::Vec3(self.floats()?),
            x if x == fourcc(b"vec4") => Value::Vec4(self.floats()?),
            x if x == fourcc(b"mat4") => Value::Mat4(self.floats()?),
            x if x == fourcc(b"quat") => Value::Quat(self.floats()?),
            x if x == fourcc(b"shnm") => Value::Shnm(Hex16(self.array()?)),
            x if x == fourcc(b"st32") => Value::St32(self.text8()?),
            x if x == fourcc(b"st64") => Value::St64(self.text8()?),
            x if x == fourcc(b"s128") => Value::S128(self.text8()?),
            x if x == fourcc(b"s256") => Value::S256(self.text16()?),
            x if x == fourcc(b"auid") => Value::Auid(self.u64()?),
            x if x == fourcc(b"woid") => Value::Woid(Hex16(self.array()?)),
            x if x == fourcc(b"sstr") => Value::Sstr(self.text16()?),
            x if x == fourcc(b"matp") => Value::Matp(Hex16(self.array()?)),
            x if x == fourcc(b"dstr") => Value::Dstr(self.text32()?),
            x if x == fourcc(b"bffr") => {
                let n = self.u32()? as usize;
                Value::Bffr(Hex(self.take(n)?.to_vec()))
            }
            x if x == fourcc(b"blwt") => Value::Blwt(Float32(self.u32()?)),
            x if x == fourcc(b"uist") => Value::Uist(self.text32()?),
            x if x == fourcc(b"dbrf") => {
                let p = self.pos;
                match self.u8()? {
                    0 => Value::Dbrf(None),
                    1 => {
                        let n = usize::from(self.u8()?);
                        let mut ids = Vec::with_capacity(n);
                        for _ in 0..n {
                            ids.push(self.u32()?);
                        }
                        Value::Dbrf(Some(ids))
                    }
                    _ => {
                        return Err(DecodeError::NonCanonical {
                            at: p,
                            what: "dbrf flag",
                        });
                    }
                }
            }
            _ => {
                return Err(DecodeError::UnknownType {
                    at,
                    id,
                    name: id
                        .to_le_bytes()
                        .iter()
                        .map(|b| char::from(*b).escape_default().to_string())
                        .collect(),
                });
            }
        };
        Ok(v)
    }
}

pub fn decode_with(bytes: &[u8], limits: Limits) -> Result<(Bag, usize), DecodeError> {
    let mut r = Reader {
        buf: bytes,
        pos: 0,
        nodes: 0,
        limits,
    };
    let bag = r.bag(0)?;
    Ok((bag, r.pos))
}

pub fn decode_prefix(bytes: &[u8]) -> Result<(Bag, usize), DecodeError> {
    decode_with(bytes, Limits::default())
}

pub fn decode(bytes: &[u8]) -> Result<Bag, DecodeError> {
    let (bag, used) = decode_prefix(bytes)?;
    if used != bytes.len() {
        return Err(DecodeError::TrailingBytes {
            at: used,
            len: bytes.len() - used,
        });
    }
    Ok(bag)
}
