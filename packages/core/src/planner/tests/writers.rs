use super::*;

#[test]
fn records_conflicts_for_replacing_writes_only() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    let put = r#"{"op":"erp.put-resource","erp":"a.erp","resource":"r","source":"x.erp"}"#;
    let patch = r#"{"op":"erp.patch-pkg","erp":"a.erp","resource":"r","edits":[]}"#;
    p.push(op("a", put));
    p.push(op("b", patch));
    p.push(op("c", put));
    p.push(op("c", r#"{"op":"file.put","source":"f","target":"x/y.dds"}"#));
    p.push(op("d", r#"{"op":"file.put","source":"f","target":"x/y.dds"}"#));
    p.push(op(
        "a",
        r#"{"op":"lang.set","language":"*","key":"k","value":"v"}"#,
    ));
    p.push(op("a", r#"{"op":"warn","message":"m"}"#));
    assert_eq!(p.take_erp().len(), 4);
    let files = p.take_files();
    assert_eq!(files.len(), 2);
    assert!(
        matches!(&files[0].op, AssetOp::File(FileOp::Put { source, .. }) if *source == Source::addon("f"))
    );
    let planned = p.finish();
    assert_eq!(planned.waiting.len(), 1);
    assert_eq!(
        waiting_lines(&planned.waiting, &features),
        ["a: 1 lang op(s) wait for lang support in swofty_core"]
    );
    let shown: Vec<String> = planned.conflicts.iter().map(Conflict::to_string).collect();
    assert_eq!(shown, ["r in a.erp: c overrides a, b", "x/y.dds: d overrides c"]);
}

#[test]
fn the_last_writer_of_a_resource_or_its_archive_comes_last_in_load_order() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let put = r#"{"op":"erp.put-resource","erp":"a.erp","resource":"r","source":"x.erp"}"#;
    let file = |erp: &str| format!(r#"{{"op":"file.put","source":"f","target":"{erp}"}}"#);
    let both = |erp: &str| {
        [
            Target::Resource {
                erp: erp.into(),
                uri: "r".into(),
            },
            Target::File(erp.into()),
        ]
    };
    let mut p = Planner::new(&features, &opt_in);
    p.push(op("a", put));
    p.push(op("b", &file("b.erp")));
    p.push(op("c", put));
    assert_eq!(p.last_writer(&both("a.erp")), Some(&origin("c")));
    assert_eq!(p.last_writer(&both("b.erp")), Some(&origin("b")));
    assert_eq!(p.last_writer(&both("c.erp")), None);
    p.push(op("d", &file("a.erp")));
    assert_eq!(p.last_writer(&both("a.erp")), Some(&origin("d")));
    p.push(op("a", &file("b.erp")));
    assert_eq!(p.last_writer(&both("b.erp")), Some(&origin("b")));
}

#[test]
fn whole_resource_and_slice_writes_override_each_other_by_load_order() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let put = r#"{"op":"erp.put-resource","erp":"a.erp","resource":"r","source":"x.erp"}"#;
    let merge = r#"{"op":"erp.merge-slices","erp":"a.erp","resource":"r","source":"x.erp"}"#;
    let slice = |slice| Target::Slice {
        erp: "a.erp".into(),
        uri: "r".into(),
        slice,
    };
    let shown =
        |p: Planner| -> Vec<String> { p.finish().conflicts.iter().map(Conflict::to_string).collect() };

    let mut p = Planner::new(&features, &opt_in);
    p.push(op("a", merge));
    p.push(op("b", put));
    p.replaced(slice(1), &origin("a"));
    assert_eq!(shown(p), ["r in a.erp: b overrides a"]);

    let mut p = Planner::new(&features, &opt_in);
    p.push(op("a", put));
    p.push(op("b", merge));
    p.replaced(slice(1), &origin("b"));
    p.replaced(slice(2), &origin("b"));
    assert_eq!(
        shown(p),
        [
            "slice 1 of r in a.erp: b overrides a",
            "slice 2 of r in a.erp: b overrides a"
        ]
    );

    let mut p = Planner::new(&features, &opt_in);
    p.push(op("a", merge));
    p.replaced(slice(0), &origin("a"));
    p.push(op("b", put));
    p.push(op("c", merge));
    p.replaced(slice(0), &origin("c"));
    assert_eq!(
        shown(p),
        [
            "r in a.erp: b overrides a",
            "slice 0 of r in a.erp: c overrides b"
        ]
    );
}

#[test]
fn a_resource_removal_puts_every_erp_op_on_the_build_route() {
    let features = Features {
        stable: BTreeSet::from([Kind::Erp, Kind::Db]),
        experimental: BTreeSet::new(),
    };
    let none = BTreeSet::new();
    let mut p = Planner::new(&features, &none);
    let remove = |r: &str| format!(r#"{{"op":"erp.remove-resource","erp":"a.erp","resource":"{r}"}}"#);
    p.push(op(
        "a",
        r#"{"op":"erp.put-resource","erp":"a.erp","resource":"r","source":"x.erp"}"#,
    ));
    p.push(op(
        "a",
        r#"{"op":"erp.add-mipmaps","source":"m","target":"x/r.tif.mipmaps"}"#,
    ));
    assert_eq!(p.route(Kind::Erp), Route::Runtime);
    assert_eq!(p.route_note(Kind::Erp), None);
    p.push(op("b", &remove("r")));
    p.push(op("b", &remove("s")));
    p.push(op(
        "b",
        r#"{"op":"file.put","source":"f","target":"x/r.tif.mipmaps"}"#,
    ));
    assert_eq!(p.route(Kind::Erp), Route::Build);
    assert_eq!(p.route(Kind::Db), Route::Runtime);
    assert_eq!(
        p.route_note(Kind::Erp).as_deref(),
        Some("built into the game files; 2 resource removal(s) cannot apply at runtime")
    );
    assert_eq!(p.route_note(Kind::Db), None);
    assert_eq!(p.take_erp().len(), 4);
    let shown: Vec<String> = p.finish().conflicts.iter().map(Conflict::to_string).collect();
    assert_eq!(
        shown,
        ["r in a.erp: b overrides a", "x/r.tif.mipmaps: b overrides a"]
    );
    let mut p = Planner::new(&features, &none).build_only(&features.stable);
    p.push(op("b", &remove("r")));
    assert_eq!(p.route(Kind::Erp), Route::Build);
    assert_eq!(p.route_note(Kind::Erp), None);
}

fn mipmap_conflicts(files: &[(&str, &[&str])], ops: &[(&str, &str)]) -> Vec<String> {
    let features = Features {
        stable: BTreeSet::from([Kind::Erp, Kind::Db]),
        experimental: BTreeSet::new(),
    };
    let none = BTreeSet::new();
    let names = |_: &Origin, src: &Source| -> Vec<String> {
        files
            .iter()
            .find(|(path, _)| *path == src.path)
            .map(|(_, names)| names.iter().map(|n| (*n).to_owned()).collect())
            .unwrap_or_default()
    };
    let mut p = Planner::new(&features, &none).mipmaps(&names);
    for (addon, source) in ops {
        p.push(op(
            addon,
            &format!(
                r#"{{"op":"erp.add-mipmaps","source":"{source}","target":"environment_package/core_assets/objects/textures/main_front_end/names/"}}"#
            ),
        ));
    }
    p.finish().conflicts.iter().map(Conflict::to_string).collect()
}

#[test]
fn folder_mipmaps_with_different_names_do_not_overlap() {
    let shown = mipmap_conflicts(
        &[("m", &["s0_a.mipmaps", "s0_b.mipmaps"]), ("n", &["s1_a.mipmaps"])],
        &[("a", "m"), ("b", "n")],
    );
    assert!(shown.is_empty(), "{shown:?}");
}

#[test]
fn folder_mipmaps_with_the_same_name_overlap_once() {
    let shown = mipmap_conflicts(
        &[
            ("m", &["s0_a.mipmaps", "shared.mipmaps"]),
            ("n", &["s1_a.mipmaps", "shared.mipmaps"]),
        ],
        &[("a", "m"), ("b", "n")],
    );
    assert_eq!(
        shown,
        [
            "environment_package/core_assets/objects/textures/main_front_end/names/shared.mipmaps: b overrides a"
        ]
    );
}
