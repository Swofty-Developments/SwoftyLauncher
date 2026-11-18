use std::io::{Read, Seek, SeekFrom};

use crate::bytes::Reader;
use crate::{Error, Result};

const FIXED_FILE_INFO: [u8; 4] = 0xfeef_04bdu32.to_le_bytes();
const RESOURCE_DIRECTORY: usize = 2;

fn read_at(r: &mut (impl Read + Seek), offset: u64, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0; len];
    r.seek(SeekFrom::Start(offset))?;
    r.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn timestamp(r: &mut (impl Read + Seek)) -> Result<u32> {
    let dos = read_at(r, 0, 64)?;
    if !dos.starts_with(b"MZ") {
        return Err(Error::Magic {
            offset: 0,
            expected: "MZ",
        });
    }
    let pe = Reader::new(&dos, 0x3c).u32()? as u64;
    let head = read_at(r, pe, 12)?;
    if !head.starts_with(b"PE\0\0") {
        return Err(Error::Magic {
            offset: pe as usize,
            expected: "PE",
        });
    }
    Reader::new(&head, 8).u32()
}

pub fn file_version(r: &mut (impl Read + Seek)) -> Result<Option<String>> {
    let dos = read_at(r, 0, 64)?;
    if !dos.starts_with(b"MZ") {
        return Err(Error::Magic {
            offset: 0,
            expected: "MZ",
        });
    }
    let pe = Reader::new(&dos, 0x3c).u32()? as u64;
    let head = read_at(r, pe, 24)?;
    if !head.starts_with(b"PE\0\0") {
        return Err(Error::Magic {
            offset: pe as usize,
            expected: "PE",
        });
    }
    let sections = Reader::new(&head, 6).u16()? as usize;
    let optional_size = Reader::new(&head, 20).u16()? as usize;
    let optional = read_at(r, pe + 24, optional_size)?;
    let directories = match Reader::new(&optional, 0).u16()? {
        0x20b => 112,
        0x10b => 96,
        m => return Err(Error::Format(format!("unknown PE optional header {m:#x}"))),
    };
    let mut dir = Reader::new(&optional, directories + RESOURCE_DIRECTORY * 8);
    let (rva, size) = (dir.u32()?, dir.u32()? as usize);
    if rva == 0 || size == 0 {
        return Ok(None);
    }
    let table = read_at(r, pe + 24 + optional_size as u64, sections * 40)?;
    for s in table.chunks_exact(40) {
        let mut h = Reader::new(s, 8);
        let (virtual_size, va, raw_size, raw) = (h.u32()?, h.u32()?, h.u32()?, h.u32()?);
        if rva < va || rva >= va + virtual_size.max(raw_size) {
            continue;
        }
        let res = read_at(r, (raw + rva - va) as u64, size)?;
        let Some(at) = (0..res.len().saturating_sub(24))
            .step_by(4)
            .find(|&i| res[i..i + 4] == FIXED_FILE_INFO)
        else {
            return Ok(None);
        };
        let mut v = Reader::new(&res, at + 8);
        let (ms, ls) = (v.u32()?, v.u32()?);
        return Ok(Some(format!(
            "{}.{}.{}.{}",
            ms >> 16,
            ms & 0xffff,
            ls >> 16,
            ls & 0xffff
        )));
    }
    Ok(None)
}

pub fn exports(image: &[u8]) -> Option<Vec<String>> {
    let u16_at = |at: usize| Reader::new(image, at).u16().ok();
    let u32_at = |at: usize| Reader::new(image, at).u32().ok();
    let pe = u32_at(0x3c)? as usize;
    if image.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let sections = u16_at(pe + 6)? as usize;
    let opt_size = u16_at(pe + 20)? as usize;
    let opt = pe + 24;
    let dirs = match u16_at(opt)? {
        0x20b => opt + 112,
        0x10b => opt + 96,
        _ => return None,
    };
    let export_rva = u32_at(dirs)?;
    if export_rva == 0 {
        return Some(Vec::new());
    }
    let table = opt + opt_size;
    let offset = |rva: u32| -> Option<usize> {
        (0..sections.min(96)).find_map(|i| {
            let s = table + i * 40;
            let size = u64::from(u32_at(s + 8)?.max(u32_at(s + 16)?));
            let (va, raw, rva) = (
                u64::from(u32_at(s + 12)?),
                u64::from(u32_at(s + 20)?),
                u64::from(rva),
            );
            (rva >= va && rva < va + size)
                .then(|| usize::try_from(raw + (rva - va)).ok())
                .flatten()
        })
    };
    let dir = offset(export_rva)?;
    let count = u32_at(dir + 24)? as usize;
    let names = offset(u32_at(dir + 32)?)?;
    let mut out = Vec::with_capacity(count.min(4096));
    for i in 0..count.min(65536) {
        let at = offset(u32_at(names + i * 4)?)?;
        let end = image.get(at..)?.iter().position(|&c| c == 0)?;
        out.push(String::from_utf8_lossy(&image[at..at + end]).into_owned());
    }
    Some(out)
}

pub fn has_export(image: &[u8], name: &str) -> bool {
    exports(image).is_some_and(|e| e.iter().any(|n| n == name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn exe(version: Option<(u32, u32)>) -> Vec<u8> {
        let mut out = vec![0; 0x200];
        out[..2].copy_from_slice(b"MZ");
        out[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        out[0x80..0x84].copy_from_slice(b"PE\0\0");
        out[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        out[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        out[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        let dir = opt + 112 + RESOURCE_DIRECTORY * 8;
        out[dir..dir + 4].copy_from_slice(&0x3000u32.to_le_bytes());
        out[dir + 4..dir + 8].copy_from_slice(&0x100u32.to_le_bytes());
        let sec = opt + 240;
        out[sec..sec + 6].copy_from_slice(b".rsrc\0");
        for (i, v) in [0x1000u32, 0x3000, 0x200, 0x400].into_iter().enumerate() {
            out[sec + 8 + i * 4..sec + 12 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        out.resize(0x600, 0);
        if let Some((ms, ls)) = version {
            let at = 0x400 + 0x5c;
            out[at..at + 4].copy_from_slice(&FIXED_FILE_INFO);
            out[at + 8..at + 12].copy_from_slice(&ms.to_le_bytes());
            out[at + 12..at + 16].copy_from_slice(&ls.to_le_bytes());
        }
        out
    }

    fn dll(names: &[&str]) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        b[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        b[opt + 112..opt + 116].copy_from_slice(&0x1000u32.to_le_bytes());
        let sec = opt + 240;
        b[sec..sec + 6].copy_from_slice(b".edata");
        b[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes());
        b[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        b[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
        b[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes());
        let dir = 0x200;
        b[dir + 24..dir + 28].copy_from_slice(&(names.len() as u32).to_le_bytes());
        b[dir + 32..dir + 36].copy_from_slice(&0x1040u32.to_le_bytes());
        let mut at = 0x280;
        for (i, n) in names.iter().enumerate() {
            let rva = 0x1000 + (at - 0x200) as u32;
            b[0x240 + i * 4..0x244 + i * 4].copy_from_slice(&rva.to_le_bytes());
            b[at..at + n.len()].copy_from_slice(n.as_bytes());
            at += n.len() + 1;
        }
        b
    }

    #[test]
    fn reads_the_export_names() {
        let with = dll(&["SwoftyAddonApi", "SwoftyAddonInit", "SwoftyAddonRegister"]);
        assert_eq!(
            exports(&with).unwrap(),
            ["SwoftyAddonApi", "SwoftyAddonInit", "SwoftyAddonRegister"]
        );
        assert!(has_export(&with, "SwoftyAddonRegister"));
        assert!(!has_export(
            &dll(&["SwoftyAddonApi", "SwoftyAddonInit"]),
            "SwoftyAddonRegister"
        ));
        assert!(!has_export(b"not a dll", "SwoftyAddonRegister"));
    }

    #[test]
    fn reads_the_link_timestamp() {
        let mut data = exe(None);
        data[0x88..0x8c].copy_from_slice(&1_789_118_616u32.to_le_bytes());
        assert_eq!(timestamp(&mut Cursor::new(data)).unwrap(), 1_789_118_616);
        assert!(timestamp(&mut Cursor::new(vec![0; 64])).is_err());
    }

    #[test]
    fn reads_the_file_version() {
        let data = exe(Some((1 << 16, 141 << 16 | 7865)));
        assert_eq!(
            file_version(&mut Cursor::new(data)).unwrap().as_deref(),
            Some("1.0.141.7865")
        );
    }

    #[test]
    fn no_version_resource() {
        assert_eq!(file_version(&mut Cursor::new(exe(None))).unwrap(), None);
        assert!(file_version(&mut Cursor::new(vec![0; 64])).is_err());
    }
}
