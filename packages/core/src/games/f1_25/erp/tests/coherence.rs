use super::*;

type Coherence = (PathBuf, Vec<String>, Vec<(String, u32, String)>, Vec<String>);

fn coherence_build(name: &str, adds: &[(&str, Resource)]) -> Coherence {
    let root = texture_game(name);
    let (game, addon) = (root.join("game"), root.join("addon"));
    let view = LOGOS.strip_suffix(".image").unwrap();
    std::fs::create_dir_all(&addon).unwrap();
    let stock = game.join(UI);
    std::fs::write(&stock, texture_erp(vec![array(2, &[[0; 4]; 4]), view_res(4)])).unwrap();
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    for (origin, res) in adds {
        let file = format!("{origin}-{}.erp", res.name.len());
        std::fs::write(addon.join(&file), texture_erp(vec![res.clone()])).unwrap();
        let op = ErpOp::PutResource {
            erp: UI.into(),
            resource: res.name.clone(),
            source: Source::addon(&file),
            rename: None,
        };
        b.apply_for(&op, origin, &|s: &Source| Ok(s.resolve(&addon)?))
            .unwrap();
    }
    let lines = b.cohere_arrays().unwrap();
    let counts = b
        .resources()
        .filter(|(_, r, _, _)| r.name == view)
        .map(|(_, r, _, o)| (r.name.clone(), view_count(r), o.to_owned()))
        .collect();
    let origins = b.origins().to_vec();
    (root, lines, counts, origins)
}

#[test]
fn a_view_from_another_addon_follows_the_image_slice_count() {
    let (root, lines, views, origins) = coherence_build(
        "cohere-split",
        &[("cs", view_res(16)), ("names", array(2, &[[1; 4]; 6]))],
    );
    let view = LOGOS.strip_suffix(".image").unwrap();
    assert_eq!(
        lines,
        [format!(
            "texture array {view}: view said 16 slices, image has 6; fixed"
        )]
    );
    assert_eq!(views, [(view.to_owned(), 6, "names".to_owned())]);
    assert_eq!(origins, ["cs", "names"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_stock_view_follows_an_addon_image_and_a_coherent_array_is_untouched() {
    let (root, lines, views, _) = coherence_build("cohere-stock", &[("names", array(2, &[[1; 4]; 6]))]);
    assert_eq!(lines.len(), 1);
    assert_eq!(views[0].1, 6);
    std::fs::remove_dir_all(&root).ok();
    let (root, lines, views, _) = coherence_build(
        "cohere-ok",
        &[("cs", array(2, &[[1; 4]; 6])), ("cs", view_res(6))],
    );
    assert!(lines.is_empty());
    assert_eq!(views[0].1, 6);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_view_that_names_no_slice_count_is_left_alone() {
    let root = texture_game("cohere-zero");
    let game = root.join("game");
    std::fs::write(
        game.join(UI),
        texture_erp(vec![array(2, &[[1; 4]; 1]), view_res(0)]),
    )
    .unwrap();
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    b.load(UI).unwrap();
    assert!(b.cohere_arrays().unwrap().is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_whole_archive_put_with_a_wider_view_than_its_image_is_repaired() {
    let root = texture_game("cohere-put");
    let game = root.join("game");
    let view = LOGOS.strip_suffix(".image").unwrap();
    let stock = game.join(UI);
    std::fs::write(&stock, texture_erp(vec![array(2, &[[1; 4]; 6]), view_res(16)])).unwrap();
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original);
    b.load(UI).unwrap();
    let lines = b.cohere_arrays().unwrap();
    assert_eq!(
        lines,
        [format!(
            "texture array {view}: view said 16 slices, image has 6; fixed"
        )]
    );
    let counts: Vec<u32> = b
        .resources()
        .filter(|(_, r, _, _)| r.name == view)
        .map(|(_, r, _, _)| view_count(r))
        .collect();
    assert_eq!(counts, [6]);
    let mut again = ErpBuild::new(&original);
    again.load(UI).unwrap();
    std::fs::write(&stock, texture_erp(vec![array(2, &[[1; 4]; 6]), view_res(6)])).unwrap();
    let mut ok = ErpBuild::new(&original);
    ok.load(UI).unwrap();
    assert!(ok.cohere_arrays().unwrap().is_empty());
    std::fs::remove_dir_all(&root).ok();
}
