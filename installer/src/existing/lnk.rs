use std::path::{Path, PathBuf};

use crate::existing::roots::root_of;

#[derive(Debug, Default, PartialEq)]
pub(super) struct Link {
    pub(super) target: Option<String>,
    pub(super) working_dir: Option<String>,
}

pub(super) const HAS_ID_LIST: usize = 0x1;
pub(super) const HAS_LINK_INFO: usize = 0x2;
pub(super) const HAS_WORKING_DIR: usize = 0x10;
pub(super) const IS_UNICODE: usize = 0x80;

pub(super) fn lnk_roots(data: &[u8]) -> Vec<PathBuf> {
    let Some(link) = read_lnk(data) else {
        return Vec::new();
    };
    let target = link.target.and_then(|t| root_of(Path::new(&t)));
    target
        .into_iter()
        .chain(link.working_dir.map(PathBuf::from))
        .collect()
}

pub(super) fn read_lnk(data: &[u8]) -> Option<Link> {
    if le32(data, 0)? != 0x4C {
        return None;
    }
    let flags = le32(data, 0x14)?;
    let mut at = 0x4C;
    let mut link = Link::default();
    if flags & HAS_ID_LIST != 0 {
        let size = le16(data, at)?;
        link.target = id_list_path(data.get(at + 2..at + 2 + size)?);
        at += 2 + size;
    }
    if flags & HAS_LINK_INFO != 0 {
        let size = le32(data, at)?;
        link.target = data.get(at..at + size).and_then(link_info_path).or(link.target);
        at += size;
    }
    let unit = if flags & IS_UNICODE != 0 { 2 } else { 1 };
    for bit in [0x4, 0x8, HAS_WORKING_DIR] {
        if flags & bit == 0 {
            continue;
        }
        let len = le16(data, at)? * unit;
        let raw = data.get(at + 2..at + 2 + len)?;
        if bit == HAS_WORKING_DIR {
            link.working_dir = Some(if unit == 2 {
                wide(raw, 0)?
            } else {
                String::from_utf8_lossy(raw).into_owned()
            });
        }
        at += 2 + len;
    }
    Some(link)
}

fn id_list_path(list: &[u8]) -> Option<String> {
    let mut path = String::new();
    let mut at = 0;
    while let Some(size) = le16(list, at)
        && size > 2
    {
        let item = list.get(at..at + size)?;
        match item[2] & 0x70 {
            0x20 => path = ansi(item, 3)?,
            0x30 => {
                let name = if item[2] & 0x04 != 0 {
                    wide(item, 14)?
                } else {
                    ansi(item, 14)?
                };
                if !path.ends_with('\\') {
                    path.push('\\');
                }
                path.push_str(&name);
            }
            _ => {}
        }
        at += size;
    }
    (path.as_bytes().get(1) == Some(&b':')).then_some(path)
}

fn link_info_path(info: &[u8]) -> Option<String> {
    if le32(info, 8)? & 0x1 == 0 {
        return None;
    }
    let (base, suffix) = if le32(info, 4)? >= 0x24 {
        (wide(info, le32(info, 28)?)?, wide(info, le32(info, 32)?))
    } else {
        (ansi(info, le32(info, 16)?)?, ansi(info, le32(info, 24)?))
    };
    Some(base + &suffix.unwrap_or_default())
}

fn le16(data: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn le32(data: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
}

fn ansi(data: &[u8], at: usize) -> Option<String> {
    let rest = data.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

fn wide(data: &[u8], at: usize) -> Option<String> {
    let units: Vec<u16> = data
        .get(at..)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}
