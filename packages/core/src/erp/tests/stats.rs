use super::*;

#[test]
fn the_build_rewrites_stale_tsmd_stats_of_a_replaced_bxml_fragment() {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-tsmd-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    std::fs::create_dir_all(game.join("tracks")).unwrap();
    std::fs::create_dir_all(addon.join("ai")).unwrap();
    const SPLINE: &str = "eaid://environment_package/tracks/bahrain/ai/bahrain_ai_track.aispline";
    let stock = spline("ai_gate_track");
    let fragment = |raw: Vec<u8>| Fragment::new(*b"temp", raw.len() as u64, 0, STORED, raw);
    let mut res = Resource {
        name: SPLINE.into(),
        kind: erp::kind("AISplineData").unwrap(),
        unk: vec![0; 6],
        fragments: vec![fragment(stock.to_bytes()), fragment(stock.string_stats(1))],
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&4u32.to_le_bytes());
    let mut archive = Erp {
        header,
        version: 4,
        resources: vec![res],
    };
    std::fs::write(
        game.join("tracks/bahrain_common.erp"),
        archive.to_bytes().unwrap(),
    )
    .unwrap();
    assert!(
        stale_stats_in(&ErpFile::open(&game.join("tracks/bahrain_common.erp")).unwrap())
            .unwrap()
            .is_empty()
    );
    let edited = spline("ai_gate_track_edited_by_hand");
    let mut cut = edited.to_bytes();
    cut.truncate(cut.len() - 3);
    std::fs::write(addon.join("ai/stock.aispline"), stock.to_bytes()).unwrap();
    std::fs::write(addon.join("ai/line.aispline"), edited.to_bytes()).unwrap();
    std::fs::write(addon.join("ai/line.tsmd"), edited.string_stats(1)).unwrap();
    std::fs::write(addon.join("ai/cut.aispline"), cut).unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let replace = |fragment: usize, source: &str| {
        erp_ops(&format!(
            r#"{{"schema_version":1,"ops":[{{"op":"replace-resource","erp":"tracks/bahrain_common.erp","resource":"{SPLINE}","fragment":{fragment},"source":"{source}"}}]}}"#
        ))
        .remove(0)
    };
    let build = |ops: &[(usize, &str)]| {
        let mut b = ErpBuild::new(&original);
        for (fragment, source) in ops {
            b.apply_for(&replace(*fragment, source), "kizi.ai", &resolve)
                .unwrap();
        }
        let repair = b.repair_stats();
        let stats = b
            .resources()
            .find(|(_, r, _, _)| r.name == SPLINE)
            .map(|(_, r, _, _)| r.fragments[1].data().unwrap());
        (repair, stats, b.repair_stats())
    };
    let at = vec![("kizi.ai".to_owned(), SPLINE.to_owned())];

    let (repair, stats, again) = build(&[(0, "ai/line.aispline")]);
    assert_eq!(repair.repaired, at);
    assert_eq!(stats.unwrap(), edited.string_stats(1));
    assert_eq!(again, StatsRepair::default());

    let (repair, stats, _) = build(&[(0, "ai/stock.aispline")]);
    assert_eq!(repair, StatsRepair::default());
    assert_eq!(stats.unwrap(), stock.string_stats(1));

    let (repair, _, _) = build(&[(0, "ai/line.aispline"), (1, "ai/line.tsmd")]);
    assert_eq!(repair, StatsRepair::default());

    let (repair, stats, _) = build(&[(1, "ai/line.tsmd")]);
    assert_eq!(repair.repaired, at);
    assert_eq!(stats.unwrap(), stock.string_stats(1));

    let (repair, _, _) = build(&[(0, "ai/cut.aispline")]);
    assert_eq!(repair.broken, at);

    archive.resources[0].set_fragment(0, &edited.to_bytes()).unwrap();
    std::fs::write(root.join("broken.erp"), archive.to_bytes().unwrap()).unwrap();
    let file = ErpFile::open(&root.join("broken.erp")).unwrap();
    assert_eq!(stale_stats_in(&file).unwrap(), [SPLINE]);
    let (fixed, broken) = repair_stats_in(&file, &swofty_formats::erp::Packer::default()).unwrap();
    assert_eq!((fixed.len(), fixed[0].0, broken.len()), (1, 0, 0));
    assert_eq!(fixed[0].1.fragments[1].data().unwrap(), edited.string_stats(1));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_stats_notes_name_the_files_and_the_fix() {
    let spline = |t: &str| format!("eaid://environment_package/tracks/{t}/ai/{t}_ai_track.aispline");
    let lines: Vec<String> = (0..20).map(|i| spline(&format!("t{i}"))).collect();
    assert_eq!(
        repaired_note("More AI", &lines),
        "fixed the size headers of 20 AI racing line(s) in More AI so F1 25 reads them safely"
    );
    let quadtree = "eaid://t/x.trackspacespline?context=quadtree".to_owned();
    assert_eq!(
        repaired_note("Sepang", &[quadtree.clone(), spline("x")]),
        "fixed the size headers of 2 file(s) in Sepang so F1 25 reads them safely"
    );
    assert_eq!(
        unrepairable_message(
            "More AI",
            &[spline("bahrain")],
            "Turn More AI off in this profile."
        ),
        "More AI would crash F1 25: its bahrain_ai_track.aispline doesn't match the size header the game \
         reads with it, and the launcher can't read the file to fix the header. Turn More AI off in this profile."
    );
    assert_eq!(
        unrepairable_message("Sepang", &[quadtree, spline("x")], "Fix."),
        "Sepang would crash F1 25: its x.trackspacespline and 1 more file(s) don't match the size headers the \
         game reads with them, and the launcher can't read the files to fix the headers. Fix."
    );
}
