use super::*;

#[test]
fn merges_changed_slices_over_the_array_in_place_in_load_order() {
    let root = texture_game("order");
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    let mods: [(&str, Resource); 7] = [
        ("a", array(2, &[[0; 4], [9; 4], [2; 4], [3; 4]])),
        ("b", array(2, &[[0; 4], [7; 4], [2; 4], [8; 4]])),
        ("c", array(2, &[[0; 4], [1; 4], [2; 4], [3; 4]])),
        ("d", array(2, &[[6; 4], [1; 4]])),
        ("e", array(2, &[[0; 4], [1; 4], [5; 4], [3; 4], [7; 4]])),
        ("f", array(4, &[[4; 4], [4; 4]])),
        ("g", array(2, &[[0; 4], [1; 4], [2; 4], [4; 4]])),
    ];
    for (id, r) in &mods {
        std::fs::write(addon.join(format!("{id}.erp")), texture_erp(vec![r.clone()])).unwrap();
    }
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    let mut seen = Vec::new();
    let mut current = Vec::new();
    for (id, _) in &mods {
        b.apply_for(&merge_op(BOOT, &format!("{id}.erp")), id, &resolve)
            .unwrap();
        let [m] = b.take_merges().try_into().unwrap();
        seen.push((m.changed, m.whole, m.notes));
        let res = b
            .resources()
            .find(|(_, r, _, _)| r.name == LOGOS)
            .map(|(_, r, _, o)| (slices_in(r), o.to_owned()));
        current.push(res);
    }
    let none: Vec<String> = Vec::new();
    assert_eq!(seen[0], (vec![1], false, none.clone()));
    assert_eq!(seen[1], (vec![1, 3], false, none.clone()));
    assert_eq!(seen[2], (vec![], false, none.clone()));
    assert_eq!(seen[3], (vec![0], false, none.clone()));
    assert_eq!(
        seen[4],
        (
            vec![2],
            false,
            vec![format!(
                "{LOGOS} has 5 slices, the game's 4; the slices past 4 are left out"
            )]
        )
    );
    assert_eq!(
        seen[5],
        (
            vec![],
            true,
            vec![format!(
                "{LOGOS} differs in format or slice size from the game's array; it replaces the whole resource in {BOOT}"
            )]
        )
    );
    assert_eq!(
        seen[6],
        (
            vec![],
            false,
            vec![format!(
                "{LOGOS} in {BOOT} has another format or slice size, as f left it; these slices are not merged, so its 2 slices stay"
            )]
        )
    );
    let at = |i: usize| current[i].clone().unwrap();
    assert_eq!(at(0), (vec![0, 9, 2, 3], "a".to_owned()));
    assert_eq!(at(1), (vec![0, 7, 2, 8], "b".to_owned()));
    assert_eq!(at(2), (vec![0, 7, 2, 8], "b".to_owned()));
    assert_eq!(at(4), (vec![6, 7, 5, 8], "e".to_owned()));
    assert_eq!(at(5), (vec![4, 4], "f".to_owned()));
    assert_eq!(at(6), (vec![4, 4], "f".to_owned()));
    assert_eq!(b.origins(), ["a", "b", "d", "e", "f"]);
    assert!(b.resized().is_empty());
    let out = root.join("out");
    assert_eq!(b.write(&out).unwrap(), [BOOT]);
    let file = ErpFile::open(&out.join(BOOT)).unwrap();
    let names: Vec<&str> = file.erp.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["eaid://vfx_package/y.image", LOGOS]);
    let merged = file.load(file.erp.find(LOGOS).unwrap()).unwrap();
    assert_eq!(slices_in(&merged), [4, 4]);
    assert_eq!(
        merged.fragments[0].data().unwrap(),
        array(4, &[[0; 4]; 2]).fragments[0].data().unwrap()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_merge_into_the_stock_archive_keeps_its_layout_for_the_runtime_route() {
    let root = texture_game("runtime");
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        addon.join("a.erp"),
        texture_erp(vec![array(2, &[[0; 4], [1; 4], [9; 4], [3; 4]])]),
    )
    .unwrap();
    std::fs::write(addon.join("wide.erp"), texture_erp(vec![array(4, &[[5; 4]; 3])])).unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    b.apply_for(&merge_op(UI, "a.erp"), "a", &resolve).unwrap();
    assert!(b.resized().is_empty());
    let mut rt = Runtime::new(Vec::new());
    let out = root.join("overlay");
    let notes = rt.resources(b.resources(), &["a".into()], &[], &out).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let overlay = Erp::parse(&std::fs::read(out.join("swofty/assets/resources/0.erp")).unwrap()).unwrap();
    assert_eq!(slices_in(&overlay.resources[0]), [0, 1, 9, 3]);
    assert_eq!(rt.assets.resource_erps[0].erps, [UI]);

    b.apply_for(&merge_op(UI, "wide.erp"), "wide", &resolve).unwrap();
    let resized = b.resized();
    assert_eq!(resized.len(), 1);
    assert_eq!(resized[0].erp, UI);
    assert!(b.take_merges().iter().all(|m| m.slices == 4));
    std::fs::remove_dir_all(&root).ok();
}

// A team mod's array and view sit in the boot archive only, and an editor then
// changes one logo in every archive that carries the array.
#[test]
fn an_origin_whose_copies_differ_gets_an_overlay_for_each() {
    const FLOW: &str = "2025_asset_groups/ui_package/flow_race.erp";
    let root = texture_game("copies");
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    let view = LOGOS.strip_suffix(".image").unwrap();
    let stock = texture_erp(vec![array(2, &[[0; 4], [1; 4], [2; 4], [3; 4]]), view_res(4)]);
    for rel in [UI, FLOW] {
        std::fs::write(game.join(rel), &stock).unwrap();
    }
    std::fs::write(
        addon.join("mod.erp"),
        texture_erp(vec![array(2, &[[0; 4], [7; 4]])]),
    )
    .unwrap();
    std::fs::write(addon.join("view.erp"), texture_erp(vec![view_res(2)])).unwrap();
    std::fs::write(
        addon.join("grid.erp"),
        texture_erp(vec![array(2, &[[0; 4], [1; 4], [2; 4], [9; 4]])]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    b.apply_for(&merge_op(BOOT, "mod.erp"), "mod", &resolve).unwrap();
    let put = ErpOp::PutResource {
        erp: BOOT.into(),
        resource: view.into(),
        source: Source::addon("view.erp"),
        rename: None,
    };
    b.apply_for(&put, "mod", &resolve).unwrap();
    for rel in [UI, FLOW, BOOT] {
        b.apply_for(&merge_op(rel, "grid.erp"), "grid", &resolve).unwrap();
    }
    assert_eq!(
        b.cohere_arrays().unwrap(),
        [format!(
            "texture array {view}: view said 2 slices, image has 4; fixed"
        )]
    );
    let mut rt = Runtime::new(Vec::new());
    let out = root.join("overlay");
    let notes = rt.resources(b.resources(), b.origins(), &[], &out).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let mut seen = Vec::new();
    for e in &rt.assets.resource_erps {
        assert_eq!(e.origin, "grid");
        let overlay = Erp::parse(&std::fs::read(out.join("swofty").join(&e.file)).unwrap()).unwrap();
        let image = overlay.resources.iter().find(|r| r.name == LOGOS).unwrap();
        let said = overlay.resources.iter().find(|r| r.name == view).map(view_count);
        seen.push((e.erps.clone(), slices_in(image), said, overlay.resources.len()));
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            (vec![UI.to_owned(), FLOW.to_owned()], vec![0, 1, 2, 9], None, 1),
            (vec![BOOT.to_owned()], vec![0, 7, 2, 9], Some(4), 2),
        ]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_array_the_game_lacks_goes_in_whole() {
    let root = texture_game("missing");
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    let other = "eaid://ui_package/art/texture_arrays/new.texarray.image";
    std::fs::write(
        addon.join("a.erp"),
        texture_erp(vec![Resource {
            name: other.into(),
            ..array(2, &[[1; 4], [2; 4]])
        }]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    let op = ErpOp::MergeSlices {
        erp: BOOT.into(),
        resource: other.into(),
        source: Source::addon("a.erp"),
    };
    b.apply_for(&op, "a", &|s: &Source| Ok(s.resolve(&addon)?))
        .unwrap();
    let [m] = b.take_merges().try_into().unwrap();
    assert_eq!(
        (m.whole, m.slices, m.notes),
        (
            true,
            0,
            vec![format!(
                "the game has no {other}; it replaces the whole resource in {BOOT}"
            )]
        )
    );
    let added: Vec<Vec<u8>> = b.resources().map(|(_, r, _, _)| slices_in(r)).collect();
    assert_eq!(added, [vec![1, 2]]);
    let missing = merge_op(BOOT, "none.erp");
    assert!(
        b.apply_for(&missing, "a", &|s: &Source| Ok(s.resolve(&addon)?))
            .is_err()
    );
    std::fs::remove_dir_all(&root).ok();
}

fn grown_game(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = texture_game(name);
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        game.join(UI),
        texture_erp(vec![array(2, &[[0; 4], [1; 4], [2; 4], [3; 4]]), view_res(4)]),
    )
    .unwrap();
    std::fs::write(
        addon.join("grow.erp"),
        texture_erp(vec![array(2, &[[5; 4], [6; 4]])]),
    )
    .unwrap();
    (root, game, addon)
}

fn image_and_view(b: &ErpBuild) -> (Vec<u8>, usize, String, Option<u32>) {
    let view = LOGOS.strip_suffix(".image").unwrap();
    let (slices, count, origin) = b
        .resources()
        .find(|(_, r, _, _)| r.name == LOGOS)
        .map(|(_, r, _, o)| (slices_in(r), Array::read(r).unwrap().count, o.to_owned()))
        .unwrap();
    let said = b
        .resources()
        .find(|(_, r, _, _)| r.name == view)
        .map(|(_, r, _, _)| view_count(r));
    (slices, count, origin, said)
}

#[test]
fn a_merge_into_an_array_an_earlier_addon_grew_keeps_every_slice() {
    let (root, game, addon) = grown_game("grown");
    std::fs::write(
        addon.join("logo.erp"),
        texture_erp(vec![array(2, &[[0; 4], [9; 4], [2; 4], [3; 4]])]),
    )
    .unwrap();
    std::fs::write(
        addon.join("short.erp"),
        texture_erp(vec![array(2, &[[8; 4], [1; 4]])]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    let grow = ErpOp::AppendSlices {
        erp: UI.into(),
        resource: LOGOS.into(),
        source: Source::addon("grow.erp"),
    };
    b.apply_for(&grow, "a", &resolve).unwrap();
    let (slices, count, _, said) = image_and_view(&b);
    assert_eq!((slices, count, said), (vec![0, 1, 2, 3, 5, 6], 6, Some(6)));
    b.apply_for(&merge_op(UI, "logo.erp"), "b", &resolve).unwrap();
    let [m] = b.take_merges().try_into().unwrap();
    assert_eq!(
        (m.changed, m.whole, m.refused, m.notes),
        (vec![1], false, false, Vec::<String>::new())
    );
    assert_eq!(
        image_and_view(&b),
        (vec![0, 9, 2, 3, 5, 6], 6, "b".to_owned(), Some(6))
    );
    b.apply_for(&merge_op(UI, "short.erp"), "c", &resolve).unwrap();
    let [m] = b.take_merges().try_into().unwrap();
    assert_eq!(
        (m.changed, m.whole, m.refused, m.notes),
        (vec![0], false, false, Vec::<String>::new())
    );
    assert_eq!(
        image_and_view(&b),
        (vec![8, 9, 2, 3, 5, 6], 6, "c".to_owned(), Some(6))
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_merge_that_does_not_fit_a_grown_array_is_refused_and_leaves_it_whole() {
    let (root, game, addon) = grown_game("refused");
    std::fs::write(addon.join("wide.erp"), texture_erp(vec![array(4, &[[4; 4]; 6])])).unwrap();
    std::fs::write(
        addon.join("logo.erp"),
        texture_erp(vec![array(2, &[[0; 4], [9; 4], [2; 4], [3; 4]])]),
    )
    .unwrap();
    std::fs::write(addon.join("hires.erp"), texture_erp(vec![array(4, &[[7; 4]; 2])])).unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let none: Vec<usize> = Vec::new();

    let mut b = ErpBuild::new(&original);
    let put = ErpOp::PutResource {
        erp: UI.into(),
        resource: LOGOS.into(),
        source: Source::addon("wide.erp"),
        rename: None,
    };
    b.apply_for(&put, "wide", &resolve).unwrap();
    b.apply_for(&merge_op(UI, "logo.erp"), "grid", &resolve).unwrap();
    let [m] = b.take_merges().try_into().unwrap();
    assert_eq!(
        (m.changed, m.whole, m.refused, m.notes),
        (
            none.clone(),
            false,
            true,
            vec![format!(
                "{LOGOS} in {UI} has another format or slice size, as wide left it; these slices are not merged, so its 6 slices stay"
            )]
        )
    );
    assert_eq!(image_and_view(&b), (vec![4; 6], 6, "wide".to_owned(), None));
    assert_eq!(b.origins(), ["wide"]);

    let mut b = ErpBuild::new(&original);
    let grow = ErpOp::AppendSlices {
        erp: UI.into(),
        resource: LOGOS.into(),
        source: Source::addon("grow.erp"),
    };
    b.apply_for(&grow, "a", &resolve).unwrap();
    b.apply_for(&merge_op(UI, "hires.erp"), "hires", &resolve)
        .unwrap();
    let [m] = b.take_merges().try_into().unwrap();
    assert_eq!(
        (m.changed, m.whole, m.refused, m.notes),
        (
            none,
            false,
            true,
            vec![format!(
                "{LOGOS} differs in format or slice size from the game's array, and an earlier addon grew the array in {UI} to 6 slices; it is not merged, so the array keeps them"
            )]
        )
    );
    let (slices, count, _, said) = image_and_view(&b);
    assert_eq!((slices, count, said), (vec![0, 1, 2, 3, 5, 6], 6, Some(6)));
    std::fs::remove_dir_all(&root).ok();
}

fn zstd_shaped(mut r: Resource) -> Resource {
    for f in &mut r.fragments {
        let data = f.data().unwrap();
        *f = Fragment::new(f.name, data.len() as u64, f.flags, ZSTD, compress(&data).unwrap());
    }
    r.hash = r.content_hash().unwrap();
    r
}

#[test]
fn deferred_compression_writes_the_archives_an_eager_build_writes() {
    let (root, game, addon) = grown_game("deferred");
    let noisy = |seed: u8| -> [u8; 4] { [seed, seed.wrapping_mul(37), seed ^ 0x5a, 0xff - seed] };
    let stock: Vec<[u8; 4]> = (0..4).map(noisy).collect();
    std::fs::write(
        game.join(UI),
        texture_erp(vec![zstd_shaped(array(2, &stock)), zstd_shaped(view_res(4))]),
    )
    .unwrap();
    let mut grown = stock.clone();
    grown[1] = noisy(9);
    for (name, slices) in [
        ("grow.erp", vec![noisy(5), noisy(6)]),
        ("logo.erp", grown),
        ("short.erp", vec![noisy(8), stock[1]]),
    ] {
        std::fs::write(
            addon.join(name),
            texture_erp(vec![zstd_shaped(array(2, &slices))]),
        )
        .unwrap();
    }
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let grow = ErpOp::AppendSlices {
        erp: UI.into(),
        resource: LOGOS.into(),
        source: Source::addon("grow.erp"),
    };
    let built = |packer: Packer| {
        let mut b = ErpBuild::new(&original).with_packer(packer);
        b.apply_for(&grow, "a", &resolve).unwrap();
        b.apply_for(&merge_op(UI, "logo.erp"), "b", &resolve).unwrap();
        b.apply_for(&merge_op(UI, "short.erp"), "c", &resolve).unwrap();
        b
    };
    let seen = |b: &ErpBuild| {
        b.resources()
            .map(|(rel, r, _, origin)| (rel.to_owned(), r.name.clone(), r.hash, data(r), origin.to_owned()))
            .collect::<Vec<_>>()
    };
    let pending = |b: &ErpBuild| {
        b.resources()
            .flat_map(|(_, r, _, _)| &r.fragments)
            .filter(|f| f.pending().is_some())
            .count()
    };
    let (eager, deferred) = (built(Packer::default()), built(Packer::default().deferring(true)));
    let expected = seen(&eager);
    assert_eq!(seen(&deferred), expected);
    assert_eq!(image_and_view(&deferred).0, vec![8, 9, 2, 3, 5, 6]);
    assert_eq!((pending(&eager), pending(&deferred)), (0, 3));

    let cache = root.join("archives");
    let zstd = root.join("zstd");
    let out = |name: &str| root.join(name);
    eager.write(&out("eager")).unwrap();
    let (written, reused) = deferred
        .write_cached(&out("deferred"), Some(&cache), &|| Ok(()))
        .unwrap();
    assert_eq!((written, reused), (vec![UI.to_owned()], 0));
    let cached = built(Packer::cached(&zstd).deferring(true));
    assert_eq!(seen(&cached), expected);
    let (_, reused) = cached
        .write_cached(&out("cached"), Some(&cache), &|| Ok(()))
        .unwrap();
    assert_eq!(reused, 1);
    let cached_eager = built(Packer::cached(&zstd));
    cached_eager.write(&out("cached-eager")).unwrap();
    let bytes = std::fs::read(out("eager").join(UI)).unwrap();
    for name in ["deferred", "cached", "cached-eager"] {
        assert!(std::fs::read(out(name).join(UI)).unwrap() == bytes, "{name}");
    }
    let back = ErpFile::open(&out("deferred").join(UI)).unwrap();
    for r in &back.erp.resources {
        let r = back.load(r).unwrap();
        assert!(r.fragments.iter().all(|f| f.compression == ZSTD && f.is_zstd()));
        assert_eq!(r.hash, r.content_hash().unwrap());
    }
    std::fs::remove_dir_all(&root).ok();
}
