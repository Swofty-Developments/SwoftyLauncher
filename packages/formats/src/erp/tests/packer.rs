use super::*;

#[test]
fn compressing_many_at_once_writes_what_compressing_each_in_turn_does() {
    let mut seed = 11u64;
    let mut noisy = |n: usize| -> Vec<u8> {
        (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if i % 4 == 0 {
                    (seed >> 59) as u8
                } else {
                    b"surface_array_"[i % 14]
                }
            })
            .collect()
    };
    let inputs: Vec<Vec<u8>> = [0, 1, 300, 20 << 10, 70 << 10, 2 << 20, 5 << 20, 9, 40 << 10]
        .into_iter()
        .map(&mut noisy)
        .collect();
    let refs: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    for level in [3, ZSTD_LEVEL] {
        let many = compress_many(&refs, level).unwrap();
        assert_eq!(many.len(), inputs.len());
        for (data, packed) in inputs.iter().zip(&many) {
            assert_eq!(
                *packed,
                streamed(data, level),
                "{} bytes, level {level}",
                data.len()
            );
            assert_eq!(zstd::decode_all(packed.as_slice()).unwrap(), *data);
        }
    }
    assert!(compress_many(&[], ZSTD_LEVEL).unwrap().is_empty());
}

#[test]
fn a_settled_fragment_is_the_bytes_an_eager_packer_writes_at_its_level() {
    let noisy = |n: usize, salt: u64| -> Vec<u8> {
        let mut seed = salt;
        (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                if i % 3 == 0 {
                    (seed >> 59) as u8
                } else {
                    b"surface_"[i % 8]
                }
            })
            .collect()
    };
    let inputs: Vec<(Vec<u8>, i32)> = [
        (0, 3),
        (300, ZSTD_LEVEL),
        (70 << 10, 3),
        (1 << 20, ZSTD_LEVEL),
        (9, 3),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (n, level))| (noisy(n, i as u64 + 1), level))
    .collect();
    let dir = std::env::temp_dir().join(format!("swofty-settle-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    for packer in [Packer::default(), Packer::cached(&dir)] {
        let deferring = packer.clone().deferring(true);
        let mut frags: Vec<Fragment> = inputs
            .iter()
            .map(|(data, level)| {
                let mut f = Fragment::new(*b"temp", 0, 0, ZSTD, Vec::new());
                deferring.put(&mut f, data, true, *level).unwrap();
                assert_eq!(
                    (f.raw(), f.packed(), f.pending()),
                    (None, None, Some((&data[..], *level)))
                );
                assert_eq!((f.data().unwrap(), f.size), (data.clone(), data.len() as u64));
                f
            })
            .collect();
        assert_eq!(packer.settle(&mut frags).unwrap(), inputs.len());
        for (f, (data, level)) in frags.iter().zip(&inputs) {
            let eager = compress_at(data, *level).unwrap();
            assert_eq!(f.raw(), Some(&eager[..]), "{} bytes, level {level}", data.len());
            assert_eq!((f.packed(), f.pending()), (Some(eager.len() as u64), None));
            assert_eq!(f.data().unwrap(), *data);
        }
        assert_eq!(packer.settle(&mut frags).unwrap(), 0);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_archive_with_pending_fragments_writes_the_bytes_an_eager_one_does() {
    let eager = sample().to_bytes().unwrap();
    let mut pending = sample();
    let res = &mut pending.resources[0];
    res.set_fragment_with(&Packer::default().deferring(true), 0, b"!pkg sample payload")
        .unwrap();
    assert_eq!(
        res.fragments[0].pending(),
        Some((&b"!pkg sample payload"[..], ZSTD_LEVEL))
    );
    assert_eq!(res.hash, sample().resources[0].hash);
    assert_eq!(pending.clone().to_bytes().unwrap(), eager);

    let dir = std::env::temp_dir().join(format!("swofty-erp-pending-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("src.erp"), &eager).unwrap();
    let file = ErpFile::open(&dir.join("src.erp")).unwrap();
    file.write(
        vec![Entry::Owned(pending.resources[0].clone())],
        &dir.join("out.erp"),
    )
    .unwrap();
    assert_eq!(std::fs::read(dir.join("out.erp")).unwrap(), eager);
    assert!(
        file.write(vec![Entry::Copy(&pending.resources[0])], &dir.join("copy.erp"))
            .is_err()
    );
    std::fs::remove_dir_all(&dir).ok();
}
