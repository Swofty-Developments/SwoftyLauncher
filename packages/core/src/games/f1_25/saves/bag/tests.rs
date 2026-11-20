use super::*;

struct S(Vec<u8>);

impl S {
    fn new() -> Self {
        S(Vec::new())
    }

    fn head(mut self, count: u64) -> Self {
        self.0.extend(1u64.to_le_bytes());
        self.0.extend(count.to_le_bytes());
        self
    }

    fn key(mut self, k: &str) -> Self {
        self.0.push(k.len() as u8);
        self.0.extend(k.as_bytes());
        self
    }

    fn id(mut self, id: &[u8; 4]) -> Self {
        self.0.extend(id);
        self
    }

    fn raw(mut self, b: &[u8]) -> Self {
        self.0.extend(b);
        self
    }

    fn entry(self, k: &str, id: &[u8; 4], payload: &[u8]) -> Self {
        self.key(k).id(id).raw(payload)
    }
}

fn f32le(x: f32) -> [u8; 4] {
    x.to_le_bytes()
}

fn everything() -> Vec<u8> {
    let mut mat = Vec::new();
    for i in 0..16 {
        mat.extend(f32le(i as f32 * 0.5));
    }
    let inner = S::new().head(1).entry("x", b"si32", &(-7i32).to_le_bytes()).0;
    let mut list = S::new().head(3).0;
    list.extend(b"ui08");
    list.push(9);
    list.extend(b"pbag");
    list.extend(&inner);
    list.extend(b"plst");
    list.extend(S::new().head(0).0);
    let mut s = S::new()
        .head(34)
        .entry("a", b"si08", &[0xfe])
        .entry("b", b"ui08", &[0xfe])
        .entry("c", b"si16", &(-2i16).to_le_bytes())
        .entry("d", b"ui16", &65535u16.to_le_bytes())
        .entry("e", b"si32", &i32::MIN.to_le_bytes())
        .entry("f", b"ui32", &u32::MAX.to_le_bytes())
        .entry("g", b"si64", &i64::MIN.to_le_bytes())
        .entry("h", b"ui64", &u64::MAX.to_le_bytes())
        .entry("i", b"bool", &[1])
        .entry("j", b"fp32", &f32le(1.5))
        .entry("k", b"fp64", &0.1f64.to_le_bytes())
        .entry("l", b"rgba", &[1, 2, 3, 4])
        .entry("m", b"vec2", &[f32le(1.0), f32le(2.0)].concat())
        .entry("n", b"vec3", &[f32le(1.0), f32le(2.0), f32le(-3.0)].concat())
        .entry(
            "o",
            b"vec4",
            &[f32le(1.0), f32le(2.0), f32le(3.0), f32le(4.0)].concat(),
        )
        .entry("p", b"mat4", &mat)
        .entry(
            "q",
            b"quat",
            &[f32le(0.0), f32le(0.0), f32le(0.0), f32le(1.0)].concat(),
        )
        .entry("r", b"shnm", &[7; 16])
        .entry("s", b"st32", b"\x03abc")
        .entry("t", b"st64", b"\x00")
        .entry("u", b"s128", b"\x02\xff\xfe")
        .entry("v", b"s256", b"\x04\x00wxyz")
        .entry("w", b"auid", &0x0123_4567_89ab_cdefu64.to_le_bytes())
        .entry("x", b"woid", &[0xaa; 16])
        .entry("y", b"sstr", b"\x02\x00hi")
        .entry("z", b"matp", &[0x11; 16])
        .entry("A", b"dstr", b"\x05\x00\x00\x00hello")
        .entry("B", b"bffr", b"\x03\x00\x00\x00\x01\x02\x03")
        .entry("C", b"blwt", &f32le(0.25))
        .entry("D", b"uist", b"\x02\x00\x00\x00ok")
        .entry("E", b"dbrf", &[1, 2, 5, 0, 0, 0, 6, 0, 0, 0])
        .entry("F", b"dbrf", &[0])
        .0;
    s.extend(S::new().key("G").id(b"pbag").raw(&inner).0);
    s.extend(S::new().key("H").id(b"plst").raw(&list).0);
    s
}

#[test]
fn decodes_every_type_and_encodes_the_same_bytes() {
    let bytes = everything();
    let bag = decode(&bytes).unwrap();
    assert_eq!(bag.entries.len(), 34);
    assert_eq!(bag.get("a"), Some(&Value::I8(-2)));
    assert_eq!(bag.get("i"), Some(&Value::Bool(true)));
    assert_eq!(bag.get("k"), Some(&Value::F64(Float64(0.1f64.to_bits()))));
    assert_eq!(bag.get("s"), Some(&Value::St32(Text::new("abc"))));
    assert_eq!(bag.get("u"), Some(&Value::S128(Text(vec![0xff, 0xfe]))));
    assert_eq!(bag.get("E"), Some(&Value::Dbrf(Some(vec![5, 6]))));
    assert_eq!(bag.get("F"), Some(&Value::Dbrf(None)));
    let Some(Value::List(l)) = bag.get("H") else {
        panic!("list")
    };
    assert_eq!(l.items.len(), 3);
    assert_eq!(l.items[0], Value::U8(9));
    assert_eq!(encode(&bag).unwrap(), bytes);
}

#[test]
fn json_round_trip_is_byte_identical() {
    let bytes = everything();
    let json = bag_to_json(&bytes).unwrap();
    assert!(json.contains("\"type\": \"mat4\""));
    assert_eq!(bag_from_json(&json).unwrap(), bytes);
}

#[test]
fn non_finite_and_odd_floats_survive_json() {
    let bits32 = [0x7fc0_0001u32, 0xff80_0000, 0x8000_0000, 1, 0x7f7f_ffff];
    let bits64 = [
        0x7ff8_0000_0000_0001u64,
        0xfff0_0000_0000_0000,
        0x8000_0000_0000_0000,
        1,
        0x3fb9_9999_9999_999a,
    ];
    let mut bag = Bag::new();
    for (i, b) in bits32.iter().enumerate() {
        bag.push(&format!("f{i}"), Value::F32(Float32(*b)));
    }
    for (i, b) in bits64.iter().enumerate() {
        bag.push(&format!("d{i}"), Value::F64(Float64(*b)));
    }
    let bytes = encode(&bag).unwrap();
    let json = serde_json::to_string(&bag).unwrap();
    let back: Bag = serde_json::from_str(&json).unwrap();
    assert_eq!(back, bag);
    assert_eq!(encode(&back).unwrap(), bytes);
}

#[test]
fn many_random_floats_survive_json() {
    let mut x = 0x9e37_79b9_7f4a_7c15u64;
    let mut bag = Bag::new();
    for i in 0..2000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        bag.push(&format!("a{i}"), Value::F32(Float32((x >> 7) as u32)));
        bag.push(&format!("b{i}"), Value::F64(Float64(x)));
    }
    let back: Bag = serde_json::from_str(&serde_json::to_string(&bag).unwrap()).unwrap();
    assert_eq!(back, bag);
}

#[test]
fn text_that_is_not_utf8_survives_json() {
    let mut bag = Bag::new();
    bag.push("k", Value::Sstr(Text(vec![0xff, b'a', 0x80])));
    let json = serde_json::to_string(&bag).unwrap();
    assert!(json.contains("\"hex\""));
    assert_eq!(serde_json::from_str::<Bag>(&json).unwrap(), bag);
}

#[test]
fn empty_bag_is_sixteen_bytes() {
    let bytes = S::new().head(0).0;
    assert_eq!(bytes.len(), 16);
    let bag = decode(&bytes).unwrap();
    assert!(bag.entries.is_empty());
    assert_eq!(encode(&bag).unwrap(), bytes);
}

#[test]
fn every_truncation_is_a_typed_error() {
    let bytes = everything();
    for n in 0..bytes.len() {
        let e = decode(&bytes[..n]).unwrap_err();
        assert!(
            matches!(
                e,
                DecodeError::Truncated { .. } | DecodeError::CountTooLarge { .. }
            ),
            "{n}: {e}"
        );
    }
}

#[test]
fn mutated_streams_never_panic() {
    let bytes = everything();
    let mut x = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..20000 {
        let mut b = bytes.clone();
        for _ in 0..1 + (x % 4) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let i = (x as usize) % b.len();
            b[i] = (x >> 32) as u8;
        }
        if let Ok(bag) = decode(&b) {
            let _ = encode(&bag);
        }
    }
}

#[test]
fn rejects_what_the_game_rejects_and_what_it_never_writes() {
    let mut v2 = S::new().head(0).0;
    v2[0] = 2;
    assert!(matches!(
        decode(&v2),
        Err(DecodeError::UnsupportedVersion { found: 2, .. })
    ));

    let unknown = S::new().head(1).entry("k", b"zzzZ", &[]).0;
    assert!(matches!(decode(&unknown), Err(DecodeError::UnknownType { .. })));

    let huge = S::new().head(70000).0;
    assert!(matches!(decode(&huge), Err(DecodeError::CountTooLarge { .. })));
    let lie = S::new().head(5).0;
    assert!(matches!(decode(&lie), Err(DecodeError::CountTooLarge { .. })));

    let b2 = S::new().head(1).entry("k", b"bool", &[2]).0;
    assert!(matches!(
        decode(&b2),
        Err(DecodeError::NonCanonical { what: "bool", .. })
    ));
    let d2 = S::new().head(1).entry("k", b"dbrf", &[2]).0;
    assert!(matches!(decode(&d2), Err(DecodeError::NonCanonical { .. })));

    let mut trailing = S::new().head(0).0;
    trailing.push(0);
    assert!(matches!(
        decode(&trailing),
        Err(DecodeError::TrailingBytes { len: 1, .. })
    ));
    assert_eq!(decode_prefix(&trailing).unwrap().1, 16);

    let long = S::new().head(1).entry("k", b"dstr", &[0xff, 0xff, 0xff, 0xff]).0;
    assert!(matches!(decode(&long), Err(DecodeError::Truncated { .. })));
}

#[test]
fn deep_nesting_is_bounded() {
    let mut inner = S::new().head(0).0;
    for _ in 0..200 {
        inner = S::new().head(1).entry("n", b"pbag", &inner).0;
    }
    assert!(matches!(decode(&inner), Err(DecodeError::TooDeep { .. })));

    let mut ok = S::new().head(0).0;
    for _ in 0..30 {
        ok = S::new().head(1).entry("n", b"pbag", &ok).0;
    }
    let bag = decode(&ok).unwrap();
    assert_eq!(encode(&bag).unwrap(), ok);

    let mut deep = Bag::new();
    for _ in 0..200 {
        let mut up = Bag::new();
        up.push("n", Value::Bag(deep));
        deep = up;
    }
    assert!(matches!(encode(&deep), Err(EncodeError::TooDeep { .. })));
}

#[test]
fn encode_refuses_what_does_not_fit() {
    let mut bag = Bag::new();
    bag.push(&"k".repeat(256), Value::U8(0));
    assert!(matches!(
        encode(&bag),
        Err(EncodeError::TooLong { what: "key", .. })
    ));

    let mut bag = Bag::new();
    for _ in 0..=0xFFFF {
        bag.push("k", Value::Bool(false));
    }
    assert!(matches!(encode(&bag), Err(EncodeError::TooMany { .. })));

    let mut bag = Bag::new();
    bag.push("k", Value::Dbrf(Some(vec![0; 256])));
    assert!(matches!(encode(&bag), Err(EncodeError::TooManyIds { .. })));

    let mut bag = Bag::new();
    bag.version = 2;
    assert!(matches!(
        encode(&bag),
        Err(EncodeError::UnsupportedVersion { .. })
    ));
}

#[test]
fn version_zero_is_kept() {
    let mut bytes = S::new().head(0).0;
    bytes[0] = 0;
    let bag = decode(&bytes).unwrap();
    assert_eq!(bag.version, 0);
    assert_eq!(encode(&bag).unwrap(), bytes);
}

#[test]
fn type_ids_spell_their_names() {
    assert_eq!(Value::Bool(true).type_id().to_le_bytes(), *b"bool");
    assert_eq!(Value::Bag(Bag::new()).type_id(), 0x6761_6270);
    assert_eq!(Value::Dstr(Text::new("")).type_id(), 0x7274_7364);
}
