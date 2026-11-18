use super::*;

use crate::Error;
use crate::erp::file::{retrying, transient};

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
