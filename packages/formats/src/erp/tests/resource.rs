use super::*;

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
fn a_fragment_under_any_plain_zstd_code_is_compressed_and_keeps_its_code() {
    for code in [0x03, 0x10, 0x11, 0x1f, 0x30, 0x3f] {
        let mut erp = sample();
        let res = erp.find_mut("eaid://ui_package/screens/sample.ui").unwrap();
        res.fragments[0].compression = code;
        res.set_fragment(0, &b"!pkg a payload that compresses".repeat(40))
            .unwrap();
        let f = &res.fragments[0];
        assert_eq!(f.compression, code);
        assert!(
            f.stored().unwrap().1.starts_with(&crate::erp::ZSTD_MAGIC),
            "code {code:#x}"
        );
        let back = Erp::parse(&erp.to_bytes().unwrap()).unwrap();
        assert_eq!(
            back.resources[0].fragments[0].data().unwrap(),
            b"!pkg a payload that compresses".repeat(40)
        );
    }
}

#[test]
fn a_fragment_under_a_dictionary_zstd_code_is_written_as_plain_zstd() {
    let mut erp = sample();
    let res = erp.find_mut("eaid://ui_package/screens/sample.ui").unwrap();
    res.fragments[0].compression = 0x23;
    res.set_fragment(0, b"changed").unwrap();
    let f = &res.fragments[0];
    assert_eq!(f.compression, crate::erp::ZSTD);
    assert!(f.stored().unwrap().1.starts_with(&crate::erp::ZSTD_MAGIC));
}

#[test]
fn a_stored_or_plain_fragment_stays_raw() {
    for code in [0x00, 0x01, 0x02, 0x91] {
        let mut erp = sample();
        let res = erp.find_mut("eaid://ui_package/screens/sample.ui").unwrap();
        res.fragments[0].compression = code;
        res.set_fragment(0, b"changed").unwrap();
        assert_eq!(res.fragments[0].stored().unwrap().1, b"changed", "code {code:#x}");
    }
}
