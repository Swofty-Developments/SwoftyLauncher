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

/// Whether the archive index `index` lists a resource named `name`, matched as an index spells it.
pub fn index_holds(index: &[u8], name: &str) -> bool {
    let mut needle = ((name.len() + 1) as u16).to_le_bytes().to_vec();
    needle.extend_from_slice(name.as_bytes());
    needle.push(0);
    index
        .windows(needle.len())
        .any(|w| w[0] == needle[0] && w == needle.as_slice())
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
