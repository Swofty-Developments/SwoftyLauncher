use super::*;

#[test]
fn patches_layers_and_writes_only_changed_archives() {
    let game = std::env::temp_dir().join(format!("swofty-erpbuild-{}", std::process::id()));
    let wep = game.join("tracks/t/wep");
    std::fs::create_dir_all(&wep).unwrap();
    let world = erp(&[
        ("eaid://t-a.layer", layer(&["a$PitBox12"])),
        ("eaid://t-b.layer", layer(&["b$X"])),
    ]);
    std::fs::write(wep.join("t_world.erp"), &world).unwrap();
    std::fs::write(
        wep.join("t_common.erp"),
        erp(&[("eaid://t-c.layer", layer(&["c$GridSlot23"]))]),
    )
    .unwrap();
    let ops = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"patch-pkg","erp":"tracks/t/wep/t_world.erp","resource":"eaid://t-a.layer","edits":[
              {"op":"clone-entry","at":"instances","src":"[name=a$PitBox12]","as":"a$PitBox13","set":{"id":13}},
              {"op":"clone-entry","at":"instances","from":{"erp":"tracks/t/wep/t_common.erp","resource":"eaid://t-c.layer"},
               "src":"[name=c$GridSlot23]","as":"a$GridSlot25","set":{"id":25}}]},
            {"op":"patch-pkg","erp":"tracks/t/wep/t_common.erp","resource":"eaid://t-c.layer","edits":[]},
            {"op":"warn","message":"room for 26 cars"}]}"#,
    );
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &game).unwrap();
    assert_eq!(b.changed().collect::<Vec<_>>(), ["tracks/t/wep/t_world.erp"]);
    let out = game.join("out");
    assert_eq!(b.write(&out).unwrap(), ["tracks/t/wep/t_world.erp"]);
    let written = Erp::parse(&std::fs::read(out.join("tracks/t/wep/t_world.erp")).unwrap()).unwrap();
    let res = written.find("eaid://t-a.layer").unwrap();
    assert_eq!(res.content_hash().unwrap(), res.hash);
    let pkg = Pkg::parse(&res.fragments[0].data().unwrap()).unwrap();
    let names: Vec<String> = pkg
        .root
        .as_obj()
        .unwrap()
        .get("instances")
        .unwrap()
        .as_arr()
        .unwrap()
        .items
        .iter()
        .map(|n| {
            n.as_obj()
                .unwrap()
                .get("name")
                .unwrap()
                .as_val()
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(names, ["a$PitBox12", "a$PitBox13", "a$GridSlot25"]);
    let mut whole = Erp::parse(&world).unwrap();
    let patched = res.fragments[0].data().unwrap();
    whole
        .find_mut("eaid://t-a.layer")
        .unwrap()
        .set_fragment(0, &patched)
        .unwrap();
    assert_eq!(
        whole.to_bytes().unwrap(),
        std::fs::read(out.join("tracks/t/wep/t_world.erp")).unwrap()
    );
    let untouched = Erp::parse(&world).unwrap();
    assert_eq!(
        written.find("eaid://t-b.layer").unwrap().fragments[0].raw(),
        untouched.find("eaid://t-b.layer").unwrap().fragments[0].raw()
    );
    let mut bad = ErpBuild::new(&original);
    let text = erp_ops(
        r#"{"schema_version":1,"ops":[{"op":"patch-pkg","erp":"x.erp","resource":"r","edits":[
            {"op":"set-text","node":"n","value":"v"}]}]}"#,
    );
    assert_eq!(unsupported(&text[0]), Some("set-text"));
    assert!(apply(&mut bad, &text, &game).is_err());
    let escape = erp_ops(
        r#"{"schema_version":1,"ops":[{"op":"patch-pkg","erp":"../x.erp","resource":"r","edits":[
            {"op":"set-value","path":"a","value":1}]}]}"#,
    );
    assert!(apply(&mut bad, &escape, &game).is_err());
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn copies_resources_from_game_and_mod_archives() {
    let root = std::env::temp_dir().join(format!("swofty-erpcopy-{}", std::process::id()));
    let game = root.join("game");
    let addon_dir = root.join("addon");
    std::fs::create_dir_all(game.join("cars")).unwrap();
    std::fs::create_dir_all(addon_dir.join("erp")).unwrap();
    std::fs::write(
        game.join("cars/slot.erp"),
        erp(&[("eaid://slot-a.layer", layer(&["a"]))]),
    )
    .unwrap();
    std::fs::write(
        game.join("cars/stock.erp"),
        erp(&[("eaid://stock-b.layer", layer(&["b"]))]),
    )
    .unwrap();
    let src = erp(&[
        ("eaid://car.wep", layer(&["w"])),
        ("eaid://car-model.layer", layer(&["m"])),
    ]);
    std::fs::write(addon_dir.join("erp/src.erp"), &src).unwrap();
    let ops = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"copy-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car.wep","as":"eaid://car_alt.wep"},
            {"op":"copy-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car-model.layer"},
            {"op":"copy-resource","from_erp":"cars/stock.erp","erp":"cars/slot.erp","resource":"eaid://stock-b.layer"},
            {"op":"patch-pkg","erp":"cars/slot.erp","resource":"eaid://car_alt.wep","edits":[
              {"op":"set-value","path":"instances[0].name","value":"w_alt"}]}]}"#,
    );
    assert!(ops.iter().all(|op| unsupported(op).is_none()));
    let named: Vec<String> = ops
        .iter()
        .flat_map(|op| {
            AssetOp::Erp(op.clone())
                .sources()
                .into_iter()
                .map(|s| s.path.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(named, ["erp/src.erp", "erp/src.erp"]);
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &addon_dir).unwrap();
    let out = root.join("out");
    assert_eq!(b.write(&out).unwrap(), ["cars/slot.erp"]);
    let written = Erp::parse(&std::fs::read(out.join("cars/slot.erp")).unwrap()).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "eaid://slot-a.layer",
            "eaid://car_alt.wep",
            "eaid://car-model.layer",
            "eaid://stock-b.layer"
        ]
    );
    let source = Erp::parse(&src).unwrap();
    let copied = written.find("eaid://car-model.layer").unwrap();
    assert_eq!(
        copied.fragments[0].raw(),
        source.find("eaid://car-model.layer").unwrap().fragments[0].raw()
    );
    assert_eq!(copied.hash, copied.content_hash().unwrap());
    let wep = written.find("eaid://car_alt.wep").unwrap();
    assert_eq!(wep.hash, wep.content_hash().unwrap());
    let pkg = Pkg::parse(&wep.fragments[0].data().unwrap()).unwrap();
    let name = pkg
        .root
        .as_obj()
        .unwrap()
        .get("instances")
        .unwrap()
        .as_arr()
        .unwrap()
        .items[0]
        .as_obj()
        .unwrap()
        .get("name")
        .unwrap()
        .as_val()
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(name, "w_alt");
    let twice = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"copy-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car.wep"},
            {"op":"copy-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car.wep"}]}"#,
    );
    assert!(apply(&mut ErpBuild::new(&original), &twice, &addon_dir).is_err());
    let both = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"copy-resource","source":"erp/src.erp","from_erp":"cars/stock.erp","erp":"cars/slot.erp","resource":"eaid://car.wep"}]}"#,
    );
    assert!(apply(&mut ErpBuild::new(&original), &both, &addon_dir).is_err());
    let escape = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"copy-resource","source":"../game/cars/stock.erp","erp":"cars/slot.erp","resource":"eaid://stock-b.layer"}]}"#,
    );
    assert!(apply(&mut ErpBuild::new(&original), &escape, &addon_dir).is_err());
    let put = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"put-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car.wep","as":"eaid://slot-a.layer"},
            {"op":"put-resource","source":"erp/src.erp","erp":"cars/slot.erp","resource":"eaid://car-model.layer"}]}"#,
    );
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &put, &addon_dir).unwrap();
    let out = root.join("put");
    b.write(&out).unwrap();
    let written = Erp::parse(&std::fs::read(out.join("cars/slot.erp")).unwrap()).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["eaid://slot-a.layer", "eaid://car-model.layer"]);
    assert_eq!(
        written.resources[0].fragments[0].raw(),
        source.find("eaid://car.wep").unwrap().fragments[0].raw()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn replaces_adds_and_removes_resources_with_the_same_bytes_on_both_routes() {
    let root = std::env::temp_dir().join(format!("swofty-erpraw-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    let addon = root.join("addon");
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    let (x, y, z) = (
        "eaid://ui_package/x.tif.image",
        "eaid://ui_package/y.tif.image",
        "eaid://ui_package/z.tif.image",
    );
    let mut l = Erp::parse(&erp(&[("eaid://ui_package/l.layer", layer(&["a"]))]))
        .unwrap()
        .resources
        .remove(0);
    l.unk = 1u32.to_le_bytes().to_vec();
    let stock = archive(vec![
        surface(x, b"HEADER-X", b"PIXELS-X"),
        surface(y, b"HEADER-Y", b"PIXELS-Y"),
        l,
    ]);
    std::fs::write(game.join("ui/a.erp"), &stock).unwrap();
    std::fs::write(addon.join("raw/pixels.bin"), b"PAINTED!").unwrap();
    std::fs::write(addon.join("raw/head.bin"), b"HEADER-Z").unwrap();
    std::fs::write(addon.join("raw/z.bin"), b"NEW PIXELS").unwrap();
    let replace_add = r#"
            {"op":"replace-resource","erp":"ui/a.erp","resource":"eaid://ui_package/x.tif.image","fragment":1,"source":"raw/pixels.bin"},
            {"op":"add-resource","erp":"ui/a.erp","resource":"eaid://ui_package/z.tif.image","type":"GfxSurfaceRes",
             "sources":["raw/head.bin","raw/z.bin"]}"#;
    let ops = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{replace_add},
            {{"op":"remove-resource","erp":"ui/a.erp","resource":"eaid://ui_package/y.tif.image"}}]}}"#
    ));
    assert!(ops.iter().all(|op| unsupported(op).is_none()));
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &addon).unwrap();
    let out = root.join("build");
    assert_eq!(b.write(&out).unwrap(), ["ui/a.erp"]);
    let built = std::fs::read(out.join("ui/a.erp")).unwrap();

    let mut expected = Erp::parse(&stock).unwrap();
    expected
        .find_mut(x)
        .unwrap()
        .set_fragment(1, b"PAINTED!")
        .unwrap();
    let like = expected.find(x).unwrap().clone();
    expected.resources.retain(|r| r.name != y);
    expected
        .resources
        .push(Resource::like(z, &like, &[b"HEADER-Z".to_vec(), b"NEW PIXELS".to_vec()]).unwrap());
    assert_eq!(built, expected.to_bytes().unwrap());

    let written = Erp::parse(&built).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, [x, "eaid://ui_package/l.layer", z]);
    let before = Erp::parse(&stock).unwrap();
    let (old, new) = (before.find(x).unwrap(), written.find(x).unwrap());
    assert_eq!(new.fragments[0].raw(), old.fragments[0].raw());
    assert_eq!(new.fragments[1].raw(), Some(&b"PAINTED!"[..]));
    assert_eq!(new.fragments[1].compression, STORED);
    assert_eq!(new.hash, new.content_hash().unwrap());
    assert_ne!(new.hash, old.hash);
    let added = written.find(z).unwrap();
    assert_eq!((added.kind(), &added.unk), ("GfxSurfaceRes".into(), &old.unk));
    assert_eq!(data(added), [b"HEADER-Z".to_vec(), b"NEW PIXELS".to_vec()]);
    assert_eq!(
        added
            .fragments
            .iter()
            .map(|f| (f.name, f.flags, f.compression))
            .collect::<Vec<_>>(),
        old.fragments
            .iter()
            .map(|f| (f.name, f.flags, f.compression))
            .collect::<Vec<_>>()
    );
    assert_eq!(added.hash, added.content_hash().unwrap());

    let runtime_ops = erp_ops(&format!(r#"{{"schema_version":1,"ops":[{replace_add}]}}"#));
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &runtime_ops, &addon).unwrap();
    let overlay = root.join("overlay");
    let mut rt = Runtime::new(Vec::new());
    assert!(
        rt.resources(b.resources(), b.origins(), &[], &overlay)
            .unwrap()
            .is_empty()
    );
    let rt = rt.finish(&overlay).unwrap().unwrap();
    assert_eq!(rt.assets.resource_erps.len(), 1);
    assert_eq!(rt.assets.resource_erps[0].erps, ["ui/a.erp"]);
    let lay =
        Erp::parse(&std::fs::read(overlay.join("swofty").join(&rt.assets.resource_erps[0].file)).unwrap())
            .unwrap();
    let names: Vec<&str> = lay.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, [x, z]);
    for r in &lay.resources {
        let b = written.find(&r.name).unwrap();
        assert_eq!((r.kind, &r.unk, r.hash), (b.kind, &b.unk, b.hash));
        for (f, g) in r.fragments.iter().zip(&b.fragments) {
            assert_eq!(
                (f.name, f.size, f.flags, f.compression, f.packed(), f.raw()),
                (g.name, g.size, g.flags, g.compression, g.packed(), g.raw())
            );
        }
    }

    let fails = |json: &str| {
        let ops = erp_ops(&format!(r#"{{"schema_version":1,"ops":[{json}]}}"#));
        apply(&mut ErpBuild::new(&original), &ops, &addon)
            .unwrap_err()
            .to_string()
    };
    assert_eq!(
        fails(
            r#"{"op":"replace-resource","erp":"ui/a.erp","resource":"eaid://ui_package/w","source":"raw/head.bin"}"#
        ),
        "ui/a.erp: eaid://ui_package/w: not found"
    );
    assert_eq!(
        fails(&format!(
            r#"{{"op":"replace-resource","erp":"ui/a.erp","resource":"{x}","fragment":2,"source":"raw/head.bin"}}"#
        )),
        format!("ui/a.erp: {x}: no fragment 2")
    );
    assert!(
        fails(&format!(
            r#"{{"op":"replace-resource","erp":"ui/a.erp","resource":"{x}","source":"raw/missing.bin"}}"#
        ))
        .contains("missing.bin")
    );
    let in_game_copy = fails(&format!(
        r#"{{"op":"add-resource","erp":"ui/a.erp","resource":"{y}","type":"GfxSurfaceRes","sources":["raw/head.bin","raw/z.bin"]}}"#
    ));
    assert_eq!(
        in_game_copy,
        format!(
            "ui/a.erp in the game folder already has {y}, so something outside the launcher probably changed this file \
             (a hand-installed mod or a leftover). \
             Remove hand-installed mods, then Repair F1 25 in the EA app (Steam: Verify integrity of game files), then Play. \
             (Addon authors: if the stock file has it, use put-resource.)"
        )
    );
    assert!(
        fails(&format!(
            r#"{{"op":"add-resource","erp":"ui/a.erp","resource":"{z}","type":"GfxSurfaceRes","sources":["raw/head.bin"]}}"#
        ))
        .contains("takes its layout from a GfxSurfaceRes resource with 1 fragment(s)")
    );
    assert!(
        fails(&format!(
            r#"{{"op":"add-resource","erp":"ui/a.erp","resource":"{z}","type":"GfxMeshRes","sources":["raw/head.bin"]}}"#
        ))
        .contains("GfxMeshRes")
    );
    assert!(
        fails(&format!(
            r#"{{"op":"add-resource","erp":"ui/a.erp","resource":"{z}","type":"SixteenBytesLong","sources":["raw/head.bin"]}}"#
        ))
        .contains("is not a resource type")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_add_resource_and_a_put_resource_of_one_name_replace_in_load_order() {
    let root = std::env::temp_dir().join(format!("swofty-erpaddput-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    std::fs::create_dir_all(addon.join("erp")).unwrap();
    let (x, z, other) = (
        "eaid://ui_package/x.tif.image",
        "eaid://ui_package/z.tif.image",
        "eaid://ui_package/w.tif.image",
    );
    std::fs::write(
        game.join("ui/a.erp"),
        archive(vec![surface(x, b"HEADER-X", b"PIXELS-X")]),
    )
    .unwrap();
    std::fs::write(
        addon.join("erp/put.erp"),
        archive(vec![surface(z, b"HEADER-P", b"PIXELS-P")]),
    )
    .unwrap();
    std::fs::write(addon.join("raw/head.bin"), b"HEADER-A").unwrap();
    std::fs::write(addon.join("raw/pixels.bin"), b"PIXELS-A").unwrap();
    let put = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{{"op":"put-resource","source":"erp/put.erp","erp":"ui/a.erp","resource":"{z}"}}]}}"#
    ))
    .remove(0);
    let add = |name: &str| {
        erp_ops(&format!(
            r#"{{"schema_version":1,"ops":[{{"op":"add-resource","erp":"ui/a.erp","resource":"{name}","type":"GfxSurfaceRes","sources":["raw/head.bin","raw/pixels.bin"]}}]}}"#
        ))
        .remove(0)
    };
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let built = |b: ErpBuild, name: &str| {
        let out = root.join(name);
        b.write(&out).unwrap();
        let written = Erp::parse(&std::fs::read(out.join("ui/a.erp")).unwrap()).unwrap();
        written
            .resources
            .iter()
            .map(|r| (r.name.clone(), data(r)))
            .collect::<Vec<_>>()
    };
    let put_data = vec![b"HEADER-P".to_vec(), b"PIXELS-P".to_vec()];
    let add_data = vec![b"HEADER-A".to_vec(), b"PIXELS-A".to_vec()];

    let mut b = ErpBuild::new(&original);
    b.apply_for(&put, "first.addon", &resolve).unwrap();
    b.apply_for(&add(z), "second.addon", &resolve).unwrap();
    assert_eq!(
        b.take_notes(),
        [format!(
            "ui/a.erp: {z} was already written by first.addon; the add-resource from second.addon replaces it"
        )]
    );
    let resources = built(b, "put-then-add");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[1], (z.to_owned(), add_data.clone()));

    let mut b = ErpBuild::new(&original);
    b.apply_for(&add(z), "first.addon", &resolve).unwrap();
    b.apply_for(&put, "second.addon", &resolve).unwrap();
    assert!(b.take_notes().is_empty());
    let resources = built(b, "add-then-put");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[1], (z.to_owned(), put_data.clone()));

    let mut b = ErpBuild::new(&original);
    b.apply_for(&put, "first.addon", &resolve).unwrap();
    b.apply_for(&add(other), "second.addon", &resolve).unwrap();
    assert!(b.take_notes().is_empty());
    let resources = built(b, "unrelated");
    assert_eq!(resources.len(), 3);
    assert_eq!(resources[1], (z.to_owned(), put_data));
    assert_eq!(resources[2], (other.to_owned(), add_data));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn removing_a_resource_twice_is_idempotent() {
    let root = std::env::temp_dir().join(format!("swofty-erpremove-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        game.join("ui/a.erp"),
        erp(&[("eaid://one", layer(&["a"])), ("eaid://two", layer(&["b"]))]),
    )
    .unwrap();
    let ops = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"remove-resource","erp":"ui/a.erp","resource":"eaid://two"},
            {"op":"remove-resource","erp":"ui/a.erp","resource":"eaid://two"}]}"#,
    );
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &addon).unwrap();
    assert_eq!(b.take_notes(), ["ui/a.erp: eaid://two already removed"]);
    let out = root.join("out");
    assert_eq!(b.write(&out).unwrap(), ["ui/a.erp"]);
    let written = Erp::parse(&std::fs::read(out.join("ui/a.erp")).unwrap()).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["eaid://one"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn add_mipmaps_places_files_and_folders_at_game_paths() {
    let root = std::env::temp_dir().join(format!("swofty-erpmips-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let addon = root.join("addon");
    std::fs::create_dir_all(addon.join("mips/sub")).unwrap();
    std::fs::write(addon.join("mips/a.tif.mipmaps"), b"a").unwrap();
    std::fs::write(addon.join("mips/sub/b.tif.mipmaps"), b"b").unwrap();
    std::fs::write(addon.join("one.tif.mipmaps"), b"1").unwrap();
    let ops = erp_ops(
        r#"{"schema_version":1,"ops":[
            {"op":"add-mipmaps","source":"mips/","target":"livery_package/textures/"},
            {"op":"add-mipmaps","source":"one.tif.mipmaps","target":"livery_package/one.tif.mipmaps"}]}"#,
    );
    assert!(ops.iter().all(|op| unsupported(op).is_none()));
    let original = |rel: &str| root.join("game").join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &addon).unwrap();
    assert_eq!(b.changed().count(), 0);
    assert_eq!(
        b.take_mipmaps(),
        BTreeMap::from([
            (
                "livery_package/one.tif.mipmaps".to_owned(),
                addon.join("one.tif.mipmaps")
            ),
            (
                "livery_package/textures/a.tif.mipmaps".to_owned(),
                addon.join("mips/").join("a.tif.mipmaps")
            ),
            (
                "livery_package/textures/sub/b.tif.mipmaps".to_owned(),
                addon.join("mips/").join("sub/b.tif.mipmaps")
            ),
        ])
    );
    for bad in [
        r#"{"op":"add-mipmaps","source":"mips","target":"../outside"}"#,
        r#"{"op":"add-mipmaps","source":"missing","target":"x"}"#,
    ] {
        let ops = erp_ops(&format!(r#"{{"schema_version":1,"ops":[{bad}]}}"#));
        assert!(apply(&mut ErpBuild::new(&original), &ops, &addon).is_err());
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn kept_resources_stay_when_an_addon_replaces_them() {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-kept-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join(UI).parent().unwrap()).unwrap();
    std::fs::create_dir_all(&addon).unwrap();
    let view = LOGOS.strip_suffix(".image").unwrap();
    let other = "eaid://ui_package/art/textures/team_logo.image";
    let named = |name: &str, r: Resource| Resource {
        name: name.into(),
        ..r
    };
    let built = array(2, &[[0; 4], [1; 4], [7; 4], [8; 4]]);
    std::fs::write(
        game.join(UI),
        texture_erp(vec![
            built.clone(),
            named(view, array(2, &[[4; 4]])),
            named(other, array(2, &[[0; 4]])),
        ]),
    )
    .unwrap();
    std::fs::write(
        addon.join("pack.erp"),
        texture_erp(vec![
            array(2, &[[0; 4], [1; 4], [5; 4]]),
            named(view, array(2, &[[3; 4]])),
            named(other, array(2, &[[9; 4]])),
        ]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    b.keep(UI, LOGOS);
    b.keep(UI, view);
    assert!(b.keeps(UI, LOGOS) && !b.keeps(UI, other) && !b.keeps(BOOT, LOGOS));
    let overlay = ErpOp::Overlay {
        erps: vec![UI.into()],
        source: Source::addon("pack.erp"),
    };
    b.apply_for(&overlay, "pack", &resolve).unwrap();
    let put = ErpOp::PutResource {
        erp: UI.into(),
        resource: LOGOS.into(),
        source: Source::addon("pack.erp"),
        rename: None,
    };
    b.apply_for(&put, "later", &resolve).unwrap();
    let written: Vec<(String, Vec<u8>, String)> = b
        .resources()
        .map(|(_, r, _, o)| (r.name.clone(), slices_in(r), o.to_owned()))
        .collect();
    assert_eq!(written, [(other.to_owned(), vec![9], "pack".to_owned())]);
    let taken = b.take_held();
    let shown: Vec<(&str, &str, &str)> = taken
        .iter()
        .map(|h| (h.erp.as_str(), h.resource.as_str(), h.origin.as_str()))
        .collect();
    assert_eq!(
        shown,
        [(UI, LOGOS, "pack"), (UI, view, "pack"), (UI, LOGOS, "later")]
    );
    assert_eq!(b.origins(), ["pack"]);
    assert!(b.take_held().is_empty());
    std::fs::remove_dir_all(&root).ok();
}
