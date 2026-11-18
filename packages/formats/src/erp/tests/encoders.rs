use super::*;

use crate::erp::encoders::{ENCODERS, encode};

#[test]
fn a_reused_encoder_writes_what_a_new_one_does() {
    let mut seed = 7u64;
    let mut noisy = |n: usize| -> Vec<u8> {
        (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if i % 3 == 0 {
                    (seed >> 59) as u8
                } else {
                    b"track_asset_"[i % 12]
                }
            })
            .collect()
    };
    let inputs = [
        Vec::new(),
        noisy(1),
        noisy(4 << 10),
        noisy(70 << 10),
        noisy(3 << 20),
        noisy(9 << 20),
    ];
    for level in [3, ZSTD_LEVEL] {
        for _ in 0..2 {
            for data in &inputs {
                assert_eq!(
                    encode(data, level).unwrap(),
                    streamed(data, level),
                    "{} bytes, level {level}",
                    data.len()
                );
            }
        }
    }
    ENCODERS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    assert_eq!(
        encode(&inputs[2], ZSTD_LEVEL).unwrap(),
        streamed(&inputs[2], ZSTD_LEVEL)
    );
}

/// The content size and window a frame header declares (RFC 8878 3.1.1.1).
fn frame_header(raw: &[u8]) -> (Option<u64>, u64) {
    assert_eq!(raw[..4], ZSTD_MAGIC);
    let fhd = raw[4];
    let single = fhd & 0x20 != 0;
    let mut at = 5;
    let mut window = 0;
    if !single {
        let (exponent, mantissa) = (u64::from(raw[at] >> 3), u64::from(raw[at] & 7));
        let base = 1u64 << (10 + exponent);
        window = base + base / 8 * mantissa;
        at += 1;
    }
    at += [0, 1, 2, 4][usize::from(fhd & 3)];
    let size = match (fhd >> 6, single) {
        (0, false) => None,
        (0, true) => Some(u64::from(raw[at])),
        (1, _) => Some(u64::from(u16::from_le_bytes([raw[at], raw[at + 1]])) + 256),
        (2, _) => Some(u64::from(u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()))),
        _ => Some(u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())),
    };
    (size, if single { size.unwrap() } else { window })
}

/// What zstd 1.5.6+'s stream decoder allocates for a frame: its input
/// block buffer and its window ring, which a known content size caps.
fn decoder_buffer(size: Option<u64>, window: u64) -> u64 {
    let block = window.min(128 << 10);
    let ring = window + 2 * block + 64;
    size.map_or(ring, |s| s.min(ring)) + block.max(4)
}

#[test]
fn every_frame_carries_its_content_size_and_at_most_a_1_mib_window() {
    let noisy = |n: usize| -> Vec<u8> {
        let mut seed = n as u64 | 1;
        (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if i % 4 == 0 {
                    (seed >> 56) as u8
                } else {
                    b"livery_"[i % 7]
                }
            })
            .collect()
    };
    for level in [3, ZSTD_LEVEL] {
        for n in [0, 1, 300, 70 << 10, 1 << 20, (1 << 20) + 1, 3 << 20, 9 << 20] {
            let data = noisy(n);
            let raw = compress_at(&data, level).unwrap();
            let (size, window) = frame_header(&raw);
            assert_eq!(size, Some(n as u64), "{n} bytes, level {level}");
            assert_eq!(
                zstd::zstd_safe::get_frame_content_size(&raw).ok(),
                Some(Some(n as u64))
            );
            assert!(
                window <= 1 << ZSTD_WINDOW_LOG,
                "{n} bytes, level {level}: window {window}"
            );
            assert!(
                decoder_buffer(size, window) <= (1 << 20) + 393_280,
                "{n} bytes, level {level}: {}",
                decoder_buffer(size, window)
            );
            assert_eq!(zstd::decode_all(raw.as_slice()).unwrap(), data);
        }
    }
    let (size, window) = frame_header(&compress(&noisy(9 << 20)).unwrap());
    assert_eq!(
        (size, window, decoder_buffer(size, window)),
        (Some(9 << 20), 1 << 20, 1_441_856)
    );
}
