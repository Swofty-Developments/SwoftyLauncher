use super::*;

#[test]
fn archives_outside_the_asset_groups_are_written_whole() {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-outside-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon, out) = (root.join("game"), root.join("addon"), root.join("out"));
    let shader = "shader_package_2025/object_flag.nefx2.sm51.erp";
    let group = "2025_asset_groups/vfx_package/common.erp";
    std::fs::create_dir_all(game.join("shader_package_2025")).unwrap();
    std::fs::create_dir_all(game.join("2025_asset_groups/vfx_package")).unwrap();
    std::fs::create_dir_all(addon.join("mods")).unwrap();
    let body = |s: &str| layer(&[s]);
    std::fs::write(game.join(shader), erp(&[("eaid://shader", body("shader"))])).unwrap();
    std::fs::write(game.join(group), erp(&[("eaid://vfx", body("vfx"))])).unwrap();
    std::fs::write(addon.join("mods/car.erp"), erp(&[("eaid://car", body("car"))])).unwrap();
    let put = |to: &str| {
        format!(r#"{{"op":"put-resource","erp":"{to}","resource":"eaid://car","source":"mods/car.erp"}}"#)
    };
    let ops = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{},{}]}}"#,
        put(shader),
        put(group)
    ));
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops, &addon).unwrap();

    assert!(served_at_runtime(group) && !served_at_runtime(shader));
    assert_eq!(b.take_outside(&out).unwrap(), [shader]);
    let written = Erp::parse(&std::fs::read(out.join(shader)).unwrap()).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["eaid://shader", "eaid://car"]);
    let left: Vec<&str> = b.resources().map(|(rel, ..)| rel).collect();
    assert_eq!(left, [group]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unchanged_archive_is_linked_from_the_cache() {
    let root = std::env::temp_dir().join(format!("swofty-erpcache-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    let addon = root.join("addon");
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    let x = "eaid://ui_package/x.tif.image";
    std::fs::write(
        game.join("ui/a.erp"),
        archive(vec![surface(x, b"HEADER-X", b"PIXELS-X")]),
    )
    .unwrap();
    let put = |bytes: &[u8]| {
        std::fs::write(addon.join("raw/pixels.bin"), bytes).unwrap();
        erp_ops(
            r#"{"schema_version":1,"ops":[{"op":"replace-resource","erp":"ui/a.erp",
                "resource":"eaid://ui_package/x.tif.image","fragment":1,"source":"raw/pixels.bin"}]}"#,
        )
    };
    let original = |rel: &str| game.join(rel);
    let cache = root.join("cache");
    let build = |n: &str, bytes: &[u8]| {
        let ops = put(bytes);
        let mut b = ErpBuild::new(&original);
        apply(&mut b, &ops, &addon).unwrap();
        let out = root.join(n);
        let (written, reused) = b.write_cached(&out, Some(&cache), &|| Ok(())).unwrap();
        assert_eq!(written, ["ui/a.erp"]);
        (std::fs::read(out.join("ui/a.erp")).unwrap(), reused)
    };
    let (first, reused) = build("one", b"PAINTED!");
    assert_eq!(reused, 0);
    let (again, reused) = build("two", b"PAINTED!");
    assert_eq!(
        (again == first, reused),
        (true, 1),
        "the same build links the cached archive"
    );
    let (other, reused) = build("three", b"REPAINT!");
    assert_eq!(
        (other == first, reused),
        (false, 0),
        "different fragment bytes write a new archive"
    );
    let mut b = ErpBuild::new(&original);
    apply(&mut b, &put(b"PAINTED!"), &addon).unwrap();
    b.write(&root.join("plain")).unwrap();
    assert_eq!(
        std::fs::read(root.join("plain/ui/a.erp")).unwrap(),
        first,
        "the cache changes no bytes"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_same_size_handling_file_rewrites_its_archive_instead_of_the_runtime_overlay() {
    let root = std::env::temp_dir().join(format!("swofty-erphandling-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("car")).unwrap();
    std::fs::create_dir_all(addon.join("car")).unwrap();
    let vtf = "eaid://f1_2026_vehicle_package/teams/ferrari/handling/ferrari.vtf";
    let other = "eaid://f1_2026_vehicle_package/teams/ferrari/art/other.idf";
    std::fs::write(
        game.join("car/ferrari.erp"),
        typed_erp("VTF", &[(vtf, b"Mass 771".to_vec()), (other, b"stays".to_vec())]),
    )
    .unwrap();
    std::fs::write(addon.join("car/edited.vtf"), b"Mass 900").unwrap();
    std::fs::write(addon.join("car/edited.idf"), b"stayz").unwrap();
    let replace = |res: &str, src: &str| {
        format!(
            r#"{{"op":"replace-resource","erp":"car/ferrari.erp","resource":"{res}","source":"car/{src}"}}"#
        )
    };
    let ops = |list: &[String]| erp_ops(&format!(r#"{{"schema_version":1,"ops":[{}]}}"#, list.join(",")));
    let original = |rel: &str| game.join(rel);

    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops(&[replace(other, "edited.idf")]), &addon).unwrap();
    assert!(b.resized().is_empty());

    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops(&[replace(vtf, "edited.vtf")]), &addon).unwrap();
    let expected = vec![Resized {
        erp: "car/ferrari.erp".into(),
        resources: vec![(vtf.into(), HANDLING_WHY.into())],
    }];
    assert_eq!(b.resized(), expected);
    let out = root.join("out");
    assert_eq!(b.take_resized(&out).unwrap(), expected);
    assert_eq!(b.resources().count(), 0);
    let written = Erp::parse(&std::fs::read(out.join("car/ferrari.erp")).unwrap()).unwrap();
    assert_eq!(
        written.find(vtf).unwrap().fragments[0].data().unwrap(),
        b"Mass 900"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_resized_scene_rewrites_its_archive_instead_of_the_runtime_overlay() {
    let root = std::env::temp_dir().join(format!("swofty-erpscene-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("ui")).unwrap();
    let uri = "eaid://ui_package/art/screens/career/rnd_overview.idf";
    let other = "eaid://ui_package/art/screens/career/rnd_dept.idf";
    let four = scene(uri, &["tile_1", "tile_2", "tile_3", "tile_4"]);
    let five = scene(uri, &["tile_1", "tile_2", "tile_3", "tile_4", "tile_5"]);
    let stock = typed_erp(
        "UILayout",
        &[(uri, four.clone()), (other, scene(other, &["dept"]))],
    );
    std::fs::write(game.join("ui/hub.erp"), &stock).unwrap();
    let side = "eaid://ui_package/art/screens/career/rnd_side.idf";
    std::fs::write(
        game.join("ui/hq.erp"),
        typed_erp("UILayout", &[(side, scene(side, &["tile_a"]))]),
    )
    .unwrap();
    std::fs::write(addon.join("ui/side.idf"), scene(side, &["tile_b"])).unwrap();
    std::fs::write(addon.join("ui/same.idf"), &four).unwrap();
    std::fs::write(addon.join("ui/five.idf"), &five).unwrap();
    std::fs::write(addon.join("ui/dept.idf"), scene(other, &["dept", "dept_5"])).unwrap();
    std::fs::write(addon.join("ui/layer.bin"), layer(&["a"])).unwrap();
    std::fs::write(addon.join("ui/text.txt"), b"not a scene").unwrap();
    let replace = |erp: &str, res: &str, src: &str| {
        format!(r#"{{"op":"replace-resource","erp":"{erp}","resource":"{res}","source":"ui/{src}"}}"#)
    };
    let ops = |list: &[String]| erp_ops(&format!(r#"{{"schema_version":1,"ops":[{}]}}"#, list.join(",")));
    let original = |rel: &str| game.join(rel);

    let mut b = ErpBuild::new(&original);
    apply(&mut b, &ops(&[replace("ui/hub.erp", uri, "same.idf")]), &addon).unwrap();
    assert!(b.resized().is_empty());
    assert_eq!(b.resources().count(), 1);

    let mut b = ErpBuild::new(&original);
    apply(
        &mut b,
        &ops(&[
            replace("ui/hub.erp", uri, "five.idf"),
            replace("ui/hq.erp", side, "side.idf"),
        ]),
        &addon,
    )
    .unwrap();
    let why = format!(
        "fragment 0 is {} bytes, the stock one {}; stock archives that bind to the replacement read it with the stock layout",
        five.len(),
        four.len()
    );
    let expected = vec![Resized {
        erp: "ui/hub.erp".into(),
        resources: vec![(uri.into(), why)],
    }];
    assert_eq!(b.resized(), expected);
    let out = root.join("out");
    assert_eq!(b.take_resized(&out).unwrap(), expected);
    let left: Vec<(&str, &str)> = b.resources().map(|(e, r, _, _)| (e, r.name.as_str())).collect();
    assert_eq!(left, [("ui/hq.erp", side)]);
    assert_eq!(b.write(&root.join("rest")).unwrap(), ["ui/hq.erp"]);

    let built = std::fs::read(out.join("ui/hub.erp")).unwrap();
    let written = Erp::parse(&built).unwrap();
    let before = Erp::parse(&stock).unwrap();
    let res = written.find(uri).unwrap();
    assert_eq!(res.fragments[0].data().unwrap(), five);
    assert_eq!(res.fragments[0].size, five.len() as u64);
    assert_eq!(res.hash, res.content_hash().unwrap());
    assert_eq!(
        (res.kind, &res.unk),
        (before.find(uri).unwrap().kind, &before.find(uri).unwrap().unk)
    );
    assert_eq!(
        written.find(other).unwrap().fragments[0].raw,
        before.find(other).unwrap().fragments[0].raw
    );
    let mut again = Erp::parse(&built).unwrap();
    assert_eq!(again.to_bytes(), built);

    let fails = |src: &str, res: &str| {
        apply(
            &mut ErpBuild::new(&original),
            &ops(&[replace("ui/hub.erp", res, src)]),
            &addon,
        )
        .unwrap_err()
        .to_string()
    };
    assert_eq!(
        fails("dept.idf", uri),
        format!(
            "ui/hub.erp: {uri}: the source is scene {:#018x}, this resource is scene {:#018x}; it was saved from another .idf",
            component_id(other.strip_suffix(".idf").unwrap()),
            component_id(uri.strip_suffix(".idf").unwrap())
        )
    );
    for src in ["layer.bin", "text.txt"] {
        assert!(
            fails(src, uri)
                .ends_with("the source is not a scene: a UILayout resource holds a !pkg with a scenelist")
        );
    }
    apply(
        &mut ErpBuild::new(&original),
        &ops(&[replace("ui/hub.erp", other, "dept.idf")]),
        &addon,
    )
    .unwrap();
    std::fs::remove_dir_all(&root).ok();
}
