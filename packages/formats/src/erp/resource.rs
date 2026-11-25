use md5::{Digest, Md5};

use crate::erp::fragment::Fragment;
use crate::erp::packer::Packer;
use crate::erp::{ZSTD, ZSTD_LEVEL};
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Resource {
    pub name: String,
    pub kind: [u8; 16],
    pub unk: Vec<u8>,
    pub fragments: Vec<Fragment>,
    pub hash: [u8; 16],
}

impl Resource {
    pub fn kind(&self) -> String {
        String::from_utf8_lossy(&self.kind)
            .trim_end_matches('\0')
            .to_owned()
    }

    pub fn content_hash(&self) -> Result<[u8; 16]> {
        let mut h = Md5::new();
        for f in &self.fragments {
            h.update(f.data()?);
        }
        Ok(h.finalize().into())
    }

    pub fn like(name: &str, template: &Resource, data: &[Vec<u8>]) -> Result<Self> {
        Self::like_with(&Packer::default(), name, template, data)
    }

    /// A resource named `name` with `template`'s kind and fragments holding `data`, compressed by `packer`.
    pub fn like_with(packer: &Packer, name: &str, template: &Resource, data: &[Vec<u8>]) -> Result<Self> {
        check_name(name)?;
        if data.len() != template.fragments.len() {
            return Err(Error::Format(format!(
                "{name}: {} fragment(s) given, a {} like {} has {}",
                data.len(),
                template.kind(),
                template.name,
                template.fragments.len()
            )));
        }
        let mut res = Resource {
            name: name.to_owned(),
            kind: template.kind,
            unk: template.unk.clone(),
            fragments: Vec::with_capacity(data.len()),
            hash: [0; 16],
        };
        for (t, d) in template.fragments.iter().zip(data) {
            let mut f = Fragment::new(t.name, 0, t.flags, t.compression, Vec::new());
            fill(packer, &mut f, d)?;
            res.fragments.push(f);
        }
        res.hash = res.content_hash()?;
        Ok(res)
    }

    pub fn has_stats(&self) -> bool {
        matches!(&self.fragments[..], [_, s, ..] if s.size == crate::bxml::STATS_LEN as u64)
    }

    pub fn stale_stats(&self) -> Result<bool> {
        match &self.fragments[..] {
            [data, stats, ..] if self.has_stats() => {
                Ok(crate::bxml::stale_stats(&data.data()?, &stats.data()?))
            }
            _ => Ok(false),
        }
    }

    // Rewrites stale TSMD stats from fragment 0, as the game's own tools write them;
    // true when it did. Fails when fragment 0 starts as binary XML but does not parse.
    pub fn repair_stats(&mut self) -> Result<bool> {
        self.repair_stats_with(&Packer::default())
    }

    /// [`Resource::repair_stats`], compressing the new stats with `packer`.
    pub fn repair_stats_with(&mut self, packer: &Packer) -> Result<bool> {
        if !self.stale_stats()? {
            return Ok(false);
        }
        let stats = self.fragments[1].data()?;
        let version = u32::from_le_bytes(stats[4..8].try_into().expect("4 bytes"));
        let doc = crate::bxml::Document::parse(&self.fragments[0].data()?)?;
        self.set_fragment_with(packer, 1, &doc.string_stats(version))?;
        Ok(true)
    }

    pub fn set_fragment(&mut self, index: usize, data: &[u8]) -> Result<()> {
        self.set_fragment_with(&Packer::default(), index, data)
    }

    /// Puts `data` in fragment `index`, compressed by `packer` when the fragment is zstd, and rehashes.
    pub fn set_fragment_with(&mut self, packer: &Packer, index: usize, data: &[u8]) -> Result<()> {
        let f = self
            .fragments
            .get_mut(index)
            .ok_or_else(|| Error::Format(format!("{}: no fragment {index}", self.name)))?;
        fill(packer, f, data)?;
        self.hash = self.content_hash()?;
        Ok(())
    }
}

fn fill(packer: &Packer, f: &mut Fragment, data: &[u8]) -> Result<()> {
    if (0x20..=0x2f).contains(&f.compression) {
        f.compression = ZSTD;
    }
    let zstd = f.zstd_coded();
    packer.put(f, data, zstd, ZSTD_LEVEL)
}

pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('\0') || name.len() >= u16::MAX as usize {
        return Err(Error::Format(format!(
            "{name:?} is not a resource name: it must be 1 to {} bytes without NUL",
            u16::MAX - 1
        )));
    }
    Ok(())
}

pub fn kind(name: &str) -> Result<[u8; 16]> {
    let b = name.as_bytes();
    if b.is_empty() || b.len() >= 16 || b.contains(&0) {
        return Err(Error::Format(format!(
            "{name:?} is not a resource type: it must be 1 to 15 bytes without NUL"
        )));
    }
    let mut out = [0; 16];
    out[..b.len()].copy_from_slice(b);
    Ok(out)
}
