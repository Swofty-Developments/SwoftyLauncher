use std::collections::VecDeque;

use crate::bytes::{Reader, u32_at};
use crate::pkg::layout::Layout;
use crate::pkg::table::Table;
use crate::pkg::tree::{Arr, Node, Obj, Val};
use crate::pkg::{CHUNK, OFFSET_MASK, Pkg, STRI, Tag, is_pkg};
use crate::{Error, Result};

impl Pkg {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if !is_pkg(data) {
            return Err(Error::Magic {
                offset: 0,
                expected: "!pkg",
            });
        }
        let name = data[4..8].try_into().expect("4 bytes");
        let root_ref = u32_at(data, 8)?;
        let vca_off = (u32_at(data, 12)? & OFFSET_MASK) as usize;
        if data.get(vca_off..vca_off + 4) != Some(b"!vca") {
            return Err(Error::Magic {
                offset: vca_off,
                expected: "!vca",
            });
        }
        let n = u32_at(data, vca_off + 4)? as usize;
        let mut vca = Vec::with_capacity(n);
        let mut tables_end = vca_off + 8 + 4 * n;
        for i in 0..n {
            let o = (u32_at(data, vca_off + 8 + 4 * i)? & OFFSET_MASK) as usize;
            let mut r = Reader::new(data, o);
            match &r.array::<4>()? {
                b"!sbi" => {
                    let len = r.u32()? as usize;
                    vca.push(Table::new(STRI, 0, r.take(len)?.to_vec()));
                }
                b"!vbi" => {
                    let ty = r.array()?;
                    let count = r.u32()? as usize;
                    let bpd = r.u32()?;
                    vca.push(Table::new(ty, bpd, r.take(count * bpd as usize)?.to_vec()));
                }
                _ => {
                    return Err(Error::Magic {
                        offset: o,
                        expected: "!sbi or !vbi",
                    });
                }
            }
            tables_end = tables_end.max(r.pos);
        }
        let parser = Parser { data, vca: &vca };
        let root_off = (root_ref & OFFSET_MASK) as usize;
        let root = parser.chunk(root_off)?;
        let layout = Layout::of(
            data,
            parser.chunks(root_off)?,
            vca_off as u32,
            &data[tables_end..],
        );
        Ok(Self {
            name,
            root,
            vca,
            layout,
        })
    }
}

struct Parser<'a> {
    data: &'a [u8],
    vca: &'a [Table],
}

impl Parser<'_> {
    fn val(&self, r: u32) -> Result<Node> {
        let (t, off) = (r >> 24, r & OFFSET_MASK);
        if t == CHUNK {
            return self.chunk(off as usize);
        }
        let table = self
            .vca
            .get(t as usize)
            .ok_or_else(|| Error::Format(format!("no value table {t}")))?;
        Ok(Node::Val(Val {
            ty: table.ty,
            value: table.get(off)?,
            slot: Some((t, off)),
        }))
    }

    fn chunk(&self, o: usize) -> Result<Node> {
        let mut r = Reader::new(self.data, o);
        let tag: Tag = r.array()?;
        match &tag {
            b"!idi" => {
                let h = r.u32()?;
                let (count, version) = ((h & OFFSET_MASK) as usize, (h >> 24) as u8);
                let es = if version == 1 { 12 } else { 8 };
                let mut items = Vec::with_capacity(count);
                for i in 0..count {
                    let p = o + 8 + es * i;
                    let key = match self.val(u32_at(self.data, p)?)? {
                        Node::Val(v) => v,
                        _ => return Err(Error::Format(format!("non-scalar key at {p:#x}"))),
                    };
                    items.push((key, self.val(u32_at(self.data, p + 4)?)?));
                }
                Ok(Node::Obj(Obj { items, version }))
            }
            b"!ili" | b"!vca" => {
                let count = r.u32()? as usize;
                let items = (0..count)
                    .map(|i| self.val(u32_at(self.data, o + 8 + 4 * i)?))
                    .collect::<Result<_>>()?;
                Ok(Node::Arr(Arr { items, tag }))
            }
            b"!iar" => Ok(Node::Iar(r.array()?)),
            _ => Err(Error::Format(format!(
                "unknown chunk {:?} at {o:#x}",
                String::from_utf8_lossy(&tag)
            ))),
        }
    }

    fn chunks(&self, root: usize) -> Result<Vec<(u32, u32)>> {
        let mut out = Vec::new();
        let mut queue = VecDeque::from([root]);
        while let Some(o) = queue.pop_front() {
            let h = u32_at(self.data, o + 4)?;
            let (size, refs) = match &self.data[o..o + 4] {
                b"!idi" => {
                    let es = if h >> 24 == 1 { 12 } else { 8 };
                    let n = (h & OFFSET_MASK) as usize;
                    (8 + es * n, (0..n).map(|i| o + 8 + es * i + 4).collect())
                }
                b"!iar" => (12, Vec::new()),
                _ => (
                    8 + 4 * h as usize,
                    (0..h as usize).map(|i| o + 8 + 4 * i).collect(),
                ),
            };
            for p in refs {
                let r = u32_at(self.data, p)?;
                if r >> 24 == CHUNK {
                    queue.push_back((r & OFFSET_MASK) as usize);
                }
            }
            out.push((o as u32, size as u32));
        }
        Ok(out)
    }
}
