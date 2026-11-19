use std::path::Path;

use swofty_formats::erp::ErpFile;

use super::{ASSET_GROUPS, fail};
use crate::Result;
use crate::addons::builtin::loader_files;

pub fn served_at_runtime(rel: &str) -> bool {
    rel.starts_with(ASSET_GROUPS)
}

/// The raw index of the archive at `path`, without reading its resources.
pub fn index_of(path: &Path) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 0x30];
    f.read_exact(&mut head).ok()?;
    if &head[..4] != b"ERPK" {
        return None;
    }
    let at = u64::from_le_bytes(head[0x10..0x18].try_into().ok()?);
    let len = u64::from_le_bytes(head[0x18..0x20].try_into().ok()?);
    if len > 1 << 28 {
        return None;
    }
    let mut index = vec![0; len as usize];
    f.seek(SeekFrom::Start(at)).ok()?;
    f.read_exact(&mut index).ok()?;
    Some(index)
}

/// Every archive under `dir`, as `rel/<path>`.
pub fn erp_files(dir: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        let child = format!("{rel}/{name}");
        if path.is_dir() {
            erp_files(&path, &child, out);
        } else if name.to_ascii_lowercase().ends_with(".erp") {
            out.push(child);
        }
    }
}

fn index_needle(name: &str) -> Vec<u8> {
    let mut needle = ((name.len() + 1) as u16).to_le_bytes().to_vec();
    needle.extend_from_slice(name.as_bytes());
    needle.push(0);
    needle
}

/// A resource name as an archive index spells it, ready to look for in any number of indexes.
pub struct IndexName(memchr::memmem::Finder<'static>);

impl IndexName {
    pub fn new(name: &str) -> Self {
        Self(memchr::memmem::Finder::new(&index_needle(name)).into_owned())
    }

    /// Whether the archive index `index` lists the name, matched as an index spells it.
    pub fn in_index(&self, index: &[u8]) -> bool {
        self.0.find(index).is_some()
    }
}

/// Whether the archive index `index` lists a resource named `name`, matched as an index spells it.
pub fn index_holds(index: &[u8], name: &str) -> bool {
    IndexName::new(name).in_index(index)
}

/// The asset group folder of a resource name, such as `2025_asset_groups/ui_package` for
/// `eaid://ui_package/...`.
pub fn group_dir(name: &str) -> Option<String> {
    let group = name.strip_prefix("eaid://")?.split('/').next()?;
    (!group.is_empty()).then(|| format!("{ASSET_GROUPS}/{group}"))
}

pub fn mipmap_path(target: &str, rel: &str) -> String {
    format!("{}/{rel}", target.trim_end_matches('/'))
}

pub fn mipmap_names(from: &Path) -> Vec<String> {
    if !from.is_dir() {
        return Vec::new();
    }
    loader_files(from)
        .map(|files| files.into_keys().collect())
        .unwrap_or_default()
}

pub fn overlay_names(path: &Path) -> Result<Vec<String>> {
    let file = ErpFile::open(path).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    Ok(file.erp.resources.iter().map(|r| r.name.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_scan(index: &[u8], name: &str) -> bool {
        let needle = index_needle(name);
        index
            .windows(needle.len())
            .any(|w| w[0] == needle[0] && w == needle.as_slice())
    }

    #[test]
    fn a_name_is_found_in_an_index_exactly_where_a_scan_of_every_window_finds_it() {
        let mut index = vec![0u8; 40];
        for name in ["eaid://a/b", "eaid://a/bc", "x"] {
            index.extend(index_needle(name));
            index.extend([7, 0, 1]);
        }
        for name in ["eaid://a/b", "eaid://a/bc", "x", "eaid://a/", "y", ""] {
            assert_eq!(index_holds(&index, name), windows_scan(&index, name), "{name}");
        }
        assert!(index_holds(&index, "eaid://a/b"));
        assert!(!index_holds(&index, "eaid://a/"));
    }
}
