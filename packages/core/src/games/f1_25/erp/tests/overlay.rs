use super::*;

#[test]
fn a_ready_overlay_whose_view_disagrees_with_the_stock_image_goes_in_resource_by_resource() {
    let root = texture_game("cohere-overlay");
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        game.join(UI),
        texture_erp(vec![array(2, &[[1; 4]; 6]), view_res(6)]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    for (count, fits) in [(16, false), (6, true), (0, true)] {
        let path = addon.join(format!("v{count}.erp"));
        std::fs::write(&path, texture_erp(vec![view_res(count)])).unwrap();
        let got = crate::content::runtime_assets::overlay_fits(&path, &[UI.into()], &original).unwrap();
        assert_eq!(got.is_ok(), fits, "{count}: {got:?}");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_ready_overlay_loads_as_it_is_or_goes_in_resource_by_resource() {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-ready-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("mods")).unwrap();
    let body = |s: &str| layer(&[s]);
    std::fs::write(game.join("ui/a.erp"), erp(&[("eaid://stock", body("stock"))])).unwrap();
    std::fs::write(addon.join("mods/a.erp"), erp(&[("eaid://one", body("a1"))])).unwrap();
    std::fs::write(
        addon.join("mods/ready.erp"),
        erp(&[("eaid://one", body("c1")), ("eaid://two", body("c2"))]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let put = &erp_ops(
        r#"{"schema_version":1,"ops":[{"op":"put-resource","erp":"ui/a.erp","resource":"eaid://one","source":"mods/a.erp"}]}"#,
    )[0];
    let ready = &erp_ops(
        r#"{"schema_version":1,"ops":[{"op":"overlay","erps":["ui/a.erp"],"source":"mods/ready.erp"}]}"#,
    )[0];
    let path = addon.join("mods/ready.erp");
    assert_eq!(
        crate::content::runtime_assets::overlay_fits(&path, &["ui/a.erp".into()], &original).unwrap(),
        Ok(2)
    );

    // Loaded as it is: listed before the earlier origin's overlay, so it wins.
    let mut b = ErpBuild::new(&original);
    b.apply_for(put, "mods (a)", &resolve).unwrap();
    let prebuilt = [crate::content::runtime_assets::Prebuilt {
        origin: "mods (c)".into(),
        path: path.clone(),
        erps: vec!["ui/a.erp".into()],
        resources: 2,
    }];
    let overlay = root.join("overlay");
    let mut rt = Runtime::new(Vec::new());
    let ranks = ["mods (a)".to_owned(), "mods (c)".to_owned()];
    rt.resources(b.resources(), &ranks, &prebuilt, &overlay).unwrap();
    let listed: Vec<(&str, &str)> = rt
        .assets
        .resource_erps
        .iter()
        .map(|e| (e.file.as_str(), e.origin.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("assets/resources/p0.erp", "mods (c)"),
            ("assets/resources/0.erp", "mods (a)")
        ]
    );
    assert_eq!(rt.files["swofty/assets/resources/p0.erp"], path);

    // Off the runtime route it is a put-resource of each of its resources.
    let mut b = ErpBuild::new(&original);
    b.apply_for(put, "mods (a)", &resolve).unwrap();
    b.apply_for(ready, "mods (c)", &resolve).unwrap();
    let mut left: Vec<(String, String)> = b
        .resources()
        .map(|(_, r, _, origin)| (r.name.clone(), origin.to_owned()))
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            ("eaid://one".to_owned(), "mods (c)".to_owned()),
            ("eaid://two".to_owned(), "mods (c)".to_owned())
        ]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn each_origin_gets_an_overlay_archive_of_its_own() {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-origins-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::create_dir_all(addon.join("mods")).unwrap();
    let body = |s: &str| layer(&[s]);
    std::fs::write(game.join("ui/a.erp"), erp(&[("eaid://stock", body("stock"))])).unwrap();
    std::fs::write(game.join("ui/boot.erp"), erp(&[("eaid://boot", body("boot"))])).unwrap();
    std::fs::write(
        addon.join("mods/a.erp"),
        erp(&[("eaid://one", body("a1")), ("eaid://shared", body("from a"))]),
    )
    .unwrap();
    std::fs::write(
        addon.join("mods/b.erp"),
        erp(&[("eaid://two", body("b2")), ("eaid://shared", body("from b"))]),
    )
    .unwrap();
    let put = |res: &str, from: &str| {
        format!(r#"{{"op":"put-resource","erp":"ui/a.erp","resource":"{res}","source":"mods/{from}.erp"}}"#)
    };
    let ops = |list: &[String]| erp_ops(&format!(r#"{{"schema_version":1,"ops":[{}]}}"#, list.join(",")));
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    for op in ops(&[put("eaid://one", "a"), put("eaid://shared", "a")]) {
        b.apply_for(&op, "mods (a)", &resolve).unwrap();
    }
    let to_boot =
        r#"{"op":"put-resource","erp":"ui/boot.erp","resource":"eaid://three","source":"mods/b3.erp"}"#;
    std::fs::write(addon.join("mods/b3.erp"), erp(&[("eaid://three", body("b3"))])).unwrap();
    for op in ops(&[
        put("eaid://two", "b"),
        put("eaid://shared", "b"),
        to_boot.to_owned(),
    ]) {
        b.apply_for(&op, "mods (b)", &resolve).unwrap();
    }
    assert_eq!(b.origins(), ["mods (a)", "mods (b)"]);
    let overlay = root.join("overlay");
    let mut rt = Runtime::new(Vec::new());
    assert!(
        rt.resources(b.resources(), b.origins(), &[], &overlay)
            .unwrap()
            .is_empty()
    );
    let rt = rt.finish(&overlay).unwrap().unwrap();
    let erps: Vec<(&str, &str, Vec<String>)> = rt
        .assets
        .resource_erps
        .iter()
        .map(|e| {
            let file = Erp::parse(&std::fs::read(overlay.join("swofty").join(&e.file)).unwrap()).unwrap();
            let names = file.resources.iter().map(|r| r.name.clone()).collect();
            (e.file.as_str(), e.origin.as_str(), names)
        })
        .collect();
    // The game reads the first listed overlay first, so the later origin comes first.
    assert_eq!(
        erps,
        [
            (
                "assets/resources/1.erp",
                "mods (b)",
                vec![
                    "eaid://shared".to_owned(),
                    "eaid://three".to_owned(),
                    "eaid://two".to_owned()
                ]
            ),
            (
                "assets/resources/0.erp",
                "mods (a)",
                vec!["eaid://one".to_owned()]
            ),
        ]
    );
    let targets: Vec<&Vec<String>> = rt.assets.resource_erps.iter().map(|e| &e.erps).collect();
    assert_eq!(
        targets,
        [
            &vec!["ui/a.erp".to_owned(), "ui/boot.erp".to_owned()],
            &vec!["ui/a.erp".to_owned()]
        ]
    );
    std::fs::remove_dir_all(&root).ok();
}
