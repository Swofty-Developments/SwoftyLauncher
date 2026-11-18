use super::*;

use super::file::{retrying, transient};
use super::packer::{ENCODERS, PACKED_MIN, compress_cached, encode};
use crate::Error;

#[test]
fn a_cut_short_archive_names_its_file() {
    let dir = std::env::temp_dir().join(format!("swofty-erp-short-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut head = vec![0; HEADER_LEN];
    head[..4].copy_from_slice(MAGIC);
    head[0x20..0x28].copy_from_slice(&(HEADER_LEN as u64 + 100).to_le_bytes());
    for (name, body) in [("index.erp", head.clone()), ("header.erp", head[..10].to_vec())] {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        let err = ErpFile::open(&path).unwrap_err();
        assert!(
            matches!(&err, Error::IoAt { path: p, source } if p == &path && source.kind() == std::io::ErrorKind::UnexpectedEof),
            "{err}"
        );
        assert_eq!(
            err.to_string(),
            format!(
                "{}: the file is shorter than it should be, so it is incomplete or damaged",
                path.display()
            )
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_bad_data_offset_or_count_is_an_error_not_a_panic() {
    let dir = std::env::temp_dir().join(format!("swofty-erp-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut head = vec![0; HEADER_LEN];
    head[..4].copy_from_slice(MAGIC);
    let path = dir.join("zero.erp");
    std::fs::write(&path, &head).unwrap();
    assert!(matches!(ErpFile::open(&path), Err(Error::Format(_))));

    head[0x20..0x28].copy_from_slice(&u64::MAX.to_le_bytes());
    let path = dir.join("huge.erp");
    std::fs::write(&path, &head).unwrap();
    let err = ErpFile::open(&path).unwrap_err();
    assert!(
        matches!(&err, Error::IoAt { path: p, source } if p == &path && source.kind() == std::io::ErrorKind::UnexpectedEof),
        "{err}"
    );

    let mut counted = head.clone();
    counted[0x20..0x28].copy_from_slice(&(HEADER_LEN as u64 + 8).to_le_bytes());
    counted.extend_from_slice(&u32::MAX.to_le_bytes());
    counted.extend_from_slice(&0u32.to_le_bytes());
    let path = dir.join("count.erp");
    std::fs::write(&path, &counted).unwrap();
    assert!(matches!(ErpFile::open(&path), Err(Error::Eof(_))));
    assert!(matches!(Erp::parse(&counted), Err(Error::Eof(_))));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_fragment_past_the_end_of_its_file_is_an_error_not_a_panic() {
    let path = std::env::temp_dir().join(format!("swofty-erp-packed-{}.erp", std::process::id()));
    std::fs::write(&path, sample().to_bytes().unwrap()).unwrap();
    let file = ErpFile::open(&path).unwrap();
    let mut res = file.erp.resources[0].clone();
    res.fragments[0].payload = Payload::Stored {
        packed: u64::MAX,
        raw: Vec::new(),
    };
    let err = file.load(&res).unwrap_err();
    assert!(
        matches!(&err, Error::IoAt { source, .. } if source.kind() == std::io::ErrorKind::UnexpectedEof),
        "{err}"
    );
    res.fragments[0].payload = Payload::Stored {
        packed: 1 << 40,
        raw: Vec::new(),
    };
    assert!(file.load(&res).is_err());
    assert!(file.load(&file.erp.resources[0]).is_ok());
    std::fs::remove_file(&path).ok();
}

fn streamed(data: &[u8], level: i32) -> Vec<u8> {
    let mut enc = zstd::Encoder::new(Vec::new(), level).unwrap();
    enc.include_checksum(true).unwrap();
    enc.include_contentsize(true).unwrap();
    enc.window_log(ZSTD_WINDOW_LOG).unwrap();
    enc.set_pledged_src_size(Some(data.len() as u64)).unwrap();
    enc.multithread(1).unwrap();
    std::io::Write::write_all(&mut enc, data).unwrap();
    enc.finish().unwrap()
}

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

fn sample() -> Erp {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&4u32.to_le_bytes());
    let text = b"!pkg sample payload".to_vec();
    let packed = compress(&text).unwrap();
    let mut kind = [0u8; 16];
    kind[..2].copy_from_slice(b"UI");
    let mut res = Resource {
        name: "eaid://ui_package/screens/sample.ui".into(),
        kind,
        unk: vec![0; 6],
        fragments: vec![
            Fragment::new(*b"temp", text.len() as u64, 0, ZSTD, packed),
            Fragment::new(*b"syst", 3, 0, 0, b"abc".to_vec()),
        ],
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    Erp {
        header,
        version: 4,
        resources: vec![res],
    }
}

fn stored_archive() -> Erp {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&4u32.to_le_bytes());
    let resources = (0..3u8)
        .map(|i| {
            let mut kind = [0u8; 16];
            kind[..3].copy_from_slice(b"TEX");
            let body = vec![i ^ 0x5a; 64 * (i as usize + 1)];
            let mut res = Resource {
                name: format!("eaid://ui_package/art/r{i}.tif.image"),
                kind,
                unk: vec![i; 6],
                fragments: vec![
                    Fragment::new(*b"temp", 40, 0, 0, vec![i; 40]),
                    Fragment::new(*b"data", body.len() as u64, 0, 0, body),
                ],
                hash: [0; 16],
            };
            res.hash = res.content_hash().unwrap();
            res
        })
        .collect();
    Erp {
        header,
        version: 4,
        resources,
    }
}

fn pin(bytes: &[u8]) -> (usize, u64) {
    (bytes.len(), crate::hash::fnv1a64(bytes))
}

#[test]
fn a_stored_archive_and_its_edit_write_the_pinned_bytes() {
    let bytes = stored_archive().to_bytes().unwrap();
    assert_eq!(pin(&bytes), (998, 0x3aebb140d8c4a597));
    let mut back = Erp::parse(&bytes).unwrap();
    assert_eq!(back.to_bytes().unwrap(), bytes);
    let res = back.find_mut("eaid://ui_package/art/r1.tif.image").unwrap();
    res.set_fragment(1, &[7; 200]).unwrap();
    assert_eq!(res.hash, res.content_hash().unwrap());
    assert_eq!(pin(&back.to_bytes().unwrap()), (1070, 0xee051df975c21b8f));
}

#[test]
fn write_parse_write_is_identical() {
    let bytes = sample().to_bytes().unwrap();
    let mut back = Erp::parse(&bytes).unwrap();
    assert_eq!(back.to_bytes().unwrap(), bytes);
    let res = back.find("eaid://ui_package/screens/sample.ui").unwrap();
    assert_eq!(res.fragments[0].data().unwrap(), b"!pkg sample payload");
    assert_eq!(res.kind(), "UI");
}

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

#[test]
fn set_fragment_updates_hash() {
    let mut erp = sample();
    let res = erp.find_mut("eaid://ui_package/screens/sample.ui").unwrap();
    res.set_fragment(0, b"changed").unwrap();
    let hash = res.hash;
    let bytes = erp.to_bytes().unwrap();
    let back = Erp::parse(&bytes).unwrap();
    let res = &back.resources[0];
    assert_eq!(res.fragments[0].data().unwrap(), b"changed");
    assert_eq!(res.content_hash().unwrap(), hash);
}

#[test]
fn stale_stats_compares_a_bxml_fragment_with_its_tsmd() {
    use crate::bxml::{Document, Element, Kind};
    let doc = |v: &[u8]| Document {
        root: Element {
            kind: Kind::Element,
            pad: 0,
            name: b"track".to_vec(),
            attrs: vec![(b"name".to_vec(), v.to_vec())],
            children: Vec::new(),
            end_pad: 0,
        },
        tail: Vec::new(),
    };
    let stock = doc(b"default");
    let t = &sample().resources[0];
    let spline = |data: &Document| {
        Resource::like(
            "eaid://environment_package/tracks/x/ai/x_ai_track.aispline",
            t,
            &[data.to_bytes(), stock.string_stats(1)],
        )
        .unwrap()
    };
    assert!(spline(&stock).has_stats() && !spline(&stock).stale_stats().unwrap());
    assert!(spline(&doc(b"a longer name")).stale_stats().unwrap());
    assert!(!sample().resources[0].has_stats() && !sample().resources[0].stale_stats().unwrap());

    let mut unchanged = spline(&stock);
    let before = unchanged.fragments[1].data().unwrap();
    assert!(!unchanged.repair_stats().unwrap());
    assert_eq!(unchanged.fragments[1].data().unwrap(), before);

    let edited = doc(b"a longer name");
    let mut res = spline(&edited);
    assert!(res.repair_stats().unwrap());
    assert_eq!(res.fragments[1].data().unwrap(), edited.string_stats(1));
    assert!(!res.stale_stats().unwrap() && !res.repair_stats().unwrap());
    assert_eq!(res.content_hash().unwrap(), res.hash);

    let mut broken = spline(&stock);
    let bytes = stock.to_bytes();
    broken.set_fragment(0, &bytes[..bytes.len() - 3]).unwrap();
    assert!(broken.repair_stats().is_err());
}

#[test]
fn like_takes_the_type_argument_and_fragment_layout_of_a_template() {
    let erp = sample();
    let t = &erp.resources[0];
    let res = Resource::like("eaid://ui_package/x.ui", t, &[b"one".to_vec(), b"two".to_vec()]).unwrap();
    assert_eq!((res.kind, &res.unk), (t.kind, &t.unk));
    for (f, s) in res.fragments.iter().zip(&t.fragments) {
        assert_eq!((f.name, f.flags, f.compression), (s.name, s.flags, s.compression));
    }
    assert_eq!(res.fragments[0].data().unwrap(), b"one");
    assert_ne!(res.fragments[0].raw(), Some(&b"one"[..]));
    assert_eq!(
        (res.fragments[0].size, res.fragments[0].packed()),
        (3, res.fragments[0].raw().map(|r| r.len() as u64))
    );
    assert_eq!(res.fragments[1].raw(), Some(&b"two"[..]));
    assert_eq!(res.hash, res.content_hash().unwrap());
    assert!(Resource::like("eaid://ui_package/x.ui", t, &[b"one".to_vec()]).is_err());
    assert!(Resource::like("", t, &[b"one".to_vec(), b"two".to_vec()]).is_err());
    assert_eq!(&kind("GfxSurfaceRes").unwrap(), b"GfxSurfaceRes\0\0\0");
    assert!(kind("AIBrakeSettings").is_ok());
    assert!(kind("SixteenBytesLong").is_err());
    assert!(kind("").is_err());
}

#[test]
fn reads_matching_resources_from_a_file() {
    let path = std::env::temp_dir().join(format!("swofty-erp-{}.erp", std::process::id()));
    std::fs::write(&path, sample().to_bytes().unwrap()).unwrap();
    let found = Erp::read_matching(&path, |n| n.ends_with("sample.ui")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].fragments[0].data().unwrap(), b"!pkg sample payload");
    assert!(Erp::read_matching(&path, |_| false).unwrap().is_empty());
    std::fs::remove_file(&path).ok();
}

fn two() -> Erp {
    let mut erp = sample();
    let mut res = erp.resources[0].clone();
    res.name = "eaid://ui_package/screens/other.ui".into();
    res.set_fragment(0, b"second payload").unwrap();
    erp.resources.push(res);
    erp
}

#[test]
fn streamed_copy_of_every_resource_is_identical() {
    let dir = std::env::temp_dir().join(format!("swofty-erpfile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.erp");
    let bytes = two().to_bytes().unwrap();
    std::fs::write(&src, &bytes).unwrap();
    let file = ErpFile::open(&src).unwrap();
    let all = file.erp.resources.iter().map(Entry::Copy).collect();
    file.write(all, &dir.join("all.erp")).unwrap();
    assert_eq!(std::fs::read(dir.join("all.erp")).unwrap(), bytes);

    let mut owned = file.load(&file.erp.resources[0]).unwrap();
    owned.name = "eaid://ui_package/screens/renamed.ui".into();
    file.write(
        vec![Entry::Copy(&file.erp.resources[1]), Entry::Owned(owned)],
        &dir.join("subset.erp"),
    )
    .unwrap();
    let back = Erp::parse(&std::fs::read(dir.join("subset.erp")).unwrap()).unwrap();
    let names: Vec<&str> = back.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "eaid://ui_package/screens/other.ui",
            "eaid://ui_package/screens/renamed.ui"
        ]
    );
    assert_eq!(back.resources[0].fragments[0].data().unwrap(), b"second payload");
    assert_eq!(
        back.resources[1].fragments[0].data().unwrap(),
        b"!pkg sample payload"
    );
    assert_eq!(back.resources[1].fragments[1].data().unwrap(), b"abc");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn copies_of_reordered_and_interleaved_stock_resources_match_the_in_memory_archive() {
    let dir = std::env::temp_dir().join(format!("swofty-erp-spans-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut erp = sample();
    let template = erp.resources[0].clone();
    for i in 0..6 {
        let mut res = template.clone();
        res.name = format!("eaid://ui_package/screens/s{i}.ui");
        res.set_fragment(0, format!("payload {i} {}", "x".repeat(i * 7000)).as_bytes())
            .unwrap();
        res.set_fragment(1, &vec![i as u8; i * 3]).unwrap();
        erp.resources.push(res);
    }
    let src = dir.join("src.erp");
    std::fs::write(&src, erp.to_bytes().unwrap()).unwrap();
    let file = ErpFile::open(&src).unwrap();
    let r = &file.erp.resources;
    let mut owned = file.load(&r[3]).unwrap();
    owned.name = "eaid://ui_package/screens/owned.ui".into();
    owned.set_fragment(0, b"owned payload").unwrap();
    let order = [
        Some(1),
        Some(2),
        None,
        Some(0),
        Some(4),
        Some(5),
        Some(6),
        Some(3),
    ];
    let entries = order
        .iter()
        .map(|i| match i {
            Some(i) => Entry::Copy(&r[*i]),
            None => Entry::Owned(owned.clone()),
        })
        .collect();
    file.write(entries, &dir.join("out.erp")).unwrap();
    let mut expected = Erp {
        header: file.erp.header,
        version: file.erp.version,
        resources: order
            .iter()
            .map(|i| match i {
                Some(i) => file.load(&r[*i]).unwrap(),
                None => owned.clone(),
            })
            .collect(),
    };
    assert_eq!(
        std::fs::read(dir.join("out.erp")).unwrap(),
        expected.to_bytes().unwrap()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_archive_rewritten_over_itself_replaces_a_read_only_copy() {
    let dir = std::env::temp_dir().join(format!("swofty-erp-in-place-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("databases.erp");
    std::fs::write(&path, two().to_bytes().unwrap()).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms).unwrap();
    let file = ErpFile::open(&path).unwrap();
    let mut owned = file.load(&file.erp.resources[1]).unwrap();
    owned.set_fragment(0, b"patched").unwrap();
    file.write(
        vec![Entry::Copy(&file.erp.resources[0]), Entry::Owned(owned)],
        &path,
    )
    .unwrap();
    let back = Erp::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(back.resources[1].fragments[0].data().unwrap(), b"patched");
    assert_eq!(
        back.resources[0].fragments[0].data().unwrap(),
        b"!pkg sample payload"
    );
    assert!(!path.with_extension("erp.tmp").exists());
    std::fs::remove_dir_all(&dir).ok();
}

fn refused(code: i32) -> std::io::Error {
    std::io::Error::from_raw_os_error(code)
}

#[test]
fn retries_a_file_windows_refuses_for_now() {
    let held = |e: &std::io::Error| matches!(e.raw_os_error(), Some(5 | 32));
    let mut waits = Vec::new();
    let mut tries = [refused(5), refused(32)].into_iter();
    let opened = retrying(&held, &mut |d| waits.push(d.as_millis()), || match tries.next() {
        Some(e) => Err(e),
        None => Ok("opened"),
    });
    assert_eq!(opened.unwrap(), "opened");
    assert_eq!(waits, [25, 50]);

    let mut waits = Vec::new();
    let mut calls = 0;
    let err = retrying(
        &held,
        &mut |d| waits.push(d.as_millis()),
        || -> std::io::Result<()> {
            calls += 1;
            Err(refused(5))
        },
    )
    .unwrap_err();
    assert_eq!(err.raw_os_error(), Some(5));
    assert_eq!(calls, BACKOFF_MS.len() + 1);
    assert_eq!(waits, BACKOFF_MS.map(u128::from));

    let mut waits = Vec::new();
    let mut calls = 0;
    let err = retrying(
        &held,
        &mut |d| waits.push(d.as_millis()),
        || -> std::io::Result<()> {
            calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert_eq!((calls, waits.len()), (1, 0));
}

#[test]
fn only_windows_refusals_count_as_held_for_now() {
    assert_eq!(transient(&refused(5)), cfg!(windows));
    assert_eq!(transient(&refused(32)), cfg!(windows));
    assert!(!transient(&refused(2)));
    assert!(!transient(&std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )));
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = sample().to_bytes().unwrap();
    bytes[0] = b'X';
    assert!(matches!(Erp::parse(&bytes), Err(Error::Magic { .. })));
}
