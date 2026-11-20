use std::collections::VecDeque;

use crate::bytes::put_u32;
use crate::hash::fnv1a32;
use crate::pkg::layout::Layout;
use crate::pkg::table::Table;
use crate::pkg::tree::{Node, Val};
use crate::pkg::{Pkg, STRI, align_of, default_bpd};
use crate::{Error, Result};

impl Pkg {
    pub(super) fn write(&self, layout: Option<&Layout>) -> Result<Option<Vec<u8>>> {
        let mut w = Writer {
            out: Vec::new(),
            vca: self.vca.clone(),
            layout,
            emitted: 0,
        };
        w.out.extend_from_slice(b"!pkg");
        w.out.extend_from_slice(&self.name);
        w.out.extend_from_slice(&[0; 8]);
        if let Some(l) = layout {
            w.out.resize(l.end as usize, 0);
            for (at, gap) in &l.gaps {
                w.out[*at as usize..*at as usize + gap.len()].copy_from_slice(gap);
            }
        }
        let mut queue = VecDeque::new();
        let Some(root_off) = w.emit(&self.root, &mut queue)? else {
            return Ok(None);
        };
        while let Some((slot, node)) = queue.pop_front() {
            let Some(off) = w.emit(node, &mut queue)? else {
                return Ok(None);
            };
            put_u32(&mut w.out, slot, 0x8000_0000 | off as u32);
        }
        if layout.is_some_and(|l| l.chunks.len() != w.emitted) {
            return Ok(None);
        }
        w.pad(4);
        let vca_off = w.out.len();
        w.out.extend_from_slice(b"!vca");
        w.out.extend_from_slice(&(w.vca.len() as u32).to_le_bytes());
        let table = w.out.len();
        w.out.resize(table + 4 * w.vca.len(), 0);
        for i in 0..w.vca.len() {
            let ty = w.vca[i].ty;
            w.pad(align_of(&ty));
            let pos = w.out.len() as u32;
            put_u32(&mut w.out, table + 4 * i, 0x8000_0000 | pos);
            let t = &w.vca[i];
            let mut block = Vec::with_capacity(t.raw.len() + 16);
            if t.ty == STRI {
                block.extend_from_slice(b"!sbi");
                block.extend_from_slice(&(t.raw.len() as u32).to_le_bytes());
            } else {
                block.extend_from_slice(b"!vbi");
                block.extend_from_slice(&t.ty);
                block.extend_from_slice(&(t.count() as u32).to_le_bytes());
                block.extend_from_slice(&t.bpd.to_le_bytes());
            }
            block.extend_from_slice(&t.raw);
            w.out.extend_from_slice(&block);
        }
        match layout {
            Some(l) => w.out.extend_from_slice(&l.tail),
            None => w.pad(4),
        }
        put_u32(&mut w.out, 8, 0x8000_0000 | root_off as u32);
        put_u32(&mut w.out, 12, 0x8000_0000 | vca_off as u32);
        Ok(Some(w.out))
    }
}

struct Writer<'l> {
    out: Vec<u8>,
    vca: Vec<Table>,
    layout: Option<&'l Layout>,
    emitted: usize,
}

impl Writer<'_> {
    fn pad(&mut self, align: usize) {
        while !self.out.len().is_multiple_of(align) {
            self.out.push(0);
        }
    }

    fn slot(&mut self, v: &Val) -> Result<u32> {
        if let Some((t, off)) = v.slot
            && let Some(table) = self.vca.get(t as usize)
            && table.ty == v.ty
            && table.get(off).ok().as_ref() == Some(&v.value)
        {
            return Ok(t << 24 | off);
        }
        let t = match self.vca.iter().position(|d| d.ty == v.ty) {
            Some(t) => t,
            None => {
                self.vca.push(Table::new(v.ty, default_bpd(&v.ty), Vec::new()));
                self.vca.len() - 1
            }
        };
        let off = self.vca[t].intern(&v.value)?;
        Ok((t as u32) << 24 | off)
    }

    fn place(&mut self, size: usize) -> Option<usize> {
        let i = self.emitted;
        self.emitted += 1;
        match self.layout {
            None => {
                let start = self.out.len();
                self.out.resize(start + size, 0);
                Some(start)
            }
            Some(l) => l
                .chunks
                .get(i)
                .filter(|&&(_, n)| n as usize == size)
                .map(|&(at, _)| at as usize),
        }
    }

    fn emit<'n>(&mut self, node: &'n Node, queue: &mut VecDeque<(usize, &'n Node)>) -> Result<Option<usize>> {
        let size = match node {
            Node::Obj(obj) => 8 + if obj.version == 1 { 12 } else { 8 } * obj.items.len(),
            Node::Arr(arr) => 8 + 4 * arr.items.len(),
            Node::Iar(_) => 12,
            Node::Val(_) => return Err(Error::Format("a scalar cannot be a chunk".into())),
        };
        let Some(start) = self.place(size) else {
            return Ok(None);
        };
        let table = start + 8;
        match node {
            Node::Obj(obj) => {
                let es = if obj.version == 1 { 12 } else { 8 };
                self.out[start..start + 4].copy_from_slice(b"!idi");
                put_u32(
                    &mut self.out,
                    start + 4,
                    obj.items.len() as u32 | (obj.version as u32) << 24,
                );
                let mut items: Vec<&(Val, Node)> = obj.items.iter().collect();
                if obj.version == 1 {
                    items.sort_by_key(|(k, _)| fnv1a32(k.as_str().unwrap_or_default()));
                }
                for (i, (k, v)) in items.into_iter().enumerate() {
                    let p = table + es * i;
                    let ks = self.slot(k)?;
                    put_u32(&mut self.out, p, ks);
                    if obj.version == 1 {
                        put_u32(&mut self.out, p + 8, fnv1a32(k.as_str().unwrap_or_default()));
                    }
                    match v {
                        Node::Val(v) => {
                            let vs = self.slot(v)?;
                            put_u32(&mut self.out, p + 4, vs);
                        }
                        other => queue.push_back((p + 4, other)),
                    }
                }
            }
            Node::Arr(arr) => {
                self.out[start..start + 4].copy_from_slice(&arr.tag);
                put_u32(&mut self.out, start + 4, arr.items.len() as u32);
                for (i, v) in arr.items.iter().enumerate() {
                    match v {
                        Node::Val(v) => {
                            let vs = self.slot(v)?;
                            put_u32(&mut self.out, table + 4 * i, vs);
                        }
                        other => queue.push_back((table + 4 * i, other)),
                    }
                }
            }
            Node::Iar(raw) => {
                self.out[start..start + 4].copy_from_slice(b"!iar");
                self.out[start + 4..start + 12].copy_from_slice(raw);
            }
            Node::Val(_) => {}
        }
        Ok(Some(start))
    }
}
