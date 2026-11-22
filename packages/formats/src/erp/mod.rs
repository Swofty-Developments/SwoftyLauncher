mod encoders;
mod file;
mod fragment;
mod frame_cache;
mod packer;
mod resource;

use std::path::Path;

use self::fragment::{Payload, ReadFrom};
use crate::bytes::Reader;
use crate::{Error, Result};

pub use file::{Entry, ErpFile, replace};
pub use fragment::Fragment;
pub use frame_cache::{FrameStats, frame_stats};
pub use packer::{Packer, compress, compress_at, compress_many, work};
pub use resource::{Resource, check_name, kind};

pub const MAGIC: &[u8; 4] = b"ERPK";
pub const HEADER_LEN: usize = 0x30;
pub const ZSTD: u8 = 0x11;
pub const STORED: u8 = 0x91;
pub const ZSTD_LEVEL: i32 = 19;
/// The window every frame is written with, whatever the level: 1 MiB, the
/// largest the game's own archives use. The game's stream decoder sizes its
/// buffer from a frame's window and content size, inside a fixed heap, so a
/// level-19 window (8 MiB) on every fragment runs that heap out.
pub const ZSTD_WINDOW_LOG: u32 = 20;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
/// The waits between tries of an archive's open or replace that Windows
/// refused for now: an antivirus holds a file it scans for a moment.
const BACKOFF_MS: [u64; 6] = [25, 50, 100, 200, 400, 800];
/// The fewest index bytes a resource entry takes: its length, name length,
/// kind, the shorter type argument, fragment count and hash.
const MIN_ENTRY_LEN: usize = 4 + 2 + 16 + 4 + 1 + 16;

#[derive(Debug, Clone)]
pub struct Erp {
    pub header: [u8; HEADER_LEN],
    pub version: u32,
    pub resources: Vec<Resource>,
}

impl Erp {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (mut erp, data_off) = Self::parse_index(data)?;
        for res in &mut erp.resources {
            let from = std::sync::Arc::new(ReadFrom {
                path: None,
                resource: res.name.clone(),
            });
            for f in &mut res.fragments {
                let (packed, _) = f.stored()?;
                let raw = Reader::new(data, data_off.saturating_add(f.offset as usize))
                    .take(packed as usize)?
                    .to_vec();
                f.payload = Payload::Stored { packed, raw };
                f.from = Some(from.clone());
            }
        }
        Ok(erp)
    }

    pub fn read_matching(path: &Path, want: impl Fn(&str) -> bool) -> Result<Vec<Resource>> {
        let file = ErpFile::open(path)?;
        file.erp
            .resources
            .iter()
            .filter(|r| want(&r.name))
            .map(|r| file.load(r))
            .collect()
    }

    fn parse_index(data: &[u8]) -> Result<(Self, usize)> {
        let mut r = Reader::new(data, 0);
        let header: [u8; HEADER_LEN] = r.array()?;
        if &header[..4] != MAGIC {
            return Err(Error::Magic {
                offset: 0,
                expected: "ERPK",
            });
        }
        let version = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes"));
        let data_off = u64::from_le_bytes(header[0x20..0x28].try_into().expect("8 bytes")) as usize;
        let count = r.u32()?;
        let _fragments = r.u32()?;
        let fit = data.len().saturating_sub(r.pos) / MIN_ENTRY_LEN;
        let mut resources = Vec::with_capacity((count as usize).min(fit));
        for _ in 0..count {
            let _entry_len = r.u32()?;
            let name_len = r.u16()? as usize;
            let name = String::from_utf8_lossy(r.take(name_len)?)
                .trim_end_matches('\0')
                .to_owned();
            let kind = r.array()?;
            let unk = r.take(if version >= 4 { 6 } else { 4 })?.to_vec();
            let n = r.u8()?;
            let mut fragments = Vec::with_capacity(n as usize);
            for _ in 0..n {
                fragments.push(Fragment {
                    name: r.array()?,
                    offset: r.u64()?,
                    size: r.u64()?,
                    flags: r.u32()?,
                    compression: r.u8()?,
                    payload: Payload::Stored {
                        packed: r.u64()?,
                        raw: Vec::new(),
                    },
                    from: None,
                });
            }
            resources.push(Resource {
                name,
                kind,
                unk,
                fragments,
                hash: r.array()?,
            });
        }
        if r.pos != data_off {
            return Err(Error::Format(format!(
                "index ends at {:#x}, data starts at {data_off:#x}",
                r.pos
            )));
        }
        Ok((
            Self {
                header,
                version,
                resources,
            },
            data_off,
        ))
    }

    pub fn find(&self, name: &str) -> Result<&Resource> {
        let mut hits = self.resources.iter().filter(|r| r.name == name);
        match (hits.next(), hits.next()) {
            (Some(r), None) => Ok(r),
            (None, _) => Err(Error::Format(format!("{name}: not found"))),
            _ => Err(Error::Format(format!("{name}: more than one resource"))),
        }
    }

    pub fn find_mut(&mut self, name: &str) -> Result<&mut Resource> {
        self.find(name)?;
        Ok(self
            .resources
            .iter_mut()
            .find(|r| r.name == name)
            .expect("found above"))
    }

    /// The archive's bytes, after compressing any fragment still pending.
    pub fn to_bytes(&mut self) -> Result<Vec<u8>> {
        Packer::default().settle(self.resources.iter_mut().flat_map(|r| r.fragments.iter_mut()))?;
        let mut out = self.layout()?;
        for res in &self.resources {
            for f in &res.fragments {
                out.extend_from_slice(f.stored()?.1);
            }
        }
        Ok(out)
    }

    fn layout(&mut self) -> Result<Vec<u8>> {
        let mut index = Vec::new();
        let mut fragments = 0u32;
        let mut offset = 0u64;
        for res in &mut self.resources {
            let mut e = Vec::new();
            let name_len = res.name.len() + 1;
            e.extend_from_slice(&(name_len as u16).to_le_bytes());
            e.extend_from_slice(res.name.as_bytes());
            e.push(0);
            e.extend_from_slice(&res.kind);
            e.extend_from_slice(&res.unk);
            e.push(res.fragments.len() as u8);
            for f in &mut res.fragments {
                let (packed, _) = f.stored()?;
                f.offset = offset;
                offset += packed;
                e.extend_from_slice(&f.name);
                e.extend_from_slice(&f.offset.to_le_bytes());
                e.extend_from_slice(&f.size.to_le_bytes());
                e.extend_from_slice(&f.flags.to_le_bytes());
                e.push(f.compression);
                e.extend_from_slice(&packed.to_le_bytes());
                fragments += 1;
            }
            e.extend_from_slice(&res.hash);
            index.extend_from_slice(&(e.len() as u32).to_le_bytes());
            index.extend_from_slice(&e);
        }
        let info_len = 8 + index.len() as u64;
        let mut header = self.header;
        header[0x10..0x18].copy_from_slice(&(HEADER_LEN as u64).to_le_bytes());
        header[0x18..0x20].copy_from_slice(&info_len.to_le_bytes());
        header[0x20..0x28].copy_from_slice(&(HEADER_LEN as u64 + info_len).to_le_bytes());
        let mut out = Vec::with_capacity(HEADER_LEN + info_len as usize + offset as usize);
        out.extend_from_slice(&header);
        out.extend_from_slice(&(self.resources.len() as u32).to_le_bytes());
        out.extend_from_slice(&fragments.to_le_bytes());
        out.extend_from_slice(&index);
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
