use std::path::Path;

use md5::{Digest, Md5};

use crate::Result;
use crate::erp::ZSTD_WINDOW_LOG;
use crate::erp::encoders::encode;

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

static STATS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// What the frame cache served and what it compressed since the process started.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub hits: u64,
    pub hit_bytes: u64,
    pub misses: u64,
    pub miss_bytes: u64,
    pub miss_packed: u64,
    pub miss_nanos: u64,
}

pub fn frame_stats() -> FrameStats {
    let g = |i: usize| STATS[i].load(Relaxed);
    FrameStats {
        hits: g(0),
        hit_bytes: g(1),
        misses: g(2),
        miss_bytes: g(3),
        miss_packed: g(4),
        miss_nanos: g(5),
    }
}

pub(super) fn compress_cached(dir: &Path, data: &[u8], level: i32) -> Result<Vec<u8>> {
    let digest: String = Md5::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    let name = format!(
        "{digest}-{}-{level}-w{ZSTD_WINDOW_LOG}-{}",
        data.len(),
        zstd::zstd_safe::version_number()
    );
    let path = dir.join(&name[..2]).join(&name);
    if let Ok(raw) = std::fs::read(&path)
        && zstd::decode_all(raw.as_slice()).is_ok_and(|back| back == data)
    {
        if let Ok(f) = std::fs::File::options().write(true).open(&path) {
            f.set_modified(std::time::SystemTime::now()).ok();
        }
        STATS[0].fetch_add(1, Relaxed);
        STATS[1].fetch_add(data.len() as u64, Relaxed);
        return Ok(raw);
    }
    let started = std::time::Instant::now();
    let raw = encode(data, level)?;
    STATS[2].fetch_add(1, Relaxed);
    STATS[3].fetch_add(data.len() as u64, Relaxed);
    STATS[4].fetch_add(raw.len() as u64, Relaxed);
    STATS[5].fetch_add(started.elapsed().as_nanos() as u64, Relaxed);
    static TMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = TMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    let stored = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&tmp, &raw))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if stored.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    Ok(raw)
}
