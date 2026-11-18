use super::*;
use crate::hash::fnv1a32;

fn sample() -> Pkg {
    let props = Obj::new(vec![
        (Val::str("name"), Node::Val(Val::str("fe_home"))),
        (Val::str("id"), Node::Val(Val::u32(0xdead_beef))),
        (Val::str("visible"), Node::Val(Val::bool(true))),
    ]);
    let root = Obj::new(vec![(
        Val::str("Screens"),
        Node::Arr(Arr::new(vec![Node::Arr(Arr::new(vec![
            Node::Obj(props),
            Node::Iar([7; 8]),
        ]))])),
    )]);
    Pkg {
        name: *b"test",
        root: Node::Obj(root),
        vca: Vec::new(),
        layout: None,
    }
}

#[test]
fn write_parse_write_is_identical() {
    let bytes = sample().to_bytes().unwrap();
    let back = Pkg::parse(&bytes).unwrap();
    assert!(back.layout.is_none());
    assert_eq!(back.to_bytes().unwrap(), bytes);
    let screens = back
        .root
        .as_obj()
        .unwrap()
        .get("Screens")
        .unwrap()
        .as_arr()
        .unwrap();
    let props = screens.items[0].as_arr().unwrap().items[0].as_obj().unwrap();
    assert_eq!(
        props.get("name").unwrap().as_val().unwrap().as_str(),
        Some("fe_home")
    );
    assert_eq!(
        props.get("id").unwrap().as_val().unwrap().as_u32(),
        Some(0xdead_beef)
    );
}

#[test]
fn edits_intern_new_values() {
    let bytes = sample().to_bytes().unwrap();
    let mut pkg = Pkg::parse(&bytes).unwrap();
    let screens = pkg
        .root
        .as_obj_mut()
        .unwrap()
        .get_mut("Screens")
        .unwrap()
        .as_arr_mut()
        .unwrap();
    let props = screens.items[0].as_arr_mut().unwrap().items[0]
        .as_obj_mut()
        .unwrap();
    props
        .get_mut("name")
        .unwrap()
        .as_val_mut()
        .unwrap()
        .set_str("mod_menu");
    props.set("tooltipsName", Node::Val(Val::str("mod_menu")));
    let out = pkg.to_bytes().unwrap();
    let back = Pkg::parse(&out).unwrap();
    let screens = back
        .root
        .as_obj()
        .unwrap()
        .get("Screens")
        .unwrap()
        .as_arr()
        .unwrap();
    let props = screens.items[0].as_arr().unwrap().items[0].as_obj().unwrap();
    assert_eq!(
        props.get("name").unwrap().as_val().unwrap().as_str(),
        Some("mod_menu")
    );
    assert_eq!(props.keys().count(), 4);
    assert_eq!(back.to_bytes().unwrap(), out);
}

#[test]
fn version_one_keys_are_hash_sorted() {
    let bytes = sample().to_bytes().unwrap();
    let back = Pkg::parse(&bytes).unwrap();
    let screens = back
        .root
        .as_obj()
        .unwrap()
        .get("Screens")
        .unwrap()
        .as_arr()
        .unwrap();
    let props = screens.items[0].as_arr().unwrap().items[0].as_obj().unwrap();
    let hashes: Vec<u32> = props.keys().map(fnv1a32).collect();
    assert!(hashes.windows(2).all(|w| w[0] <= w[1]));
}

fn edited_in_place() -> Vec<u8> {
    let (s, chunk) = (|o: u32| o, |o: u32| 0x8000_0000 | o);
    let mut d = b"!pkgtest".to_vec();
    for v in [chunk(16), chunk(80)] {
        d.extend(v.to_le_bytes());
    }
    let mut put = |tag: &[u8; 4], head: u32, refs: &[u32]| {
        d.extend(tag);
        d.extend(head.to_le_bytes());
        refs.iter().for_each(|r| d.extend(r.to_le_bytes()));
    };
    put(b"!idi", 2, &[s(0), chunk(64), s(7), chunk(52)]);
    put(b"!ili", 1, &[s(15)]);
    put(b"!ili", 1, &[s(18)]);
    put(b"!ili", 2, &[s(15), s(18)]);
    put(b"!vca", 1, &[chunk(92)]);
    let strings = b"\x05first\0\x06second\0\x01a\0\x01b\0";
    put(b"!sbi", strings.len() as u32, &[]);
    d.extend(strings);
    d
}

fn list(p: &Pkg, key: &str) -> Vec<String> {
    let items = &p.root.as_obj().unwrap().get(key).unwrap().as_arr().unwrap().items;
    items
        .iter()
        .map(|v| v.as_val().unwrap().as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_layout_from_an_in_place_editor_round_trips() {
    let bytes = edited_in_place();
    let pkg = Pkg::parse(&bytes).unwrap();
    assert!(pkg.layout.is_some());
    assert_eq!(
        (list(&pkg, "first"), list(&pkg, "second")),
        (vec!["a".into(), "b".into()], vec!["b".into()])
    );
    assert_eq!(pkg.to_bytes().unwrap(), bytes);

    let mut renamed = pkg.clone();
    let second = renamed.root.as_obj_mut().unwrap().get_mut("second").unwrap();
    second.as_arr_mut().unwrap().items[0]
        .as_val_mut()
        .unwrap()
        .set_str("c");
    let out = renamed.to_bytes().unwrap();
    assert_eq!(out[40..52], bytes[40..52]);
    assert_eq!(list(&Pkg::parse(&out).unwrap(), "second"), ["c"]);

    let mut grown = pkg;
    let second = grown.root.as_obj_mut().unwrap().get_mut("second").unwrap();
    second.as_arr_mut().unwrap().items.push(Node::Val(Val::str("c")));
    let out = grown.to_bytes().unwrap();
    let back = Pkg::parse(&out).unwrap();
    assert!(back.layout.is_none());
    assert_eq!(back.to_bytes().unwrap(), out);
    assert_eq!(
        (list(&back, "first"), list(&back, "second")),
        (vec!["a".into(), "b".into()], vec!["b".into(), "c".into()])
    );
}

fn menu() -> Pkg {
    let widget = |name: &str, i: u8| {
        let mut o = Obj::new(vec![
            (Val::str("name"), Node::Val(Val::str(name))),
            (Val::str("pos"), Node::Val(Val::raw(*b"vec4", [i; 16]))),
            (
                Val::str("guid"),
                Node::Val(Val::raw(*b"ui64", [i, 1, 2, 3, 4, 5, 6, 7])),
            ),
            (Val::str("flags"), Node::Val(Val::raw(*b"ui16", [i, 0]))),
            (Val::str("visible"), Node::Val(Val::bool(i.is_multiple_of(2)))),
            (Val::str("bounds"), Node::Val(Val::raw(*b"bbox", [i; 32]))),
            (Val::str("xform"), Node::Val(Val::raw(*b"mat4", [i; 64]))),
            (Val::str("tint"), Node::Val(Val::raw(*b"ui08", [i]))),
            (Val::str("children"), Node::Arr(Arr::new(vec![Node::Iar([i; 8])]))),
        ]);
        o.version = i % 2;
        Node::Obj(o)
    };
    let screen = |name: &str, n: u8| {
        Node::Obj(Obj::new(vec![
            (Val::str("name"), Node::Val(Val::str(name))),
            (Val::str("id"), Node::Val(Val::u32(0x1000 + n as u32))),
            (
                Val::str("Widgets"),
                Node::Arr(Arr::new(
                    (0..n).map(|i| widget(&format!("{name}_w{i}"), i)).collect(),
                )),
            ),
        ]))
    };
    let mut tags = Arr::new(vec![Node::Val(Val::str("fe")), Node::Val(Val::str("home"))]);
    tags.tag = *b"!vca";
    let root = Obj::new(vec![
        (
            Val::str("Screens"),
            Node::Arr(Arr::new(vec![screen("fe_home", 3), screen("fe_options", 2)])),
        ),
        (Val::str("Tags"), Node::Arr(tags)),
        (Val::str("name"), Node::Val(Val::str("fe_home"))),
    ]);
    Pkg {
        name: *b"menu",
        root: Node::Obj(root),
        vca: Vec::new(),
        layout: None,
    }
}

fn pin(bytes: &[u8]) -> (usize, u32) {
    (bytes.len(), fnv1a32(bytes))
}

#[test]
fn a_menu_tree_writes_the_pinned_bytes() {
    let bytes = menu().to_bytes().unwrap();
    assert_eq!(pin(&bytes), (1588, 0xe2049b17));
    let back = Pkg::parse(&bytes).unwrap();
    assert!(back.layout.is_none());
    assert_eq!(back.to_bytes().unwrap(), bytes);
}

#[test]
fn an_edited_menu_writes_the_pinned_bytes() {
    let mut pkg = Pkg::parse(&menu().to_bytes().unwrap()).unwrap();
    let root = pkg.root.as_obj_mut().unwrap();
    root.remove("name");
    root.set("tooltipsName", Node::Val(Val::str("mod_menu")));
    let screens = root.get_mut("Screens").unwrap().as_arr_mut().unwrap();
    let home = screens.items[0].as_obj_mut().unwrap();
    home.get_mut("id").unwrap().as_val_mut().unwrap().set_u32(0xfeed);
    let widgets = home.get_mut("Widgets").unwrap().as_arr_mut().unwrap();
    widgets.items[1]
        .as_obj_mut()
        .unwrap()
        .get_mut("name")
        .unwrap()
        .as_val_mut()
        .unwrap()
        .set_str("fe_home_mods");
    widgets.items.push(Node::Val(Val::str("fe_home_w0")));
    let out = pkg.to_bytes().unwrap();
    assert_eq!(pin(&out), (1620, 0x3c6e52c7));
    assert_eq!(Pkg::parse(&out).unwrap().to_bytes().unwrap(), out);
}

#[test]
fn in_place_edits_write_the_pinned_bytes() {
    let pkg = Pkg::parse(&edited_in_place()).unwrap();
    let mut renamed = pkg.clone();
    let second = renamed.root.as_obj_mut().unwrap().get_mut("second").unwrap();
    second.as_arr_mut().unwrap().items[0]
        .as_val_mut()
        .unwrap()
        .set_str("c");
    assert_eq!(pin(&renamed.to_bytes().unwrap()), (124, 0xc0cf6709));
    let mut grown = pkg;
    let first = grown.root.as_obj_mut().unwrap().get_mut("first").unwrap();
    first
        .as_arr_mut()
        .unwrap()
        .items
        .push(Node::Val(Val::str("first")));
    assert_eq!(pin(&grown.to_bytes().unwrap()), (116, 0xed3e5421));
}
