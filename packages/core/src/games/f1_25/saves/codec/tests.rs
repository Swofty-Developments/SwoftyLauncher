use super::*;

fn synthetic_stream(len: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(len);
    stream.extend_from_slice(&1u64.to_le_bytes());
    stream.extend_from_slice(&1u64.to_le_bytes());
    let mut x = 12345u32;
    while stream.len() < len {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
        stream.push((x >> 16) as u8 % 7);
    }
    stream
}

const IV: [u8; IV_LEN] = [7; IV_LEN];

#[test]
fn data_round_trips() {
    for len in [16, 100, 4096, 70_001] {
        let stream = synthetic_stream(len);
        let file = encode_data(&stream, IV, 3).unwrap();
        assert_eq!(&file[..4], &MAGIC);
        assert_eq!((file.len() - HEADER_LEN) % 16, 0);
        let decoded = decode_data(&file).unwrap();
        assert_eq!(decoded.stream, stream);
        assert_eq!(decoded.iv, IV);
        assert_eq!(seal_frame(&decoded.frame, decoded.iv), file);
    }
}

#[test]
fn tampering_is_rejected() {
    let file = encode_data(&synthetic_stream(2000), IV, 3).unwrap();
    let mut body = file.clone();
    let last = body.len() - 1;
    body[last] ^= 1;
    assert!(matches!(decode_data(&body), Err(CodecError::BadMac)));
    let mut magic = file.clone();
    magic[0] ^= 1;
    assert!(matches!(decode_data(&magic), Err(CodecError::BadMagic)));
    let mut size = file.clone();
    size[5] ^= 1;
    assert!(matches!(decode_data(&size), Err(CodecError::BadSize { .. })));
    assert!(matches!(decode_data(&file[..20]), Err(CodecError::Short)));
}

#[test]
fn iv_changes_the_whole_file() {
    let stream = synthetic_stream(3000);
    let a = encode_data(&stream, [1; IV_LEN], 3).unwrap();
    let b = encode_data(&stream, [2; IV_LEN], 3).unwrap();
    assert_eq!(a.len(), b.len());
    assert_ne!(a[HEADER_LEN..], b[HEADER_LEN..]);
}

#[test]
fn index_round_trips() {
    let plain: Vec<u8> = (0..INDEX_LEN).map(|i| (i * 7) as u8).collect();
    let sealed = encrypt_index(&plain).unwrap();
    assert_eq!(sealed.len(), INDEX_LEN);
    assert_ne!(sealed, plain);
    assert_eq!(decrypt_index(&sealed).unwrap(), plain);
    assert!(decrypt_index(&plain[..17]).is_err());
}

#[test]
fn index_is_deterministic_per_block() {
    let block = [9u8; 16];
    let plain: Vec<u8> = block.iter().chain(block.iter()).copied().collect();
    let sealed = encrypt_index(&plain).unwrap();
    assert_eq!(sealed[..16], sealed[16..]);
}

#[test]
fn mt_matches_the_reference_sequence() {
    let mut mt = mt::Mt19937::new(0x1234_5678);
    let first: Vec<u32> = (0..3).map(|_| mt.next_u32()).collect();
    let mut again = mt::Mt19937::new(0x1234_5678);
    assert_eq!(first, (0..3).map(|_| again.next_u32()).collect::<Vec<_>>());
    assert!(first.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn real_sample_decodes_and_reseals_identically() {
    let Some(path) = std::env::var_os("SWOFTY_SAVE_SAMPLE") else {
        return;
    };
    let file = std::fs::read(path).unwrap();
    let decoded = decode_data(&file).unwrap();
    assert_eq!(decoded.stream[..8], 1u64.to_le_bytes());
    assert_eq!(seal_frame(&decoded.frame, decoded.iv), file);
}
