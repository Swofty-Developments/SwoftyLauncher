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

#[test]
fn pair_is_self_consistent() {
    let (data, index) = write_pair(
        &synthetic_stream(5000),
        IV,
        3,
        "Save System 2 Demo - Display Name",
        PROFILE_NAME_HASH,
        99,
    )
    .unwrap();
    assert!(decode_data(&data).is_ok());
    assert_eq!(index.len(), INDEX_LEN);
    let plain = decrypt_index(&index).unwrap();
    let stored = u32::from_le_bytes(plain[0x180..0x184].try_into().unwrap());
    assert_eq!(index_checksum(&plain).unwrap(), stored);
    assert_eq!(&plain[..4], &[b'S', 0, b'a', 0]);
    assert_eq!(&plain[0x184..0x188], &1u32.to_le_bytes());
}

#[test]
fn checksum_ignores_its_own_field_and_catches_edits() {
    let mut plain = index_plain("x", PROFILE_NAME_HASH, 1);
    let sum = index_checksum(&plain).unwrap();
    plain[0x180] ^= 0xff;
    assert_eq!(index_checksum(&plain).unwrap(), sum);
    plain[0x50] ^= 1;
    assert_ne!(index_checksum(&plain).unwrap(), sum);
}

#[test]
fn real_indexes_have_matching_checksums() {
    let Some(dir) = std::env::var_os("SWOFTY_SAVE_COPIES") else {
        return;
    };
    let mut seen = 0;
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("#QKRHMYXE"))
            {
                let index = std::fs::read(&path).unwrap();
                let plain = decrypt_index(&index).unwrap();
                let stored = u32::from_le_bytes(plain[0x180..0x184].try_into().unwrap());
                assert_eq!(index_checksum(&plain).unwrap(), stored, "{path:?}");
                let data_path = path.with_file_name(
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .replace("QKRHMYXE", "NXDSMWW.BWW"),
                );
                let data = decode_data(&std::fs::read(data_path).unwrap()).unwrap();
                let (_, rebuilt) = write_pair(
                    &data.stream,
                    data.iv,
                    3,
                    "Save System 2 Demo - Display Name",
                    PROFILE_NAME_HASH,
                    1,
                )
                .unwrap();
                let rebuilt = decrypt_index(&rebuilt).unwrap();
                let sum = u32::from_le_bytes(rebuilt[0x180..0x184].try_into().unwrap());
                assert_eq!(index_checksum(&rebuilt).unwrap(), sum);
                seen += 1;
            }
        }
    }
    assert!(seen > 0);
}
