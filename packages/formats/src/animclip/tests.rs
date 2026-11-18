use super::*;
use crate::pkg::{Arr, Obj};

fn raw(ty: &[u8; 4], b: &[u8]) -> Node {
    Node::Val(Val::raw(*ty, b))
}

fn pkg(name: &[u8; 4], items: Vec<(&str, Node)>) -> Pkg {
    Pkg {
        name: *name,
        root: Node::Obj(Obj::new(
            items.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
        )),
        vca: vec![],
        layout: None,
    }
}

fn template() -> Vec<u8> {
    let channel = Node::Obj(Obj::new(vec![
        (Val::str("name_index"), raw(b"si32", &[0; 4])),
        (Val::str("mins"), raw(b"ui64", &[0; 8])),
        (Val::str("misc"), raw(b"ui64", &[0; 8])),
        (Val::str("name"), Node::Val(Val::str("x"))),
        (Val::str("keyptr"), raw(b"ui32", &[0; 4])),
        (Val::str("timesptr"), raw(b"ui32", &[0; 4])),
    ]));
    let packet = Node::Obj(Obj::new(vec![
        (Val::str("maxtime"), raw(b"fp32", &[0; 4])),
        (Val::str("mintime"), raw(b"fp32", &[0; 4])),
        (Val::str("interpolator"), raw(b"ui32", &[0; 4])),
        (Val::str("channels"), Node::Arr(Arr::new(vec![channel]))),
    ]));
    pkg(
        b"ANIM",
        vec![
            ("packets", Node::Arr(Arr::new(vec![packet]))),
            ("string_table", Node::Val(Val::str("t"))),
        ],
    )
    .to_bytes()
    .unwrap()
}

fn channels() -> Vec<Channel> {
    vec![
        Channel::position(vec![0.0, 1.5], vec![[-1.0, 0.0, 0.5], [1.0, 2.0, 0.5]]),
        Channel::rotation(
            vec![0.0, 0.75, 1.5],
            vec![[0.0, 0.0, 0.0, -1.0], [0.0, 0.0, 0.5, -0.8], [0.0, 0.0, 1.0, 0.0]],
        ),
        Channel::scale(vec![0.0], vec![[1.0, 1.0, 1.0]]),
    ]
}

#[test]
fn a_built_clip_reads_back_and_rebuilds_identically() {
    let (temp, syst) = build_clip(&template(), "zh_a", 7, "tbl", &channels()).unwrap();
    let clip = read_clip(&temp, &syst).unwrap();
    assert_eq!((clip.node.as_str(), clip.name_index), ("zh_a", 7));
    assert_eq!(clip.string_table, "tbl");
    assert_eq!(clip.channels, channels());
    let again = build_clip(
        &temp,
        &clip.node,
        clip.name_index,
        &clip.string_table,
        &clip.channels,
    )
    .unwrap();
    assert_eq!(again, (temp, syst));
}

#[test]
fn a_one_key_rotation_is_told_apart() {
    let (temp, syst) = build_clip(&template(), "n", 0, "t", &channels()).unwrap();
    assert!(!has_constant_rotation(&temp, &syst).unwrap());
    let mut c = channels();
    c.push(Channel::rotation(vec![0.0], vec![[0.0, 0.0, 0.0, -1.0]]));
    let (temp, syst) = build_clip(&template(), "n", 0, "t", &c).unwrap();
    assert!(has_constant_rotation(&temp, &syst).unwrap());
}

fn key_offsets(temp: &[u8]) -> Vec<u32> {
    let mut p = Pkg::parse(temp).unwrap();
    packets(&mut p)
        .unwrap()
        .items
        .iter()
        .map(|pk| {
            let ch = pk
                .as_obj()
                .and_then(|o| o.get("channels"))
                .and_then(Node::as_arr)
                .and_then(|a| a.items.first())
                .unwrap();
            u32_of(ch, "keyptr").unwrap()
        })
        .collect()
}

fn game_slot(syst_len: usize) -> usize {
    let request = syst_len + if syst_len.is_multiple_of(16) { 16 } else { 7 };
    let n = request - 1;
    let shift = if n < 64 { 3 } else { 3 + (n >> 6).ilog2() };
    ((n >> shift) + 1) << shift
}

fn needle(keys: usize) -> Vec<Channel> {
    let times = (0..keys).map(|i| i as f32).collect();
    vec![
        Channel::position(vec![0.0], vec![[0.1, 0.2, 0.0]]),
        Channel::rotation(times, vec![[0.0, 0.0, 0.0, -1.0]; keys]),
        Channel::scale(vec![0.0], vec![[1.0; 3]]),
    ]
}

fn slide(keys: usize) -> Vec<Channel> {
    let times = (0..keys).map(|i| i as f32).collect();
    vec![Channel::position(times, vec![[0.5, 0.25, 0.0]; keys])]
}

#[test]
fn the_game_slot_model_matches_the_crashed_and_the_stock_buffers() {
    assert_eq!(game_slot(92), 104);
    assert_eq!(game_slot(108), 120);
    assert_eq!(game_slot(96), 112);
    assert_eq!(game_slot(124), 144);
    assert_eq!(game_slot(216), 224);
}

#[test]
fn every_syst_is_padded_to_16_bytes_with_its_keys_on_16() {
    for keys in 1..=12 {
        for channels in [needle(keys), slide(keys), channels()] {
            let (temp, syst) = build_clip(&template(), "n", 0, "t", &channels).unwrap();
            assert!(
                syst.len().is_multiple_of(SYST_ALIGN),
                "{keys} keys: {}",
                syst.len()
            );
            assert!(
                game_slot(syst.len()).is_multiple_of(16),
                "{keys} keys: {}",
                syst.len()
            );
            assert!(key_offsets(&temp).iter().all(|k| k % 16 == 0));
            assert_eq!(read_clip(&temp, &syst).unwrap().channels, channels);
        }
    }
}

#[test]
fn a_two_key_needle_is_no_longer_108_bytes() {
    let (_, syst) = build_clip(&template(), "n", 0, "t", &needle(2)).unwrap();
    assert_eq!(syst.len(), 112);
    assert_eq!(&syst[108..], &[0; 4]);
}

#[test]
fn keys_must_match_times() {
    let mut c = channels();
    c[0].keys.pop();
    assert!(build_clip(&template(), "n", 0, "t", &c).is_err());
}

#[test]
fn strings_append_in_order_only() {
    let t = pkg(
        b"ANST",
        vec![(
            "strings",
            Node::Arr(Arr::new(vec![
                Node::Val(Val::str("Alpha")),
                Node::Val(Val::str("Beta")),
            ])),
        )],
    )
    .to_bytes()
    .unwrap();
    let (out, first) = append_strings(&t, &["zh_a", "zh_b"]).unwrap();
    assert_eq!(first, 2);
    assert_eq!(read_strings(&out).unwrap(), ["Alpha", "Beta", "zh_a", "zh_b"]);
    assert!(append_strings(&t, &["Aaa"]).is_err());
    assert!(append_strings(&t, &["Alpha"]).is_err());
    assert!(append_strings(&t, &["zh_b", "zh_a"]).is_err());
}
