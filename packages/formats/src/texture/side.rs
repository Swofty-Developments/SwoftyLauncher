use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::erp::Resource;
use crate::{Error, Result, lz4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideLevel {
    pub compression: u8,
    pub offset: u64,
    pub packed: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideFile {
    pub path: String,
    pub levels: Vec<SideLevel>,
}

const LZ4: u8 = 3;
const SIDE_LEVEL_LEN: usize = 26;
const MAX_LEVEL_BYTES: u64 = 1 << 28;

impl SideFile {
    pub fn parse(desc: &[u8]) -> Result<SideFile> {
        let len = *desc.first().ok_or(Error::Eof(0))? as usize;
        let path = desc.get(1..1 + len).ok_or(Error::Eof(desc.len()))?;
        let path = String::from_utf8_lossy(path)
            .trim_end_matches('\0')
            .replace('\\', "/");
        if path.is_empty()
            || path.starts_with('/')
            || path.contains(':')
            || path.split('/').any(|p| p == "..")
        {
            return Err(Error::Format(format!("bad mipmap file path {path:?}")));
        }
        let mut at = 1 + len + 1;
        let count = desc
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)
            .ok_or(Error::Eof(desc.len()))?;
        at += 4;
        let mut levels = Vec::with_capacity(count.min(32));
        for _ in 0..count {
            let entry = desc.get(at..at + SIDE_LEVEL_LEN).ok_or(Error::Eof(desc.len()))?;
            let u64_at = |o: usize| u64::from_le_bytes(entry[o..o + 8].try_into().expect("8 bytes"));
            levels.push(SideLevel {
                compression: entry[1],
                offset: u64_at(2),
                packed: u64_at(10),
                size: u64_at(18),
            });
            at += SIDE_LEVEL_LEN;
        }
        Ok(SideFile { path, levels })
    }

    pub fn read(&self, root: &Path, index: usize) -> Result<Vec<u8>> {
        let level = self
            .levels
            .get(index)
            .ok_or_else(|| Error::Format(format!("{}: no level {index}", self.path)))?;
        if level.packed > MAX_LEVEL_BYTES || level.size > MAX_LEVEL_BYTES {
            return Err(Error::Format(format!(
                "{}: level {index} is too large",
                self.path
            )));
        }
        let at = root.join(&self.path);
        let mut file = std::fs::File::open(&at).map_err(crate::io_at(&at))?;
        file.seek(SeekFrom::Start(level.offset))
            .map_err(crate::io_at(&at))?;
        let mut packed = vec![0; level.packed as usize];
        file.read_exact(&mut packed).map_err(crate::io_at(&at))?;
        let size = level.size as usize;
        match level.compression {
            0 if packed.len() == size => Ok(packed),
            LZ4 => lz4::decompress_block(&packed, size),
            c => Err(Error::Format(format!(
                "{}: level {index} has unsupported compression {c}",
                self.path
            ))),
        }
    }
}

pub fn side_file(surface: &Resource) -> Result<Option<SideFile>> {
    match surface.fragments.get(2) {
        Some(f) => SideFile::parse(&f.data()?).map(Some),
        None => Ok(None),
    }
}
