//! The SHA-256 of each small build source, kept between builds of a profile so an unchanged Play does not
//! read every source again. An entry is reused only while the file's size, modified time and change time
//! all match. A write to the file moves its change time (the inode's ctime on Linux, ChangeTime on Windows),
//! and copying tools never set that time back, even ones that restore the modified time, as `cp -p` does.
//! A file system that keeps no change time keeps no digests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::game_folder::baseline::hex;
use crate::{Result, io_err};

const VERSION: u32 = 1;

/// A file changed this recently may be written again within the same tick of the file system's clock,
/// leaving all three times as they were, so its digest is not kept.
const SETTLED: Duration = Duration::from_secs(2);

/// When a file was last written and when its inode last changed, in nanoseconds since 1970.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Times {
    pub modified: i128,
    pub changed: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    size: u64,
    times: Times,
    sha256: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    version: u32,
    entries: BTreeMap<String, Entry>,
}

/// The digests a build may reuse and the ones it computed, saved for the next build.
#[derive(Default)]
pub(super) struct Digests {
    at: Option<PathBuf>,
    kept: BTreeMap<String, Entry>,
    used: BTreeMap<String, Entry>,
    settled: Duration,
    #[cfg(test)]
    pub read: usize,
}

impl Digests {
    /// No digests kept: every source is read.
    #[cfg(test)]
    pub fn none() -> Self {
        Self::default()
    }

    /// The digests saved at `at`; none when the file is missing, unreadable or of another version.
    pub fn load(at: PathBuf) -> Self {
        let kept = std::fs::read(&at)
            .ok()
            .and_then(|data| serde_json::from_slice::<File>(&data).ok())
            .filter(|f| f.version == VERSION)
            .map(|f| f.entries)
            .unwrap_or_default();
        Self {
            at: Some(at),
            kept,
            settled: SETTLED,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub fn settled_after(mut self, settled: Duration) -> Self {
        self.settled = settled;
        self
    }

    /// The SHA-256 of the file at `path`, whose metadata is `meta`: the kept one when its size, modified
    /// time and change time are unchanged, else read from the file. A path that is not UTF-8 is always read.
    pub fn sha256(&mut self, path: &Path, meta: &std::fs::Metadata) -> Result<String> {
        let key = path.to_str().map(|p| key(p, cfg!(windows)));
        let times = key.as_ref().and_then(|_| times(path, meta));
        if let (Some(key), Some(t)) = (&key, times)
            && let Some(e) = (self.used.get(key))
                .or(self.kept.get(key))
                .filter(|e| e.size == meta.len() && e.times == t)
        {
            let sha = e.sha256.clone();
            self.used.insert(key.clone(), e.clone());
            return Ok(sha);
        }
        let data = std::fs::read(path).map_err(io_err(path))?;
        #[cfg(test)]
        {
            self.read += 1;
        }
        let sha = hex(&Sha256::digest(&data));
        if let (Some(key), Some(t)) = (key, times.filter(|t| settled(t, self.settled))) {
            let entry = Entry {
                size: data.len() as u64,
                times: t,
                sha256: sha.clone(),
            };
            if times_of(path).is_some_and(|after| after == t) {
                self.used.insert(key, entry);
            }
        }
        Ok(sha)
    }

    /// Saves the digests this build used, when they differ from the ones it loaded.
    pub fn save(&self) {
        let Some(at) = &self.at else { return };
        if self.used == self.kept {
            return;
        }
        let file = File {
            version: VERSION,
            entries: self.used.clone(),
        };
        if crate::write_json_shared(at, &file).is_err() {
            std::fs::remove_file(at).ok();
        }
    }
}

/// The key a file's digest is kept under, one for every spelling of its path: on Windows a path may use
/// either separator and any case, as an op's path joined onto a folder does beside the same file found by
/// walking that folder.
pub(super) fn key(path: &str, windows: bool) -> String {
    match windows {
        true => path.replace('/', "\\").to_lowercase(),
        false => path.to_owned(),
    }
}

fn settled(t: &Times, after: Duration) -> bool {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i128);
    now - t.changed.max(t.modified) >= after.as_nanos() as i128
}

fn times_of(path: &Path) -> Option<Times> {
    let meta = std::fs::metadata(path).ok()?;
    times(path, &meta)
}

#[cfg(unix)]
pub(crate) fn times(_: &Path, meta: &std::fs::Metadata) -> Option<Times> {
    use std::os::unix::fs::MetadataExt;
    let ns = |secs: i64, nanos: i64| secs as i128 * 1_000_000_000 + nanos as i128;
    Some(Times {
        modified: ns(meta.mtime(), meta.mtime_nsec()),
        changed: ns(meta.ctime(), meta.ctime_nsec()),
    })
}

#[cfg(windows)]
pub(crate) fn times(path: &Path, _: &std::fs::Metadata) -> Option<Times> {
    let info = win32::basic_info(path).filter(|i| i.change != 0)?;
    let ns = |filetime: i64| (filetime as i128 - win32::UNIX_EPOCH) * 100;
    Some(Times {
        modified: ns(info.last_write),
        changed: ns(info.change),
    })
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn times(_: &Path, _: &std::fs::Metadata) -> Option<Times> {
    None
}

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    /// 1970-01-01 in FILETIME units, 100 ns since 1601-01-01.
    pub const UNIX_EPOCH: i128 = 116_444_736_000_000_000;
    const FILE_BASIC_INFO: i32 = 0;

    #[repr(C)]
    #[derive(Default)]
    pub struct BasicInfo {
        pub creation: i64,
        pub last_access: i64,
        pub last_write: i64,
        pub change: i64,
        pub attributes: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(file: *mut c_void, class: i32, info: *mut c_void, size: u32) -> i32;
    }

    pub fn basic_info(path: &Path) -> Option<BasicInfo> {
        let file = std::fs::File::open(path).ok()?;
        let mut info = BasicInfo::default();
        // SAFETY: the handle is open for the call, and `info` is a FILE_BASIC_INFO of the size passed.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FILE_BASIC_INFO,
                (&mut info as *mut BasicInfo).cast(),
                std::mem::size_of::<BasicInfo>() as u32,
            )
        };
        (ok != 0).then_some(info)
    }
}
