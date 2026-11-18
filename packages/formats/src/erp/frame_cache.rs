use std::path::Path;

use md5::{Digest, Md5};

use crate::Result;
use crate::erp::ZSTD_WINDOW_LOG;
use crate::erp::encoders::encode;

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
        return Ok(raw);
    }
    let raw = encode(data, level)?;
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
