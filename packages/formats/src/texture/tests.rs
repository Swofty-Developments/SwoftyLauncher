use super::*;
use crate::erp::Fragment;

fn fragment(raw: Vec<u8>) -> Fragment {
    Fragment::new(*b"temp", raw.len() as u64, 0, 0, raw)
}

fn surface_with(format: u32, width: u32, height: u32, levels: u32, data: Vec<u8>) -> Resource {
    let mut head = vec![0; 40];
    for (at, v) in [(8, format), (12, width), (16, height), (24, levels), (28, 1)] {
        head[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    Resource {
        name: "flag.tif.image".into(),
        kind: [0; 16],
        unk: vec![0; 4],
        fragments: vec![fragment(head), fragment(data)],
        hash: [0; 16],
    }
}

fn surface(format: u32, width: u32, height: u32, data: Vec<u8>) -> Resource {
    surface_with(format, width, height, 1, data)
}

const BC1_RED: [u8; 8] = [0x00, 0xf8, 0x00, 0xf8, 0, 0, 0, 0];
const BC1_BLUE: [u8; 8] = [0x1f, 0x00, 0x1f, 0x00, 0, 0, 0, 0];

#[test]
fn decodes_and_crops_bc7() {
    let block = vec![
        0x40, 0x20, 0x10, 0x02, 0xf9, 0xff, 0xff, 0xff, 0x01, 0, 0, 0, 0, 0, 0, 0,
    ];
    let image = decode(&surface(BC7, 3, 2, block)).unwrap();
    assert_eq!((image.width, image.height), (3, 2));
    assert_eq!(image.rgba, [0x81, 0x21, 0xff, 0xff].repeat(6));
}

#[test]
fn decodes_bc1_blocks_in_place() {
    let image = decode(&surface(54, 8, 4, [BC1_RED, BC1_BLUE].concat())).unwrap();
    assert_eq!(&image.rgba[..4], &[255, 0, 0, 255]);
    assert_eq!(&image.rgba[16..20], &[0, 0, 255, 255]);
    assert_eq!(&image.rgba[image.rgba.len() - 4..], &[0, 0, 255, 255]);
}

#[test]
fn decodes_bc3_alpha() {
    let mut block = vec![0x80, 0x80, 0, 0, 0, 0, 0, 0];
    block.extend(BC1_RED);
    let image = decode(&surface(57, 4, 4, block)).unwrap();
    assert!(image.rgba.chunks(4).all(|p| p == [255, 0, 0, 0x80]));
}

#[test]
fn decodes_bc5_into_red_green() {
    let mut block = vec![200, 200, 0, 0, 0, 0, 0, 0];
    block.extend([40, 40, 0, 0, 0, 0, 0, 0]);
    let image = decode(&surface(65, 4, 4, block)).unwrap();
    assert!(image.rgba.chunks(4).all(|p| p == [200, 40, 0, 255]));
}

#[test]
fn rejects_other_formats_and_short_data() {
    assert!(decode(&surface(0x47, 4, 4, vec![0; 16])).is_err());
    assert!(decode(&surface(54, 8, 8, vec![0; 16])).is_err());
}

#[test]
fn picks_an_inner_level() {
    let data = [BC1_RED.repeat(4), BC1_BLUE.to_vec(), BC1_BLUE.to_vec()].concat();
    let s = surface_with(54, 8, 8, 3, data);
    let near = decode_near(&s, None, 4).unwrap();
    assert_eq!((near.width, near.height), (4, 4));
    assert_eq!(&near.rgba[..4], &[0, 0, 255, 255]);
    let big = decode_near(&s, None, 64).unwrap();
    assert_eq!((big.width, big.height), (8, 8));
    assert_eq!(&big.rgba[..4], &[255, 0, 0, 255]);
}

fn descriptor(path: &str, levels: &[(u8, u64, u64, u64)]) -> Vec<u8> {
    let mut d = vec![(path.len() + 1) as u8];
    d.extend(path.as_bytes());
    d.extend([0, 0]);
    d.extend((levels.len() as u32).to_le_bytes());
    for &(c, offset, packed, size) in levels {
        d.extend([0, c]);
        for v in [offset, packed, size] {
            d.extend(v.to_le_bytes());
        }
    }
    d
}

#[test]
fn parses_the_side_file_descriptor() {
    let d = descriptor("a\\b.mipmaps", &[(3, 0, 10, 32), (0, 10, 8, 8)]);
    let side = SideFile::parse(&d).unwrap();
    assert_eq!(side.path, "a/b.mipmaps");
    assert_eq!(
        side.levels,
        [
            SideLevel {
                compression: 3,
                offset: 0,
                packed: 10,
                size: 32
            },
            SideLevel {
                compression: 0,
                offset: 10,
                packed: 8,
                size: 8
            },
        ]
    );
    assert!(SideFile::parse(&d[..d.len() - 1]).is_err());
    assert!(SideFile::parse(&descriptor("../x", &[])).is_err());
    assert!(SideFile::parse(&descriptor("/x", &[])).is_err());
}

#[test]
fn reads_bigger_levels_from_the_side_file() {
    let dir = std::env::temp_dir().join(format!("swofty-side-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut packed = vec![0x8f];
    packed.extend(BC1_RED);
    packed.extend([8, 0, 5]);
    let mut raw = BC1_BLUE.repeat(4);
    raw.extend(&packed);
    std::fs::write(dir.join("t.mipmaps"), &raw).unwrap();
    let mut s = surface_with(54, 4, 4, 1, BC1_RED.to_vec());
    let levels = [(3, 32, packed.len() as u64, 128), (0, 0, 32, 32)];
    s.fragments.push(fragment(descriptor("t.mipmaps", &levels)));
    let biggest = decode_near(&s, Some(&dir), 16).unwrap_err();
    assert!(biggest.to_string().contains("lz4"), "{biggest}");
    let mid = decode_near(&s, Some(&dir), 8).unwrap();
    assert_eq!((mid.width, mid.height), (8, 8));
    assert!(mid.rgba.chunks(4).all(|p| p == [0, 0, 255, 255]));
    let small = decode_near(&s, None, 8).unwrap();
    assert_eq!((small.width, small.height), (4, 4));
    assert!(small.rgba.chunks(4).all(|p| p == [255, 0, 0, 255]));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn decompresses_lz4_side_levels() {
    let dir = std::env::temp_dir().join(format!("swofty-side-lz4-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut packed = vec![0x8f];
    packed.extend(BC1_RED);
    packed.extend([8, 0, 5]);
    std::fs::write(dir.join("t.mipmaps"), &packed).unwrap();
    let mut s = surface_with(54, 4, 4, 1, BC1_BLUE.to_vec());
    s.fragments.push(fragment(descriptor(
        "t.mipmaps",
        &[(3, 0, packed.len() as u64, 32)],
    )));
    let big = decode_near(&s, Some(&dir), 8).unwrap();
    assert_eq!((big.width, big.height), (8, 8));
    assert!(big.rgba.chunks(4).all(|p| p == [255, 0, 0, 255]));
    std::fs::remove_dir_all(&dir).ok();
}
