mod encoders;
mod file;
mod frame_cache;
mod packer;
mod resource;

use super::*;

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
fn reads_matching_resources_from_a_file() {
    let path = std::env::temp_dir().join(format!("swofty-erp-{}.erp", std::process::id()));
    std::fs::write(&path, sample().to_bytes().unwrap()).unwrap();
    let found = Erp::read_matching(&path, |n| n.ends_with("sample.ui")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].fragments[0].data().unwrap(), b"!pkg sample payload");
    assert!(Erp::read_matching(&path, |_| false).unwrap().is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = sample().to_bytes().unwrap();
    bytes[0] = b'X';
    assert!(matches!(Erp::parse(&bytes), Err(Error::Magic { .. })));
}
