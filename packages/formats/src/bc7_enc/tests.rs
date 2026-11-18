use super::*;

#[test]
fn a_solid_block_is_one_flat_colour() {
    let rgba: Vec<u8> = [200, 100, 50, 255].repeat(16);
    let out = encode_rgba(&rgba, 4, 4).unwrap();
    assert_eq!(out.len(), 16);
    assert_eq!(out[0] & 0x7f, 1 << 6, "mode 6");
    let dec = decode6(out[..16].try_into().unwrap());
    for p in dec {
        assert!((p[0] - 200).abs() <= 1 && (p[1] - 100).abs() <= 1 && p[3] >= 254);
    }
}

#[test]
fn rejects_sizes_that_are_not_blocks() {
    assert!(encode_rgba(&[0; 12], 3, 1).is_err());
    assert!(encode_rgba(&[0; 10], 4, 4).is_err());
    assert!(encode_logo(&[], 0, 0).is_err());
}

#[test]
fn soft_alpha_picks_mode_five_somewhere() {
    let mut rgba = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            rgba.extend_from_slice(&[255, 40 * x as u8, 20 * y as u8, (x * 64 + y * 5) as u8]);
        }
    }
    let a = encode_logo(&rgba, 4, 4).unwrap();
    let b = encode_rgba(&rgba, 4, 4).unwrap();
    assert!(a == b || a[0] & 0x3f == 1 << 5);
}

#[test]
fn transparent_blocks_do_not_panic() {
    let mut rgba = vec![0u8; 4 * 4 * 4];
    for (i, v) in rgba.iter_mut().enumerate() {
        if i % 4 != 3 {
            *v = (i * 37 % 251) as u8;
        }
    }
    encode_rgba(&rgba, 4, 4).unwrap();
    encode_logo(&rgba, 4, 4).unwrap();
}
