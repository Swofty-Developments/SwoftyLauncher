use super::*;

use crate::erp::encoders::encode;
use crate::erp::frame_cache::compress_cached;
use crate::erp::packer::PACKED_MIN;

#[test]
fn a_cached_compression_matches_a_fresh_one_and_a_damaged_entry_is_redone() {
    let dir = std::env::temp_dir().join(format!("swofty-packed-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let data: Vec<u8> = (0..PACKED_MIN as u32 * 2).map(|i| (i % 251) as u8).collect();
    let fresh = encode(&data, ZSTD_LEVEL).unwrap();
    assert_eq!(compress_cached(&dir, &data, ZSTD_LEVEL).unwrap(), fresh);
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten())
        .map(|e| e.path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(std::fs::read(&entries[0]).unwrap(), fresh);
    assert_eq!(compress_cached(&dir, &data, ZSTD_LEVEL).unwrap(), fresh);
    std::fs::write(&entries[0], &fresh[..fresh.len() / 2]).unwrap();
    assert_eq!(compress_cached(&dir, &data, ZSTD_LEVEL).unwrap(), fresh);
    assert_eq!(std::fs::read(&entries[0]).unwrap(), fresh);
    std::fs::remove_dir_all(&dir).ok();
}
