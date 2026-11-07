//! Copying a data folder: resumable and cancellable, with a size and content
//! check of what was written.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

const CHUNK: usize = 1 << 20;
const HASHED_UP_TO: u64 = 1 << 20;
const CLOCK_SLACK: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq)]
pub enum Entry {
    Dir,
    File { len: u64, modified: Option<SystemTime> },
    Link(PathBuf),
}

pub type Listing = BTreeMap<PathBuf, Entry>;

/// What a copy or sync stopped on.
#[derive(Debug)]
pub enum Stop {
    Cancelled,
    Io(String),
}

impl From<io::Error> for Stop {
    fn from(e: io::Error) -> Self {
        Stop::Io(e.to_string())
    }
}

pub fn listing(root: &Path) -> io::Result<Listing> {
    fn walk(path: &Path, rel: PathBuf, out: &mut Listing) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        let entry = if meta.file_type().is_symlink() {
            Entry::Link(std::fs::read_link(path)?)
        } else if meta.is_dir() {
            for child in std::fs::read_dir(path)? {
                let child = child?;
                walk(&child.path(), rel.join(child.file_name()), out)?;
            }
            Entry::Dir
        } else {
            Entry::File {
                len: meta.len(),
                modified: meta.modified().ok(),
            }
        };
        out.insert(rel, entry);
        Ok(())
    }
    let mut out = Listing::new();
    walk(root, PathBuf::new(), &mut out)?;
    Ok(out)
}

pub fn totals(list: &Listing) -> (u64, usize) {
    list.values().fold((0, 0), |(bytes, files), e| match e {
        Entry::File { len, .. } => (bytes + len, files + 1),
        _ => (bytes, files),
    })
}

fn same_stamp(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a
            .duration_since(b)
            .or_else(|_| b.duration_since(a))
            .is_ok_and(|d| d <= CLOCK_SLACK),
        _ => false,
    }
}

/// Makes `dst` match `src`: copies the files that are missing or differ in
/// size or time, and removes what `src` no longer has. Safe to run again after
/// it stopped half way. `seen` gets each file's path and bytes as they are
/// written.
pub fn sync(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    seen: &mut dyn FnMut(&Path, u64),
) -> Result<(), Stop> {
    let from = listing(src)?;
    std::fs::create_dir_all(dst)?;
    let have = listing(dst)?;
    for (rel, entry) in have.iter().rev() {
        let keep = match (from.get(rel), entry) {
            (Some(Entry::Dir), Entry::Dir) => true,
            (Some(Entry::File { .. }), Entry::File { .. }) => true,
            (Some(Entry::Link(a)), Entry::Link(b)) => a == b,
            _ => false,
        };
        if !keep && !rel.as_os_str().is_empty() {
            remove(&dst.join(rel))?;
        }
    }
    for (rel, entry) in &from {
        if cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let (from, to) = (src.join(rel), dst.join(rel));
        match entry {
            Entry::Dir => std::fs::create_dir_all(&to)?,
            Entry::Link(_) => {
                if to.symlink_metadata().is_err() {
                    copy_link(&from, &to)?;
                }
            }
            Entry::File { len, modified } => {
                let current = matches!(
                    have.get(rel),
                    Some(Entry::File { len: l, modified: m }) if l == len && same_stamp(*m, *modified)
                );
                if !current {
                    copy_file(&from, &to, cancel, &mut |n| seen(rel, n))?;
                }
            }
        }
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path, cancel: &AtomicBool, wrote: &mut dyn FnMut(u64)) -> Result<(), Stop> {
    let mut input = File::open(src)?;
    let modified = input.metadata()?.modified().ok();
    let mut output = File::create(dst)?;
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n])?;
        wrote(n as u64);
    }
    output.flush()?;
    if let Some(modified) = modified {
        output.set_modified(modified)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn copy_link(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)
}

#[cfg(windows)]
fn copy_link(src: &Path, dst: &Path) -> io::Result<()> {
    let target = std::fs::read_link(src)?;
    if std::fs::metadata(src).is_ok_and(|m| m.is_dir()) {
        super::link::create(&target, dst)
    } else {
        std::os::windows::fs::symlink_file(&target, dst)
    }
}

/// Checks that `dst` holds what `src` holds: the same entries and sizes, and
/// the same bytes in every file up to a megabyte.
pub fn verify(src: &Path, dst: &Path) -> Result<(), String> {
    let (a, b) = (
        listing(src).map_err(|e| format!("could not read {}: {e}", src.display()))?,
        listing(dst).map_err(|e| format!("could not read {}: {e}", dst.display()))?,
    );
    if a.len() != b.len() {
        return Err(format!("{} entries were copied of {}", b.len(), a.len()));
    }
    for (rel, entry) in &a {
        match (entry, b.get(rel)) {
            (Entry::Dir, Some(Entry::Dir)) => {}
            (Entry::Link(x), Some(Entry::Link(y))) if x == y => {}
            (Entry::File { len: x, .. }, Some(Entry::File { len: y, .. })) if x == y => {
                if *x <= HASHED_UP_TO && digest(&src.join(rel)) != digest(&dst.join(rel)) {
                    return Err(format!("{} differs after the copy", rel.display()));
                }
            }
            _ => return Err(format!("{} does not match after the copy", rel.display())),
        }
    }
    Ok(())
}

fn digest(path: &Path) -> Option<Vec<u8>> {
    Some(Sha256::digest(std::fs::read(path).ok()?).to_vec())
}

/// Deletes a file, link or folder; a link is unlinked, never followed.
pub fn remove(path: &Path) -> io::Result<()> {
    let meta = path.symlink_metadata()?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        super::link::remove(path).or_else(|_| crate::remove_placed(path))
    }
}

/// Deletes what is inside `dir` and keeps `dir`.
pub fn empty(dir: &Path) -> io::Result<()> {
    for child in std::fs::read_dir(dir)? {
        remove(&child?.path())?;
    }
    Ok(())
}
