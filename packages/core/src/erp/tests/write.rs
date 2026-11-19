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
    assert_eq!(b.take_outside(&out, None).unwrap(), (vec![shader.to_owned()], 0));
    let written = Erp::parse(&std::fs::read(out.join(shader)).unwrap()).unwrap();
    let names: Vec<&str> = written.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["eaid://shader", "eaid://car"]);
    let left: Vec<&str> = b.resources().map(|(rel, ..)| rel).collect();
    assert_eq!(left, [group]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_runtime_route_build_links_the_archives_it_builds_into_the_game_files_unchanged() {
    use crate::erp::pack::frame_cache;
    let root = std::env::temp_dir().join(format!("swofty-erpcache-built-in-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    let shader = "shader_package_2025/object_flag.nefx2.sm51.erp";
    let group = "2025_asset_groups/ui_package/screens.erp";
    let (shader_res, group_res) = ("eaid://shader", "eaid://ui_package/screens/a.ui");
    let noise = |seed: u8, len: u32| -> Vec<u8> {
        (0..len)
            .map(|i| ((i % 251) as u8 ^ (i / 1021) as u8).wrapping_add(seed))
            .collect()
    };
    for (rel, name) in [(shader, shader_res), (group, group_res)] {
        let path = game.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            archive(vec![frame_cache::zstd_resource(
                name,
                "UIDF",
                &noise(1, 20 << 10),
            )]),
        )
        .unwrap();
    }
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    std::fs::write(addon.join("raw/shader.bin"), noise(2, 24 << 10)).unwrap();
    std::fs::write(addon.join("raw/screen.bin"), noise(3, 24 << 10)).unwrap();
    let replace = |erp: &str, res: &str, src: &str| {
        format!(
            r#"{{"op":"replace-resource","erp":"{erp}","resource":"{res}","fragment":0,"source":"raw/{src}"}}"#
        )
    };
    let ops = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{},{}]}}"#,
        replace(shader, shader_res, "shader.bin"),
        replace(group, group_res, "screen.bin")
    ));
    let original = |rel: &str| game.join(rel);
    let (zstd, cache) = (root.join("zstd"), root.join("archives"));
    let build = |n: &str, cache: Option<&Path>| {
        let mut b = ErpBuild::new(&original).with_packer(Packer::cached(&zstd).deferring(true));
        apply(&mut b, &ops, &addon).unwrap();
        let out = root.join(n);
        let (outside, outside_reused) = b.take_outside(&out, cache).unwrap();
        let (resized, resized_reused) = b.take_resized(&out, cache).unwrap();
        assert_eq!(outside, [shader]);
        assert_eq!(
            resized.iter().map(|r| r.erp.as_str()).collect::<Vec<_>>(),
            [group]
        );
        let bytes = [shader, group].map(|rel| std::fs::read(out.join(rel)).unwrap());
        (bytes, outside_reused + resized_reused)
    };
    let (first, reused) = build("one", Some(&cache));
    assert_eq!(reused, 0);
    assert_eq!(
        frame_cache::mark(&zstd),
        2,
        "the replaced fragments are in the frame cache"
    );
    let (again, reused) = build("two", Some(&cache));
    assert_eq!(reused, 2, "the same build links both archives");
    assert_eq!(again, first);
    assert_eq!(
        again.map(|a| frame_cache::marked_in(&a)),
        [0, 0],
        "a linked archive compresses nothing"
    );
    let (uncached, reused) = build("three", None);
    assert_eq!(reused, 0);
    assert_eq!(
        uncached.map(|a| frame_cache::marked_in(&a)),
        [1, 1],
        "without the archive cache it compresses again"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_archive_cache_entry_whose_bytes_changed_is_written_again_instead_of_linked() {
    use crate::cache::archive_entry::{change_a_byte_in_place, kept_archives};
    use crate::erp::pack::frame_cache;
    let root = std::env::temp_dir().join(format!("swofty-erpcache-changed-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    let shader = "shader_package_2025/object_flag.nefx2.sm51.erp";
    let res = "eaid://shader";
    let noise = |seed: u8, len: u32| -> Vec<u8> {
        (0..len)
            .map(|i| ((i % 251) as u8 ^ (i / 1021) as u8).wrapping_add(seed))
            .collect()
    };
    let path = game.join(shader);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        archive(vec![frame_cache::zstd_resource(res, "UIDF", &noise(1, 20 << 10))]),
    )
    .unwrap();
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    std::fs::write(addon.join("raw/shader.bin"), noise(2, 24 << 10)).unwrap();
    let ops = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{{"op":"replace-resource","erp":"{shader}","resource":"{res}","fragment":0,"source":"raw/shader.bin"}}]}}"#
    ));
    let original = |rel: &str| game.join(rel);
    let cache = root.join("archives");
    let build = |n: &str| {
        let mut b = ErpBuild::new(&original);
        apply(&mut b, &ops, &addon).unwrap();
        let out = root.join(n);
        let (outside, reused) = b.take_outside(&out, Some(&cache)).unwrap();
        assert_eq!(outside, [shader]);
        (std::fs::read(out.join(shader)).unwrap(), reused)
    };
    let (first, reused) = build("one");
    assert_eq!(reused, 0);
    let kept = kept_archives(&cache);
    assert_eq!(kept.len(), 1);
    change_a_byte_in_place(&kept[0]);
    let (again, reused) = build("two");
    assert_eq!(reused, 0, "the changed archive is not linked");
    assert_eq!(again, first);
    let (third, reused) = build("three");
    assert_eq!(reused, 1, "the archive written again is");
    assert_eq!(third, first);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_built_in_archives_and_the_runtime_overlay_compress_together() {
    let root = std::env::temp_dir().join(format!("swofty-erp-built-in-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    let shader = "shader_package_2025/object_flag.nefx2.sm51.erp";
    let resized = "2025_asset_groups/ui_package/screens.erp";
    let served = "2025_asset_groups/ui_package/persistent.erp";
    let res = "eaid://ui_package/screens/a.ui";
    let noise = |seed: u8, len: u32| -> Vec<u8> {
        (0..len)
            .map(|i| ((i % 251) as u8 ^ (i / 1021) as u8).wrapping_add(seed))
            .collect()
    };
    let stock = |r: &Resource| {
        let mut r = r.clone();
        for f in &mut r.fragments {
            let data = f.data().unwrap();
            *f = Fragment::new(f.name, data.len() as u64, f.flags, ZSTD, compress(&data).unwrap());
        }
        r.hash = r.content_hash().unwrap();
        r
    };
    let plain = Resource {
        name: res.into(),
        kind: erp::kind("UIDF").unwrap(),
        unk: vec![0; 4],
        fragments: vec![Fragment::new(*b"temp", 8 << 10, 0x10, STORED, noise(1, 8 << 10))],
        hash: [0; 16],
    };
    for rel in [shader, resized, served] {
        let path = game.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, archive(vec![stock(&plain)])).unwrap();
    }
    std::fs::create_dir_all(addon.join("raw")).unwrap();
    std::fs::write(addon.join("raw/shader.bin"), noise(2, 8 << 10)).unwrap();
    std::fs::write(addon.join("raw/longer.bin"), noise(3, 9 << 10)).unwrap();
    std::fs::write(addon.join("raw/same.bin"), noise(4, 8 << 10)).unwrap();
    let replace = |erp: &str, src: &str| {
        format!(
            r#"{{"op":"replace-resource","erp":"{erp}","resource":"{res}","fragment":0,"source":"raw/{src}"}}"#
        )
    };
    let ops = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{},{},{}]}}"#,
        replace(shader, "shader.bin"),
        replace(resized, "longer.bin"),
        replace(served, "same.bin")
    ));
    let original = |rel: &str| game.join(rel);
    let left_data = |b: &ErpBuild| -> Vec<(String, Vec<u8>)> {
        b.resources()
            .map(|(rel, r, _, _)| (rel.to_owned(), r.fragments[0].raw().unwrap().to_vec()))
            .collect()
    };

    let mut eager = ErpBuild::new(&original);
    apply(&mut eager, &ops, &addon).unwrap();
    let eager_out = root.join("eager");
    assert_eq!(eager.take_outside(&eager_out, None).unwrap().0, [shader]);
    assert_eq!(eager.take_resized(&eager_out, None).unwrap().0[0].erp, resized);
    let expected = left_data(&eager);

    let mut b = ErpBuild::new(&original).with_packer(Packer::default().deferring(true));
    apply(&mut b, &ops, &addon).unwrap();
    let out = root.join("together");
    let built = b.take_built_in(&out, None, &|| Ok(())).unwrap();
    assert_eq!(built.outside, [shader]);
    assert_eq!(
        built.resized.iter().map(|r| r.erp.as_str()).collect::<Vec<_>>(),
        [resized]
    );
    assert_eq!(built.reused, 0);
    assert_eq!(
        b.settle().unwrap(),
        0,
        "the runtime overlay's fragments were compressed with the archives"
    );
    assert_eq!(left_data(&b), expected);
    assert_eq!(
        expected.iter().map(|(rel, _)| rel.as_str()).collect::<Vec<_>>(),
        [served]
    );
    for rel in [shader, resized] {
        assert_eq!(
            std::fs::read(out.join(rel)).unwrap(),
            std::fs::read(eager_out.join(rel)).unwrap(),
            "{rel}"
        );
    }
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
fn a_linked_archive_keeps_the_time_it_was_written() {
    let root = std::env::temp_dir().join(format!("swofty-erpcache-time-{}", std::process::id()));
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
    std::fs::write(addon.join("raw/pixels.bin"), b"PAINTED!").unwrap();
    let ops = erp_ops(
        r#"{"schema_version":1,"ops":[{"op":"replace-resource","erp":"ui/a.erp",
            "resource":"eaid://ui_package/x.tif.image","fragment":1,"source":"raw/pixels.bin"}]}"#,
    );
    let original = |rel: &str| game.join(rel);
    let cache = root.join("cache");
    let build = |n: &str| {
        let mut b = ErpBuild::new(&original);
        apply(&mut b, &ops, &addon).unwrap();
        let out = root.join(n);
        let (_, reused) = b.write_cached(&out, Some(&cache), &|| Ok(())).unwrap();
        (out.join("ui/a.erp"), reused)
    };
    let (first, _) = build("one");
    let written = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&first)
        .unwrap()
        .set_modified(written)
        .unwrap();
    let (second, reused) = build("two");
    assert_eq!(reused, 1);
    assert_eq!(
        std::fs::metadata(&second).unwrap().modified().unwrap(),
        written,
        "linking the cached archive changes its write time"
    );
    let used: Vec<std::time::SystemTime> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .flat_map(|sub| std::fs::read_dir(sub.path()).unwrap().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "used"))
        .map(|p| std::fs::metadata(p).unwrap().modified().unwrap())
        .collect();
    assert_eq!(used.len(), 1, "the link marks the entry used");
    assert!(used[0] > written);
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
    assert_eq!(b.take_resized(&out, None).unwrap(), (expected, 0));
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
    assert_eq!(b.take_resized(&out, None).unwrap(), (expected, 0));
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
        written.find(other).unwrap().fragments[0].raw(),
        before.find(other).unwrap().fragments[0].raw()
    );
    let mut again = Erp::parse(&built).unwrap();
    assert_eq!(again.to_bytes().unwrap(), built);

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
