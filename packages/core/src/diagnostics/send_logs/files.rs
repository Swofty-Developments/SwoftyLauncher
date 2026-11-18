//! Reading the files a bundle sends: the tail of a log, both ends of one too long to send whole, and whether
//! two files are the same.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::{GAP_NOTE, LOADER_KEEP, size};

pub(super) fn read_tail(path: &Path, cap: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    if len <= cap {
        let mut data = Vec::with_capacity(len as usize);
        f.read_to_end(&mut data)?;
        return Ok((data, false));
    }
    f.seek(SeekFrom::Start(len - cap))?;
    let mut data = Vec::with_capacity(cap as usize);
    f.take(cap).read_to_end(&mut data)?;
    if let Some(nl) = data
        .iter()
        .position(|&b| b == b'\n')
        .filter(|&i| i < data.len() / 4)
    {
        data.drain(..=nl);
    }
    Ok((data, true))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Cut {
    None,
    Tail,
    Middle { head: u64, tail: u64 },
}

pub(super) fn kept_head(path: &Path, cap: u64) -> u64 {
    let name = file_name(path);
    if name.starts_with("loader") && name.ends_with(".log") && cap >= GAP_NOTE * 4 {
        LOADER_KEEP.min(cap / 2)
    } else {
        0
    }
}

pub(super) fn read_cut(path: &Path, cap: u64, head: u64) -> std::io::Result<(Vec<u8>, Cut)> {
    let len = std::fs::metadata(path)?.len();
    if len <= cap || head == 0 {
        let (data, cut) = read_tail(path, cap)?;
        return Ok((data, if cut { Cut::Tail } else { Cut::None }));
    }
    let mut data = read_head(path, head)?;
    if let Some(nl) = data.iter().rposition(|&b| b == b'\n') {
        data.truncate(nl + 1);
    } else {
        data.push(b'\n');
    }
    let head = data.len() as u64;
    let (tail, _) = read_tail(path, cap.saturating_sub(head + GAP_NOTE))?;
    let left = len.saturating_sub(head + tail.len() as u64);
    data.extend_from_slice(
        format!(
            "[swofty: left out {left} bytes ({}) of {} here]\n",
            size(left),
            size(len)
        )
        .as_bytes(),
    );
    data.extend_from_slice(&tail);
    Ok((
        data,
        Cut::Middle {
            head,
            tail: tail.len() as u64,
        },
    ))
}

pub(super) fn read_head(path: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.take(cap).read_to_end(&mut data)?;
    Ok(data)
}

pub(super) fn read_ends(path: &Path, head: u64, tail: u64) -> std::io::Result<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len <= head + tail {
        return read_head(path, head + tail);
    }
    let mut data = read_head(path, head)?;
    data.push(b'\n');
    data.extend(read_tail(path, tail)?.0);
    Ok(data)
}

pub(super) fn same_contents(a: &Path, b: &Path) -> bool {
    let len = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    len(a).is_some_and(|n| len(b) == Some(n))
        && matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

pub(super) fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}
