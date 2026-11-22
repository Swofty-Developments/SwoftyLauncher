//! Reading the files a bundle sends: the tail of a log, both ends of one too long to send whole, and whether
//! two files are the same.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::{GAP_NOTE, LOADER_KEEP, WINDOW_KEEP, size};

/// The loader's line for the start of the window stage (native/src/loader/version.c stage_window).
const WINDOW_STAGE: &[u8] = b"] stage: window (";
const SEARCH_CHUNK: u64 = 1 << 20;

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
    Window { head: u64, window: u64, tail: u64 },
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
    let rest = cap.saturating_sub(head + GAP_NOTE);
    if let Some((at, window)) = window_stage(path, head, len, rest)? {
        let (tail, _) = read_tail(path, rest.saturating_sub(window.len() as u64 + GAP_NOTE))?;
        let end = at + window.len() as u64;
        data.extend_from_slice(gap_note(at - head, len).as_bytes());
        data.extend_from_slice(&window);
        data.extend_from_slice(gap_note(len.saturating_sub(end + tail.len() as u64), len).as_bytes());
        data.extend_from_slice(&tail);
        return Ok((
            data,
            Cut::Window {
                head,
                window: window.len() as u64,
                tail: tail.len() as u64,
            },
        ));
    }
    let (tail, _) = read_tail(path, rest)?;
    let left = len.saturating_sub(head + tail.len() as u64);
    data.extend_from_slice(gap_note(left, len).as_bytes());
    data.extend_from_slice(&tail);
    Ok((
        data,
        Cut::Middle {
            head,
            tail: tail.len() as u64,
        },
    ))
}

fn gap_note(left: u64, len: u64) -> String {
    format!(
        "[swofty: left out {left} bytes ({}) of {} here]\n",
        size(left),
        size(len)
    )
}

/// The window stage's lines when its first line starts past `from` and before the tail that `rest` bytes
/// would keep: where they start and up to WINDOW_KEEP of them, at most half of `rest`, ending at a line end.
fn window_stage(path: &Path, from: u64, len: u64, rest: u64) -> std::io::Result<Option<(u64, Vec<u8>)>> {
    let keep = WINDOW_KEEP.min(rest / 2);
    let end = len.saturating_sub(rest);
    if keep <= GAP_NOTE || end <= from {
        return Ok(None);
    }
    let Some(at) = find_line(path, from, end, WINDOW_STAGE)? else {
        return Ok(None);
    };
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(at))?;
    let mut window = Vec::with_capacity(keep as usize);
    f.take(keep).read_to_end(&mut window)?;
    match window.iter().rposition(|&b| b == b'\n') {
        Some(nl) => window.truncate(nl + 1),
        None => return Ok(None),
    }
    Ok(Some((at, window)))
}

/// Where the first line holding `needle` in `from..end` starts, at or after `from`.
pub(super) fn find_line(path: &Path, from: u64, end: u64, needle: &[u8]) -> std::io::Result<Option<u64>> {
    let mut f = std::fs::File::open(path)?;
    let mut pos = from;
    let mut buf = Vec::new();
    while pos < end {
        let want = SEARCH_CHUNK.min(end - pos) + needle.len() as u64 - 1;
        f.seek(SeekFrom::Start(pos))?;
        buf.clear();
        (&mut f).take(want).read_to_end(&mut buf)?;
        if let Some(i) = buf.windows(needle.len()).position(|w| w == needle) {
            let hit = pos + i as u64;
            let back = hit.saturating_sub(from).min(4096);
            let mut before = vec![0; back as usize];
            f.seek(SeekFrom::Start(hit - back))?;
            f.read_exact(&mut before)?;
            let start = before
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(hit - back, |nl| hit - back + nl as u64 + 1);
            return Ok(Some(start));
        }
        if buf.len() < needle.len() {
            break;
        }
        pos += SEARCH_CHUNK;
    }
    Ok(None)
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
