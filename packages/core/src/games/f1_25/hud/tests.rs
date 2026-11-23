use super::composer::*;
use super::inputs::*;
use super::model::{Dial, HudElement};
use super::stock::*;
use super::*;
use crate::Result;
use crate::ui::node::{NodeExt, ObjExt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use swofty_formats::erp::{Erp, Packer, Resource};
use swofty_formats::pkg::Pkg;

/// A stock that lays every resource out with zstd fragments.
struct ZstdLayout;

impl Stock for ZstdLayout {
    fn read(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        Err(err(format!("no stock {resource} in {erp}")))
    }

    fn layout(&self, name: &str, kind: &str, fragments: usize) -> Result<Resource> {
        let mut r = crate::erp::pack::frame_cache::zstd_resource(name, kind, b"");
        r.fragments = vec![r.fragments[0].clone(); fragments];
        Ok(r)
    }

    fn header(&self) -> Result<([u8; swofty_formats::erp::HEADER_LEN], u32)> {
        let mut header = [0u8; swofty_formats::erp::HEADER_LEN];
        header[..4].copy_from_slice(swofty_formats::erp::MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        Ok((header, 3))
    }

    fn carriers(&self, _: &[&str]) -> Result<BTreeMap<String, Vec<String>>> {
        Ok(BTreeMap::new())
    }
}

#[test]
fn a_second_hud_archive_takes_its_frames_from_the_cache() {
    use crate::erp::pack::frame_cache;
    let scene: Vec<u8> = (0..24u32 << 10)
        .map(|i| (i % 251) as u8 ^ (i / 1021) as u8)
        .collect();
    let resources = || {
        vec![(
            "eaid://ui_package/hud/a.uidf".to_owned(),
            "UIDF",
            vec![scene.clone()],
        )]
    };
    let dir = std::env::temp_dir().join(format!("swofty-hud-frames-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let packer = Packer::cached(&dir);
    let first = archive(&ZstdLayout, resources(), &packer).unwrap();
    assert_eq!(frame_cache::marked_in(&first), 0);
    assert_eq!(frame_cache::mark(&dir), 1);
    let second = archive(&ZstdLayout, resources(), &packer).unwrap();
    assert_eq!(frame_cache::marked_in(&second), 1);
    let erp = Erp::parse(&second).unwrap();
    assert_eq!(erp.resources[0].fragments[0].data().unwrap(), scene);
    std::fs::remove_dir_all(&dir).ok();
}

fn game() -> Option<GameStock> {
    let Some(dir) = std::env::var_os("F1_25_DIR") else {
        eprintln!("F1_25_DIR is not set; skipping");
        return None;
    };
    let game = PathBuf::from(dir);
    let data = crate::store::Store::open_default().root;
    Some(GameStock::with_baseline(&game, &data).unwrap_or_else(|_| GameStock::new(game)))
}

fn png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = rgba.iter().copied().cycle().take((w * h * 4) as usize).collect();
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    std::fs::write(path, out).unwrap();
}

fn addon_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-hud-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("art")).unwrap();
    for (n, w, h) in [
        ("base", 64, 28),
        ("lit", 16, 8),
        ("fill", 16, 8),
        ("mask", 32, 32),
        ("solid", 8, 8),
    ] {
        png(&dir.join(format!("art/{n}.png")), w, h, [255, 255, 255, 255]);
    }
    dir
}

fn record() -> serde_json::Value {
    serde_json::json!({
        "dial": "f1_26",
        "sprites": {
            "base": { "png": "art/base.png", "bounds": [-60, -20, 60, 20] },
            "lit": { "png": "art/lit.png", "bounds": [-50, -10, -34, -2] },
            "fill": { "png": "art/fill.png", "bounds": [-20, 8, -4, 16] },
            "mask": { "png": "art/mask.png", "bounds": [10, -30, 42, 2] },
            "solid": { "png": "art/solid.png", "bounds": [-4, -4, 4, 4] }
        },
        "elements": [
            { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
            { "kind": "group", "id": "std", "visible": { "view_model": "view_model_data.hud_t_std", "default": true }, "children": [
                { "kind": "sprite", "id": "boost", "sprite": "lit", "visible": { "view_model": "view_model_data.hud_t_boost" } },
                { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 40, "cap_height": 9.5, "at": [0, -6], "value": { "game": "speed_str" } },
                { "kind": "text", "id": "pos", "style": "17_bold", "align": "right", "box": 14, "cap_height": 6, "at": [-40, 12], "value": { "session": "current_car.position" } },
                { "kind": "levels", "id": "dp", "sprite": "lit", "count": 4, "show": "view_model_data.hud_t_dp{nn}" },
                { "kind": "slats", "id": "th", "sprites": ["lit", "lit"], "show": "view_model_data.hud_t_th{nn}" },
                { "kind": "gauge", "id": "batt", "sprite": "fill", "path": [[-20, 12], [-4, 12]], "value": { "game": "ers_stored" } },
                { "kind": "sweep", "id": "rpm", "mask_sprite": "mask", "fill_sprite": "solid", "pivot": [0, 0], "start": 225, "arc": [[0, 0], [1, -270]], "value": { "game": "revs" }, "tint": { "session": "current_car.nub_colour" } }
            ] }
        ],
        "editor": { "preview": { "game.revs": 0.5, "game.speed_str": "135", "game.ers_stored": 0.75,
                                 "view_model_data.hud_t_boost": true, "view_model_data.hud_t_dp02": true,
                                 "session.current_car.position": "1" }, "team_colour": "#27F4D2" },
        "variants": { "f2": { "dial": "f2", "remove": ["batt"] } }
    })
}

#[test]
fn prefixes_come_from_the_addon_and_the_key() {
    assert_eq!(
        prefix_for("nyannoying.halo", "halo", None).unwrap(),
        "nyannoying_halo"
    );
    assert_eq!(
        prefix_for("swofty.gauges", "dial_a", None).unwrap(),
        "swofty_gauges_dial_a"
    );
    assert_eq!(prefix_for("a.b", "c", Some("mine")).unwrap(), "mine");
    assert!(prefix_for("a.b", "c", Some("Mine")).is_err());
    assert!(prefix_for("swofty.a-very-long-addon-name", "and_a_long_key", None).is_err());
}

#[test]
fn the_stock_facts_hold_in_the_game_files() {
    let Some(stock) = game() else { return };
    let r = check::stock_facts(&stock);
    for l in &r.ok {
        eprintln!("ok   {l}");
    }
    for l in &r.problems {
        eprintln!("FAIL {l}");
    }
    assert!(r.is_ok(), "{:#?}", r.problems);
}

#[test]
fn composes_every_element_kind_against_the_stock_dials() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("all");
    let rec = HudElement::parse(&record()).unwrap();
    let input = Input {
        addon: "test.hud".into(),
        key: "hud".into(),
        dir: dir.clone(),
        record: rec,
    };
    let out = compose(&stock, &[input]).unwrap();
    for l in &out.lines {
        eprintln!("{l}");
    }
    let names: Vec<&str> = out.archives.iter().map(|a| a.file.as_str()).collect();
    assert_eq!(names, ["hud_scene.erp", "hud_template.erp", "hud_spectator.erp"]);
    assert!(check::unpadded_clips(&out.archives).unwrap().is_empty());
    let scene = Erp::parse(&out.archives[0].bytes).unwrap();
    for dial in Dial::ALL {
        let f = dials::facts(dial);
        let res = scene.find(f.scene).unwrap();
        let p = Pkg::parse(&res.fragments[0].data().unwrap()).unwrap();
        assert!(crate::ui::idf::id_problems(&p).unwrap().is_empty(), "{}", f.scene);
        let root = crate::ui::idf::scenes(&p).unwrap()[0]
            .as_obj()
            .unwrap()
            .get("scene")
            .unwrap();
        assert!(
            crate::ui::idf::find(root, "test_hud_speed").is_some() || dial == Dial::F125,
            "{}",
            f.scene
        );
    }
    let f126 = dials::facts(Dial::F126);
    let anim = scene.find(&format!(
        "{}/zh_test_hud_batt_win_win?context=animation",
        f126.scene
    ));
    assert!(
        anim.is_ok()
            || scene
                .resources
                .iter()
                .any(|r| r.name.contains("context=animation"))
    );
    assert!(
        scene
            .resources
            .iter()
            .any(|r| r.name.ends_with("test_hud_base.tif.image"))
    );
    let tmpl = Erp::parse(&out.archives[1].bytes).unwrap();
    let t = Pkg::parse(&tmpl.find(dials::TEMPLATE).unwrap().fragments[0].data().unwrap()).unwrap();
    let at =
        crate::ui::node::find_path(&t.root, &format!("{}.test_hud_ctl", dials::F126.race_parent)).unwrap();
    assert!(!at.is_empty());
    let ours: std::collections::BTreeSet<String> =
        ["test_hud_ctl", "test_hud_pv_ctl"].map(String::from).into();
    let stock_t = Pkg::parse(&stock.read(dials::STOCK_ERP, dials::TEMPLATE).unwrap()[0]).unwrap();
    assert!(
        check::stock_kept(&stock_t, &t, &ours).unwrap(),
        "a stock template node changed"
    );
    let spec = Erp::parse(&out.archives[2].bytes).unwrap();
    let s = Pkg::parse(&spec.find(dials::SPECTATOR).unwrap().fragments[0].data().unwrap()).unwrap();
    let stock_s = Pkg::parse(&stock.read(dials::STOCK_ERP, dials::SPECTATOR).unwrap()[0]).unwrap();
    assert!(
        check::stock_kept(&stock_s, &s, &ours).unwrap(),
        "a stock spectator node changed"
    );
    for (d, ctl) in [
        (Dial::F126, "test_hud_ctl"),
        (Dial::F125, "test_hud_pv_ctl"),
        (Dial::F2, "test_hud_ctl"),
    ] {
        let parent = dials::facts(d).spectator_parent;
        assert!(
            crate::ui::node::find_path(&s.root, &format!("{parent}.{ctl}")).is_ok(),
            "{parent}"
        );
    }
    std::fs::remove_dir_all(dir).ok();
}

fn tower_record() -> serde_json::Value {
    serde_json::json!({
        "screen": "standings",
        "show": { "view_model": "view_model_data.hud_tt_show" },
        "also": { "replay": { "show": { "view_model": "view_model_data.hud_tt_on" } },
                  "spectator": { "show": { "view_model": "view_model_data.hud_tt_on" } } },
        "sprites": {
            "strip": { "png": "art/base.png", "bounds": [0, 0, 64, 20] },
            "bn_m": { "png": "art/solid.png", "bounds": [0, 0, 8, 8] }
        },
        "elements": [
            { "kind": "group", "id": "r00", "visible": { "view_model": "view_model_data.hud_tt_r00_on" }, "children": [
                { "kind": "sprite", "id": "s", "sprite": "strip", "layer": 20 },
                { "kind": "text", "id": "pos", "style": "OSD_24_bold", "align": "centre", "box": 12, "cap_height": 6,
                  "at": [6, 10], "value": { "view_model": "view_model_data.hud_tt_r00_pos" } },
                { "kind": "text", "id": "code", "style": "OSD_24_bold", "align": "left", "box": 24, "cap_height": 7,
                  "at": [20, 10], "value": { "session_car": "driver.second_name", "slot": 0 } }
            ] }
        ],
        "stock_nodes": [
            { "template": screens::LAPC_PARENT, "hide": screens::LAPC_HIDE,
              "while": { "view_model": "view_model_data.hud_tt_show" } }
        ],
        "restyle": { "preset": "banner", "sprite": "bn_m" },
        "editor": { "rows": 5, "preview": { "view_model_data.hud_tt_r00_on": true, "view_model_data.hud_tt_r00_pos": "10" } }
    })
}

fn pkg_in(erp: &Erp, name: &str) -> Pkg {
    Pkg::parse(&erp.find(name).unwrap().fragments[0].data().unwrap()).unwrap()
}

#[test]
fn a_screen_record_composes_the_three_screens_and_the_banner() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("tower");
    let rec = HudElement::parse(&tower_record()).unwrap();
    let input = Input {
        addon: "test.tower".into(),
        key: "tower".into(),
        dir: dir.clone(),
        record: rec,
    };
    let out = compose(&stock, &[input]).unwrap();
    for l in &out.lines {
        eprintln!("{l}");
    }
    let names: Vec<&str> = out.archives.iter().map(|a| a.file.as_str()).collect();
    assert_eq!(
        names,
        [
            "hud_template.erp",
            "hud_spectator.erp",
            "hud_screen_scene.erp",
            "hud_replay.erp",
            "hud_replay_scene.erp",
            "hud_screen_scene_2.erp"
        ]
    );
    assert_eq!(out.archives[2].erps, screen_build::SCREEN_SCENE_ERPS);
    assert_eq!(
        out.archives[5].erps,
        [
            "2025_asset_groups/ui_package/screens/hud.erp",
            "2025_asset_groups/ui_package/screens/hud_player_1.erp",
            "2025_asset_groups/ui_package/screens/hud_player_2.erp"
        ]
    );
    assert!(check::uncovered(&stock, &out.archives).unwrap().is_empty());
    assert!(check::constant_rotations(&out.archives).unwrap().is_empty());
    assert!(check::unpadded_clips(&out.archives).unwrap().is_empty());
    let also = Erp::parse(&out.archives[5].bytes).unwrap();
    let main = Erp::parse(&out.archives[2].bytes).unwrap();
    for name in [screens::CAP, screens::STACKED, screens::STACKED_UIPRE] {
        assert_eq!(
            also.find(name).unwrap().fragments[0].data().unwrap(),
            main.find(name).unwrap().fragments[0].data().unwrap(),
            "{name}"
        );
    }
    assert!(
        also.resources
            .iter()
            .any(|r| r.name.ends_with("test_tower_strip.tif.image"))
    );
    let scene = Erp::parse(&out.archives[2].bytes).unwrap();
    let cap = pkg_in(&scene, screens::CAP);
    assert!(crate::ui::idf::id_problems(&cap).unwrap().is_empty());
    let root = stock_edit::scene_root(&cap).unwrap();
    let mine = crate::ui::idf::find(root, "test_tower").unwrap();
    assert_eq!(
        crate::ui::idf::param(mine, "isHidden").and_then(stock_edit::as_bool),
        Some(true)
    );
    assert!(crate::ui::idf::find(root, "test_tower_s").is_some());
    assert!(crate::ui::idf::find(root, "test_tower_pos").is_some());
    let stacked = pkg_in(&scene, screens::STACKED);
    assert!(crate::ui::idf::id_problems(&stacked).unwrap().is_empty());
    let host = crate::ui::idf::find(stock_edit::scene_root(&stacked).unwrap(), "test_tower_host").unwrap();
    assert_eq!(crate::ui::idf::param_str(host, "fileRef"), Some(screens::CAP));
    let pv = crate::ui::idf::find(root, "test_tower_pv").expect("the editor copy");
    assert_ne!(
        crate::ui::idf::param(pv, "isHidden").and_then(stock_edit::as_bool),
        Some(true)
    );
    assert!(crate::ui::idf::find(root, "test_tower_pv_pos").is_some());
    let pre = swofty_formats::uipre::Uipre::parse(
        &scene.find(screens::STACKED_UIPRE).unwrap().fragments[0]
            .data()
            .unwrap(),
    )
    .unwrap();
    let host = screens::standings().host_name("test_tower");
    let inside = pre.element_ids_in(&host);
    for name in [
        "pill_rotation_anim",
        "test_tower",
        "test_tower_pv",
        "test_tower_pv_pos",
    ] {
        assert!(
            inside.contains(&swofty_formats::uipre::element_id(name)),
            "{name} in the host's preview"
        );
    }
    assert!(
        pre.element_ids()
            .contains(&swofty_formats::uipre::element_id(screens::STACKED_MASK))
    );
    assert!(
        scene
            .resources
            .iter()
            .any(|r| r.name.ends_with("test_tower_strip.tif.image"))
    );
    assert!(scene.find(&stock_edit::emb_name("test_tower", "bn_m")).is_ok());
    for (rel, fills, twin, hide, _) in stock_edit::BANNER_SCENES {
        let sc = stock_edit::banner_scene(rel);
        let p = pkg_in(&scene, &sc);
        assert!(crate::ui::idf::id_problems(&p).unwrap().is_empty(), "{sc}");
        let root = stock_edit::scene_root(&p).unwrap();
        for (el, _) in fills {
            let e = crate::ui::idf::find(root, el).unwrap();
            let m = e.o().unwrap().at("geoms").unwrap().a().unwrap()[0]
                .o()
                .unwrap()
                .str_at("materialid")
                .unwrap();
            assert_eq!(m, stock_edit::emb_name("test_tower", "bn_m"), "{sc}/{el}");
        }
        if let Some(t) = twin {
            assert!(crate::ui::idf::find(root, &format!("{t}_bn")).is_some(), "{sc}");
        }
        for el in hide {
            let e = crate::ui::idf::find(root, el).unwrap();
            assert_eq!(
                crate::ui::idf::param(e, "isHidden").and_then(stock_edit::as_bool),
                Some(true),
                "{sc}/{el}"
            );
        }
    }
    assert!(scene.find(stock_edit::BANNER_SYSTEM).is_ok());

    let mut ours: std::collections::BTreeSet<String> = ["test_tower_ctl", "test_tower_sn0", "test_tower_sn1"]
        .map(String::from)
        .into();
    for (_, _, _, hide, _) in stock_edit::BANNER_SCENES {
        ours.extend(hide.iter().map(|h| format!("test_tower_hide_{h}")));
    }
    let tmpl = Erp::parse(&out.archives[0].bytes).unwrap();
    let t = pkg_in(&tmpl, dials::TEMPLATE);
    let base = format!("{}.test_tower_ctl", screens::STANDINGS_PARENT);
    for sub in [
        "mask_0",
        "host_n",
        "host_n.v_cap",
        "host_n.v_pv",
        "host_n.root_n",
        "host_n.v_r00",
        "host_n.t_pos",
    ] {
        assert!(
            crate::ui::node::find_path(&t.root, &format!("{base}.{sub}")).is_ok(),
            "{sub}"
        );
    }
    let at = crate::ui::node::find_path(&t.root, screens::STANDINGS_PARENT).unwrap();
    let parent = crate::ui::node::node_at(&t.root, &at).unwrap();
    let enable = parent
        .slot(crate::ui::node::COMMANDS)
        .unwrap()
        .iter()
        .find(|c| c.as_obj().unwrap().str_at("commandName").unwrap() == "enable")
        .unwrap();
    assert!(format!("{enable:?}").contains("test_tower_ctl"));
    let lapc =
        crate::ui::node::find_path(&t.root, &format!("{}.test_tower_sn0", screens::LAPC_PARENT)).unwrap();
    let grp = crate::ui::node::node_at(&t.root, &lapc).unwrap();
    assert_eq!(
        grp.slot(crate::ui::node::CHILDREN).unwrap().len(),
        screens::LAPC_HIDE.len()
    );
    for root in ["v_test_tower", "v_test_tower_pv"] {
        assert!(
            crate::ui::node::find_path(
                &t.root,
                &format!("{}.test_tower_sn1.{root}", screens::STANDINGS_CAP_NODE)
            )
            .is_ok(),
            "{root}"
        );
    }
    assert!(crate::ui::node::find_path(
        &t.root,
        "hud_notifications_dynamic_p2.osd_visibility_option.hud_notifications_dynamic_p2.notification_0.hud_message.right.central_message.test_tower_hide_ms_colour"
    )
    .is_ok());
    let stock_t = Pkg::parse(&stock.read(dials::STOCK_ERP, dials::TEMPLATE).unwrap()[0]).unwrap();
    assert!(
        check::stock_kept(&stock_t, &t, &ours).unwrap(),
        "a stock template node changed"
    );

    let spec = Erp::parse(&out.archives[1].bytes).unwrap();
    let s = pkg_in(&spec, dials::SPECTATOR);
    assert!(
        crate::ui::node::find_path(
            &s.root,
            &format!("{}.test_tower_ctl.host_n", screens::SPECT_PARENT)
        )
        .is_ok()
    );
    let stock_s = Pkg::parse(&stock.read(dials::STOCK_ERP, dials::SPECTATOR).unwrap()[0]).unwrap();
    assert!(
        check::stock_kept(&stock_s, &s, &ours).unwrap(),
        "a stock spectator node changed"
    );
    let table = pkg_in(&spec, screens::SPECT_TABLE);
    assert!(crate::ui::idf::id_problems(&table).unwrap().is_empty());
    let host = crate::ui::idf::find(stock_edit::scene_root(&table).unwrap(), "test_tower_host").unwrap();
    assert_eq!(crate::ui::idf::param_str(host, "fileRef"), Some(screens::CAP));

    let replay = Erp::parse(&out.archives[3].bytes).unwrap();
    assert_eq!(
        out.archives[3].erps,
        [screens::FLOW_REPLAY, screens::REPLAY_SCREEN_ERP]
    );
    assert_eq!(out.archives[4].erps, [screens::FLOW_FRONTEND]);
    let rt = pkg_in(&replay, screens::REPLAY_UI);
    assert!(
        crate::ui::node::find_path(
            &rt.root,
            &format!("{}.test_tower_ctl.extra_hide", screens::REPLAY_ROOT)
        )
        .is_ok()
    );
    let stock_r = Pkg::parse(&stock.read(screens::FLOW_REPLAY, screens::REPLAY_UI).unwrap()[0]).unwrap();
    assert!(
        check::stock_kept(&stock_r, &rt, &ours).unwrap(),
        "a stock replay node changed"
    );
    let rs = pkg_in(&replay, screens::REPLAY_SCENE);
    assert!(crate::ui::idf::id_problems(&rs).unwrap().is_empty());
    let host = crate::ui::idf::find(stock_edit::scene_root(&rs).unwrap(), "test_tower_host").unwrap();
    assert_eq!(crate::ui::idf::param_str(host, "fileRef"), Some(screens::CAP));
    let p = crate::ui::idf::position(host).unwrap();
    assert!((p[0] - screens::REPLAY_POS[0]).abs() < 1e-4 && (p[1] - screens::REPLAY_POS[1]).abs() < 1e-4);
    let rs2 = Erp::parse(&out.archives[4].bytes).unwrap();
    assert!(rs2.find(screens::REPLAY_SCENE).is_ok());
    std::fs::remove_dir_all(dir).ok();
}

fn fake_archive(names: &[&str]) -> Vec<u8> {
    use swofty_formats::erp::{Fragment, HEADER_LEN, MAGIC, STORED};
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    Erp {
        header,
        version: 3,
        resources: names
            .iter()
            .map(|n| Resource {
                name: (*n).into(),
                kind: swofty_formats::erp::kind("UILayout").unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", 1, 0x10, STORED, vec![n.len() as u8])],
                hash: [0; 16],
            })
            .collect(),
    }
    .to_bytes()
    .unwrap()
}

#[test]
fn a_clip_whose_syst_is_not_a_multiple_of_16_bytes_is_reported() {
    use swofty_formats::erp::{Fragment, HEADER_LEN, MAGIC, STORED};
    let frag = |name: &[u8; 4], raw: Vec<u8>| Fragment::new(*name, raw.len() as u64, 0x10, STORED, raw);
    let clip = |name: &str, syst: usize| Resource {
        name: name.into(),
        kind: swofty_formats::erp::kind("AnimClip").unwrap(),
        unk: vec![0; 4],
        fragments: vec![frag(b"temp", vec![0]), frag(b"syst", vec![0; syst])],
        hash: [0; 16],
    };
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let bytes = Erp {
        header,
        version: 3,
        resources: vec![clip("slide", 92), clip("needle", 112), clip("short", 108)],
    }
    .to_bytes()
    .unwrap();
    let built = [Built {
        file: "hud_scene.erp".into(),
        erps: vec![],
        bytes,
    }];
    assert_eq!(
        check::unpadded_clips(&built).unwrap(),
        ["slide (92 bytes)", "short (108 bytes)"]
    );
}

#[test]
fn a_resource_carried_outside_the_made_for_archives_goes_there_too() {
    let dir = std::env::temp_dir().join(format!("swofty-hud-carriers-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let ui = "2025_asset_groups/ui_package";
    let toast = "eaid://ui_package/art/osd/units/message_system/central_message.idf";
    let sprite = "eaid://ui_package/art/osd/tt_strip.tif.image";
    for (rel, names) in [
        ("flow_race.erp", vec![screens::CAP, toast]),
        ("flow_options.erp", vec![screens::CAP]),
        ("persistent.erp", vec![screens::CAP]),
        ("screens/hud.erp", vec![screens::CAP, "eaid://ui_package/x.idf"]),
        ("screens/hud_player_1.erp", vec![screens::CAP]),
        ("screens/other.erp", vec!["eaid://ui_package/y.idf"]),
    ] {
        let path = dir.join(ui).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, fake_archive(&names)).unwrap();
    }
    std::fs::write(dir.join(ui).join("screens/junk.erp"), b"not an archive").unwrap();
    let stock = GameStock::new(&dir);
    let made_for: Vec<String> = screen_build::SCREEN_SCENE_ERPS
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut archives = vec![Built {
        file: "hud_screen_scene.erp".into(),
        erps: made_for.clone(),
        bytes: fake_archive(&[screens::CAP, toast, sprite]),
    }];
    let left = check::uncovered(&stock, &archives).unwrap();
    assert_eq!(
        left,
        [
            format!(
                "{} is composed, but {ui}/screens/hud.erp also carries it and would keep the stock copy",
                screens::CAP
            ),
            format!(
                "{} is composed, but {ui}/screens/hud_player_1.erp also carries it and would keep the stock copy",
                screens::CAP
            ),
        ]
    );
    let mut lines = Vec::new();
    cover_carriers(&stock, &mut archives, &mut lines).unwrap();
    assert_eq!(archives.len(), 2);
    assert_eq!(archives[0].erps, made_for);
    assert_eq!(archives[1].file, "hud_screen_scene_2.erp");
    assert_eq!(
        archives[1].erps,
        [
            format!("{ui}/screens/hud.erp"),
            format!("{ui}/screens/hud_player_1.erp")
        ]
    );
    let added = Erp::parse(&archives[1].bytes).unwrap();
    let names: Vec<&str> = added.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, [screens::CAP, sprite]);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(check::uncovered(&stock, &archives).unwrap().is_empty());

    let mut shared = vec![
        Built {
            file: "a.erp".into(),
            erps: vec![format!("{ui}/flow_race.erp"), format!("{ui}/flow_options.erp")],
            bytes: fake_archive(&[screens::CAP]),
        },
        Built {
            file: "b.erp".into(),
            erps: vec![
                format!("{ui}/persistent.erp"),
                format!("{ui}/screens/hud.erp"),
                format!("{ui}/screens/hud_player_1.erp"),
            ],
            bytes: fake_archive(&[screens::CAP]),
        },
    ];
    cover_carriers(&stock, &mut shared, &mut lines).unwrap();
    assert_eq!(shared.len(), 2, "every carrier is covered between the two");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn two_screen_records_hide_each_others_roots_in_their_hosts() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("two-screens");
    let mk = |addon: &str| {
        let mut v = tower_record();
        let o = v.as_object_mut().unwrap();
        o.remove("stock_nodes");
        o.remove("restyle");
        Input {
            addon: addon.into(),
            key: "tower".into(),
            dir: dir.clone(),
            record: HudElement::parse(&v).unwrap(),
        }
    };
    let out = compose(&stock, &[mk("one.season"), mk("two.season")]).unwrap();
    let ins: Vec<String> = [("one.season", "tower"), ("two.season", "tower")]
        .iter()
        .map(|(a, k)| prefix_for(a, k, None).unwrap())
        .collect();
    let (a, b) = (&ins[0], &ins[1]);
    let file = |name: &str| {
        let built = out.archives.iter().find(|x| x.file == name).unwrap();
        Erp::parse(&built.bytes).unwrap()
    };
    let checks = [
        (
            pkg_in(&file("hud_template.erp"), dials::TEMPLATE),
            screens::STANDINGS_PARENT,
        ),
        (
            pkg_in(&file("hud_spectator.erp"), dials::SPECTATOR),
            screens::SPECT_PARENT,
        ),
        (
            pkg_in(&file("hud_replay.erp"), screens::REPLAY_UI),
            screens::REPLAY_ROOT,
        ),
    ];
    for (t, parent) in &checks {
        for (me, other) in [(a, b), (b, a)] {
            for (node, el) in [
                (format!("o_{other}"), other.clone()),
                (format!("o_{other}_pv"), screen_build::preview_root(other)),
            ] {
                let path = format!("{parent}.{me}_ctl.host_n.{node}");
                let at = crate::ui::node::find_path(&t.root, &path).unwrap_or_else(|_| panic!("{path}"));
                let n = crate::ui::node::node_at(&t.root, &at).unwrap();
                let layout: Vec<String> = n
                    .slot(crate::ui::node::LAYOUT)
                    .unwrap()
                    .iter()
                    .flat_map(|c| c.as_arr().unwrap().items.iter())
                    .map(|o| o.as_obj().unwrap().str_at("debugName").unwrap().to_owned())
                    .collect();
                assert_eq!(layout, std::slice::from_ref(&el), "{path}");
                let vis = n.props().unwrap().get("visible").unwrap().as_obj().unwrap();
                assert_eq!(
                    vis.get("value")
                        .and_then(swofty_formats::pkg::Node::as_val)
                        .and_then(stock_edit::as_bool),
                    Some(false),
                    "{path}"
                );
            }
            assert!(
                crate::ui::node::find_path(&t.root, &format!("{parent}.{me}_ctl.host_n.o_{me}")).is_err(),
                "{me} does not hide its own root"
            );
        }
    }
    let scene = file("hud_screen_scene.erp");
    let pre = swofty_formats::uipre::Uipre::parse(
        &scene.find(screens::STACKED_UIPRE).unwrap().fragments[0]
            .data()
            .unwrap(),
    )
    .unwrap();
    for (me, other) in [(a, b), (b, a)] {
        let inside = pre.element_ids_in(&screens::standings().host_name(me));
        for name in [other.clone(), screen_build::preview_root(other)] {
            assert!(
                inside.contains(&swofty_formats::uipre::element_id(&name)),
                "{name} hidden in {me}'s host in the editor"
            );
        }
    }
    assert!(out.lines.iter().any(|l| l.contains("other screen records")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn two_addons_share_one_scene_and_one_template_pass() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("two");
    let mk = |addon: &str| Input {
        addon: addon.into(),
        key: "hud".into(),
        dir: dir.clone(),
        record: HudElement::parse(&record()).unwrap(),
    };
    let out = compose(&stock, &[mk("one.hud"), mk("two.hud")]).unwrap();
    let tmpl = Erp::parse(&out.archives[1].bytes).unwrap();
    let t = Pkg::parse(&tmpl.find(dials::TEMPLATE).unwrap().fragments[0].data().unwrap()).unwrap();
    for p in ["one_hud", "two_hud"] {
        assert!(crate::ui::node::find_path(&t.root, &format!("{}.{p}_ctl", dials::F126.race_parent)).is_ok());
    }
    let clash = compose(&stock, &[mk("one.hud"), mk("one.hud")]);
    assert!(clash.unwrap_err().to_string().contains("both get the prefix"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn two_screen_records_wrap_the_lap_counter_inside_each_other() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("two-wraps");
    let flag_node = "hud_left_notification_dynamic.osd_visibility_option.left_notification.left_message";
    let hide = [
        "car_category_logo",
        "session_icon",
        "centre_lap_value",
        "separator_bg",
        "separator_bg_top",
        "centre_formation",
    ];
    let wrap = ["edge_base", "edge_border", "lap_bg", "lockup"];
    let mk = |addon: &str| {
        let mut v = tower_record();
        let o = v.as_object_mut().unwrap();
        o.remove("restyle");
        o.insert(
            "stock_nodes".into(),
            serde_json::json!([
                { "template": screens::LAPC_PARENT, "scene": screens::LAPC_SCENE, "hide": hide, "wrap": wrap,
                  "while": { "view_model": "view_model_data.hud_tt_show" } },
                { "template": flag_node, "hide": ["boundless"], "while": { "view_model": "view_model_data.hud_tt_show" } }
            ]),
        );
        Input {
            addon: addon.into(),
            key: "tower".into(),
            dir: dir.clone(),
            record: HudElement::parse(&v).unwrap(),
        }
    };
    let inputs = [mk("two.season"), mk("one.season")];
    let t_stock = Pkg::parse(&stock.read(dials::STOCK_ERP, dials::TEMPLATE).unwrap()[0]).unwrap();
    let flag = stock_edit::scene_of(&t_stock.root, flag_node).unwrap().unwrap();
    let mut shared = vec![
        screens::LAPC_SCENE.to_owned(),
        flag.clone(),
        screens::STACKED.to_owned(),
        screens::REPLAY_SCENE.to_owned(),
        screens::SPECT_TABLE.to_owned(),
        screens::SPECT_SCREEN.to_owned(),
    ];
    shared.sort();
    assert_eq!(screen_build::shared_scenes(&stock, &inputs).unwrap(), shared);
    let out = compose(&stock, &inputs).unwrap();
    assert!(check::uncovered(&stock, &out.archives).unwrap().is_empty());
    let a = prefix_for("one.season", "tower", None).unwrap();
    let b = prefix_for("two.season", "tower", None).unwrap();
    let file = |name: &str| {
        let built = out.archives.iter().find(|x| x.file == name).unwrap();
        Erp::parse(&built.bytes).unwrap()
    };
    let scene = file("hud_screen_scene.erp");
    for name in [screens::LAPC_SCENE, flag.as_str()] {
        assert_eq!(
            scene.resources.iter().filter(|r| r.name == name).count(),
            1,
            "{name}"
        );
    }
    let lapc = pkg_in(&scene, screens::LAPC_SCENE);
    assert!(crate::ui::idf::id_problems(&lapc).unwrap().is_empty());
    let lc: Vec<String> = crate::ui::idf::elements(stock_edit::scene_root(&lapc).unwrap())
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    for w in wrap {
        let nested = format!(
            "{}.{}.{w}",
            stock_edit::wrap_name(&a, w),
            stock_edit::wrap_name(&b, w)
        );
        assert!(lc.contains(&nested), "{nested} in {lc:#?}");
    }
    for h in hide {
        let wrapper = stock_edit::wrap_name(&b, h);
        assert!(
            lc.iter().any(|p| p.ends_with(&format!("{wrapper}.{h}"))),
            "{h} inside {wrapper}"
        );
        assert!(!lc.iter().any(|p| p.contains(&stock_edit::wrap_name(&a, h))));
    }
    let fl = pkg_in(&scene, &flag);
    assert!(crate::ui::idf::id_problems(&fl).unwrap().is_empty());
    assert!(
        crate::ui::idf::find(
            stock_edit::scene_root(&fl).unwrap(),
            &stock_edit::wrap_name(&b, "boundless")
        )
        .is_some()
    );

    let t = pkg_in(&file("hud_template.erp"), dials::TEMPLATE);
    let targets = |parent: &str, group: &str| -> Vec<String> {
        let at = crate::ui::node::find_path(&t.root, &format!("{parent}.{group}")).unwrap();
        crate::ui::node::node_at(&t.root, &at)
            .unwrap()
            .slot(crate::ui::node::CHILDREN)
            .unwrap()
            .iter()
            .flat_map(|n| n.slot(crate::ui::node::LAYOUT).unwrap().to_vec())
            .flat_map(|c| c.as_arr().unwrap().items.clone())
            .map(|o| o.as_obj().unwrap().str_at("debugName").unwrap().to_owned())
            .collect()
    };
    let mine_a = targets(screens::LAPC_PARENT, &format!("{a}_sn0"));
    let mine_b = targets(screens::LAPC_PARENT, &format!("{b}_sn0"));
    let mut want_a: Vec<String> = hide.iter().map(|h| (*h).to_owned()).collect();
    want_a.extend(wrap.iter().map(|w| stock_edit::wrap_name(&a, w)));
    assert_eq!(mine_a, want_a);
    let want_b: Vec<String> = wrap
        .iter()
        .chain(&hide)
        .map(|w| stock_edit::wrap_name(&b, w))
        .collect();
    assert_eq!(mine_b, want_b);
    for el in mine_a.iter().chain(&mine_b) {
        assert!(
            lc.iter().any(|p| p.rsplit('.').next() == Some(el.as_str())),
            "{el} in the scene"
        );
    }
    assert_eq!(targets(flag_node, &format!("{a}_sn1")), ["boundless"]);
    assert_eq!(
        targets(flag_node, &format!("{b}_sn1")),
        [stock_edit::wrap_name(&b, "boundless")]
    );

    let chain = |root: &swofty_formats::pkg::Node, path: &str| -> Vec<String> {
        let at = crate::ui::node::find_path(root, path).unwrap_or_else(|_| panic!("{path}"));
        crate::ui::node::node_at(root, &at)
            .unwrap()
            .slot(crate::ui::node::LAYOUT)
            .unwrap()
            .iter()
            .flat_map(|c| c.as_arr().unwrap().items.clone())
            .map(|o| o.as_obj().unwrap().str_at("debugName").unwrap().to_owned())
            .collect()
    };
    let paths_of = |erp: &Erp, scene: &str| -> Vec<String> {
        let p = pkg_in(erp, scene);
        assert!(crate::ui::idf::id_problems(&p).unwrap().is_empty(), "{scene}");
        crate::ui::idf::elements(stock_edit::scene_root(&p).unwrap())
            .into_iter()
            .map(|(p, _)| p)
            .collect()
    };
    let spec = file("hud_spectator.erp");
    let replay_scene = file("hud_replay_scene.erp");
    let screens_of = [
        (
            &t,
            screens::standings(),
            &scene,
            vec![vec![stock_edit::wrap_name(&b, "mask")]],
        ),
        (
            &pkg_in(&spec, dials::SPECTATOR),
            screens::spectator(),
            &spec,
            vec![
                vec![
                    screens::SPECT_PREFIX.to_owned(),
                    screens::SPECT_TABLE_EL.to_owned(),
                    stock_edit::wrap_name(&b, "transition"),
                ],
                vec![
                    screens::SPECT_PREFIX.to_owned(),
                    stock_edit::wrap_name(&b, screens::SPECT_HEADER_EL),
                ],
            ],
        ),
        (
            &pkg_in(&file("hud_replay.erp"), screens::REPLAY_UI),
            screens::replay(),
            &replay_scene,
            vec![
                vec![stock_edit::wrap_name(&b, screens::REPLAY_TABLE)],
                vec![stock_edit::wrap_name(&b, screens::REPLAY_HEADER)],
            ],
        ),
    ];
    for (tp, facts, erp, want_b) in screens_of {
        for (i, (stock_chain, want)) in facts.masks.iter().zip(&want_b).enumerate() {
            let at = |p: &str| format!("{}.{p}_ctl.mask_{i}", facts.parent_ui);
            assert_eq!(&chain(&tp.root, &at(&a)), stock_chain, "{} {a}", facts.key);
            assert_eq!(&chain(&tp.root, &at(&b)), want, "{} {b}", facts.key);
        }
        for (_, el, sc) in facts.mask_wraps() {
            let wrapper = stock_edit::wrap_name(&b, &el);
            let paths = paths_of(erp, &sc);
            assert!(
                paths.iter().any(|p| p.ends_with(&format!("{wrapper}.{el}"))),
                "{el} inside {wrapper} in {sc}"
            );
            assert!(!paths.iter().any(|p| p.contains(&stock_edit::wrap_name(&a, &el))));
        }
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn two_dial_records_hide_the_stock_dial_through_their_own_wrappers() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("two-dials");
    let mk = |addon: &str, leaf: &str| Input {
        addon: addon.into(),
        key: "dial".into(),
        dir: dir.clone(),
        record: HudElement::parse(&serde_json::json!({
            "dial": "f1_25",
            "stock": "hide",
            "switch": { "view_model": leaf, "default": false },
            "sprites": { "base": { "png": "art/base.png", "bounds": [-60, -20, 60, 20] } },
            "elements": [ { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 } ]
        }))
        .unwrap(),
    };
    let inputs = [
        mk("two.season", "view_model_data.hud_b_dial"),
        mk("one.season", "view_model_data.hud_a_dial"),
    ];
    let out = compose(&stock, &inputs).unwrap();
    let a = prefix_for("one.season", "dial", None).unwrap();
    let b = prefix_for("two.season", "dial", None).unwrap();
    let facts = dials::facts(Dial::F125);
    let find = |name: &str| {
        out.archives
            .iter()
            .map(|x| Erp::parse(&x.bytes).unwrap())
            .find(|e| e.find(name).is_ok())
            .unwrap_or_else(|| panic!("{name}"))
    };
    let scene = pkg_in(&find(facts.scene), facts.scene);
    assert!(crate::ui::idf::id_problems(&scene).unwrap().is_empty());
    let paths: Vec<String> = crate::ui::idf::elements(stock_edit::scene_root(&scene).unwrap())
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    for part in dials::STOCK_KIDS {
        let nested = format!(
            "{}.{}.{part}",
            dials::CUSTOMISATION,
            stock_edit::wrap_name(&b, part)
        );
        assert!(paths.contains(&nested), "{nested}");
        assert!(!paths.iter().any(|p| p.contains(&stock_edit::wrap_name(&a, part))));
    }
    let t = pkg_in(&find(dials::TEMPLATE), dials::TEMPLATE);
    let mut nodes = Vec::new();
    crate::ui::node::node_paths_of(&t.root, "", &mut nodes);
    let stock_hides = |pfx: &str| -> Vec<(String, String)> {
        nodes
            .iter()
            .filter(|(p, _)| p.contains(&format!(".{pfx}_ctl.v_stock_")))
            .map(|(p, n)| {
                let el = n
                    .slot(crate::ui::node::LAYOUT)
                    .unwrap()
                    .iter()
                    .flat_map(|c| c.as_arr().unwrap().items.clone())
                    .map(|o| o.as_obj().unwrap().str_at("debugName").unwrap().to_owned())
                    .collect::<Vec<_>>()
                    .join(".");
                let db = n
                    .props()
                    .unwrap()
                    .get("visible")
                    .unwrap()
                    .as_obj()
                    .unwrap()
                    .str_at("dbentry")
                    .unwrap()
                    .to_owned();
                (p.rsplit('.').next().unwrap().to_owned(), format!("{el} <- {db}"))
            })
            .collect()
    };
    let want = |pfx: &str, leaf: &str, wrapped: bool| -> Vec<(String, String)> {
        dials::STOCK_KIDS
            .iter()
            .map(|part| {
                let el = if wrapped {
                    stock_edit::wrap_name(pfx, part)
                } else {
                    (*part).to_owned()
                };
                (format!("v_stock_{part}"), format!("{el} <- #{leaf}"))
            })
            .collect()
    };
    assert_eq!(stock_hides(&a), want(&a, "view_model_data.hud_a_dial", false));
    assert_eq!(stock_hides(&b), want(&b, "view_model_data.hud_b_dial", true));
    assert!(
        out.lines
            .iter()
            .any(|l| l.starts_with(&format!("{b}: another record hides the stock")))
    );
    std::fs::remove_dir_all(dir).ok();
}

fn shared_input(tag: &str, addon: &str, prefix: Option<&str>) -> Input {
    let mut v = record();
    if let Some(p) = prefix {
        v["prefix"] = serde_json::json!(p);
    }
    // same_sprites compares bytes only, so plain files stand in for the sprites
    let dir = std::env::temp_dir().join(format!("swofty-hud-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("art")).unwrap();
    for n in ["base", "lit", "fill", "mask", "solid"] {
        std::fs::write(dir.join(format!("art/{n}.png")), n.as_bytes()).unwrap();
    }
    Input {
        addon: addon.into(),
        key: "era".into(),
        dir,
        record: HudElement::parse(&v).unwrap(),
    }
}

#[test]
fn identical_records_from_several_addons_compose_once() {
    let a = shared_input("shared-a", "x.season-1955", Some("vmkzt"));
    let b = shared_input("shared-b", "x.season-1975", Some("vmkzt"));
    let c = shared_input("shared-c", "x.season-1995", Some("vmkzt"));
    let (kept, lines) = shared_records(vec![a, b, c]).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].addon, "x.season-1955");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].contains("x.season-1975 brings the same record era as x.season-1955"),
        "{lines:?}"
    );
}

#[test]
fn records_that_differ_stay_apart() {
    // other sprite bytes
    let a = shared_input("differ-a", "x.a", Some("vmkzt"));
    let b = shared_input("differ-b", "x.b", Some("vmkzt"));
    std::fs::write(b.dir.join("art/base.png"), b"other").unwrap();
    let (kept, lines) = shared_records(vec![a, b]).unwrap();
    assert_eq!((kept.len(), lines.len()), (2, 0));
    // another record under the same prefix
    let a = shared_input("differ-c", "x.a", Some("vmkzt"));
    let mut b = shared_input("differ-d", "x.b", Some("vmkzt"));
    b.record.elements.pop();
    assert_eq!(shared_records(vec![a, b]).unwrap().0.len(), 2);
    // no explicit prefix: the prefix comes from the addon, never shared
    let a = shared_input("differ-e", "x.a", None);
    let b = shared_input("differ-f", "x.b", None);
    assert_eq!(shared_records(vec![a, b]).unwrap().0.len(), 2);
}

const YIELD_LEAF: &str = "view_model_data.hud_y_on";

fn yield_input(addon: &str, parts: &[&str]) -> Input {
    Input {
        addon: addon.into(),
        key: "yield".into(),
        dir: std::env::temp_dir(),
        record: HudElement::parse(&serde_json::json!({
            "yield": { "parts": parts, "while": { "view_model": YIELD_LEAF } }
        }))
        .unwrap(),
    }
}

/// Every copy of every resource in `out`, by name.
fn copies(out: &Output) -> BTreeMap<String, Vec<Vec<u8>>> {
    let mut m: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
    for a in &out.archives {
        for r in Erp::parse(&a.bytes).unwrap().resources {
            m.entry(r.name.clone())
                .or_default()
                .push(r.fragments[0].data().unwrap());
        }
    }
    m
}

fn one_copy(out: &Output, name: &str) -> Vec<u8> {
    let all = copies(out)
        .remove(name)
        .unwrap_or_else(|| panic!("{name} is not composed"));
    assert!(all.iter().all(|c| *c == all[0]), "{name}: the copies differ");
    all[0].clone()
}

fn splice_out(e: &mut swofty_formats::pkg::Node, name: &str) -> bool {
    let Ok(kids) = crate::ui::idf::children_mut(e) else {
        return false;
    };
    if let Some(i) = kids
        .iter()
        .position(|c| c.as_obj().is_some() && crate::ui::idf::elem_name(c) == name)
    {
        let mut null = kids.remove(i);
        let inner = std::mem::take(crate::ui::idf::children_mut(&mut null).unwrap());
        for (k, c) in inner.into_iter().enumerate() {
            kids.insert(i + k, c);
        }
        return true;
    }
    kids.iter_mut()
        .filter(|c| c.as_obj().is_some())
        .any(|c| splice_out(c, name))
}

/// The scene with the nulls `name`, `name_2` … taken out and their children back in their place.
fn unyield(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut pkg = Pkg::parse(bytes).unwrap();
    let entry = &mut crate::ui::idf::scenes_mut(&mut pkg).unwrap()[0];
    let root = entry.o_mut().unwrap().at_mut("scene").unwrap();
    assert!(splice_out(root, name), "no {name}");
    for k in 2.. {
        if !splice_out(root, &format!("{name}_{k}")) {
            break;
        }
    }
    for kid in crate::ui::idf::children_mut(root).unwrap() {
        let n = crate::ui::idf::elem_name(kid).to_owned();
        crate::ui::idf::recompute_ids(kid, &n).unwrap();
    }
    crate::ui::idf::relookup(entry).unwrap();
    pkg.to_bytes().unwrap()
}

/// Every element of a scene by path, with its own fields (its id among them) and not its children,
/// and the scene's lookup.
fn scene_view(bytes: &[u8]) -> Vec<(String, String)> {
    let p = Pkg::parse(bytes).unwrap();
    let entry = &crate::ui::idf::scenes(&p).unwrap()[0];
    let mut out: Vec<(String, String)> = crate::ui::idf::elements(stock_edit::scene_root(&p).unwrap())
        .into_iter()
        .map(|(path, e)| {
            let mut own = crate::ui::idf::detached(e);
            crate::ui::idf::clear_list(&mut own, "children").ok();
            (path, crate::ui::node::canonical(&own))
        })
        .collect();
    let lookup = entry.as_obj().unwrap().get("nodeidlookup").unwrap();
    out.push(("nodeidlookup".into(), crate::ui::node::canonical(lookup)));
    out
}

/// The template with every yield group of `pfx` taken out.
fn ungrouped(bytes: &[u8], pfx: &str) -> String {
    let mut pkg = Pkg::parse(bytes).unwrap();
    let mut paths = Vec::new();
    crate::ui::node::node_paths_of(&pkg.root, "", &mut paths);
    let groups: Vec<String> = paths
        .iter()
        .map(|(p, _)| p.clone())
        .filter(|p| {
            p.rsplit('.')
                .next()
                .is_some_and(|n| n.starts_with(&format!("{pfx}_yield_")))
        })
        .collect();
    for g in groups {
        let (parent, name) = g.rsplit_once('.').unwrap();
        let at = crate::ui::node::find_path(&pkg.root, parent).unwrap();
        let p = crate::ui::node::node_at_mut(&mut pkg.root, &at).unwrap();
        p.slot_mut(crate::ui::node::CHILDREN)
            .unwrap()
            .retain(|c| !(c.is_node() && c.name() == name));
    }
    crate::ui::node::canonical(&pkg.root)
}

fn chain_of(n: &swofty_formats::pkg::Node) -> Vec<String> {
    n.slot(crate::ui::node::LAYOUT)
        .unwrap()
        .iter()
        .flat_map(|c| c.as_arr().unwrap().items.clone())
        .map(|o| o.as_obj().unwrap().str_at("debugName").unwrap().to_owned())
        .collect()
}

fn element_paths(bytes: &[u8]) -> Vec<String> {
    let p = Pkg::parse(bytes).unwrap();
    assert!(crate::ui::idf::id_problems(&p).unwrap().is_empty());
    crate::ui::idf::elements(stock_edit::scene_root(&p).unwrap())
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

fn template_of(stock: &dyn Stock, name: &str) -> Vec<u8> {
    stock.read(yields::stock_erp(name), name).unwrap().remove(0)
}

/// Checks the yield's template groups in `out`: one per hook, the null's visible bound to NOT the leaf.
fn assert_hooks(out: &Output, pfx: &str, parts: &[&str]) {
    for part in parts {
        for (i, spot) in yields::part(part).unwrap().spots.iter().enumerate() {
            for hook in spot.hooks {
                let t = Pkg::parse(&one_copy(out, hook.template)).unwrap();
                let path = format!(
                    "{}.{}.{}",
                    hook.parent,
                    yields::group_name(pfx, part),
                    yields::node_name(i)
                );
                let at = crate::ui::node::find_path(&t.root, &path).unwrap_or_else(|_| panic!("{path}"));
                let n = crate::ui::node::node_at(&t.root, &at).unwrap();
                let mut want: Vec<String> = hook.via.iter().map(|s| (*s).to_owned()).collect();
                want.push(yields::null_name(pfx));
                assert_eq!(chain_of(n), want, "{path}");
                let vis = n.props().unwrap().get("visible").unwrap().as_obj().unwrap();
                assert_eq!(vis.str_at("dbentry").unwrap(), format!("#{YIELD_LEAF}"), "{path}");
                assert_eq!(
                    vis.get("dbentrynegate")
                        .and_then(|v| v.as_val())
                        .and_then(stock_edit::as_bool),
                    Some(true),
                    "{path}"
                );
                assert_eq!(
                    vis.get("value")
                        .and_then(|v| v.as_val())
                        .and_then(stock_edit::as_bool),
                    Some(true),
                    "{path}"
                );
            }
        }
    }
}

#[test]
fn a_yield_alone_wraps_every_part_and_leaves_the_rest_as_the_game_has_it() {
    let Some(stock) = game() else { return };
    let input = yield_input("swofty.yielder", &yields::KEYS);
    let pfx = prefix_for("swofty.yielder", "yield", None).unwrap();
    let out = compose(&stock, std::slice::from_ref(&input)).unwrap();
    assert!(check::uncovered(&stock, &out.archives).unwrap().is_empty());
    let null = yields::null_name(&pfx);
    for part in yields::PARTS {
        for spot in part.spots {
            let composed = one_copy(&out, spot.scene);
            let paths = element_paths(&composed);
            let held: Vec<&String> = paths
                .iter()
                .filter(|p| p.split('.').any(|s| s == null) && !p.ends_with(&null))
                .collect();
            assert!(!held.is_empty(), "{} holds nothing in {}", null, spot.scene);
            let stock_scene = template_of(&stock, spot.scene);
            assert_eq!(
                scene_view(&unyield(&composed, &null)),
                scene_view(&stock_scene),
                "{}",
                spot.scene
            );
        }
    }
    assert_hooks(&out, &pfx, &yields::KEYS);
    for t in [dials::TEMPLATE, dials::SPECTATOR, screens::REPLAY_UI] {
        let stock_t = Pkg::parse(&template_of(&stock, t)).unwrap();
        assert_eq!(
            ungrouped(&one_copy(&out, t), &pfx),
            crate::ui::node::canonical(&stock_t.root),
            "{t}"
        );
    }
    let dial = element_paths(&one_copy(&out, dials::F125.scene));
    for part in dials::STOCK_KIDS {
        let want = format!("{}.{null}.{part}", dials::CUSTOMISATION);
        assert!(dial.contains(&want), "{want}");
    }
    let replay = element_paths(&one_copy(&out, screens::REPLAY_SCENE));
    assert!(replay.contains(&format!("{}.replay_interface", screens::REPLAY_GROUP)));
    assert!(replay.contains(&format!(
        "{}.{null}.{}",
        screens::REPLAY_GROUP,
        screens::REPLAY_TABLE
    )));
    let spect = element_paths(&one_copy(&out, screens::SPECT_SCREEN));
    assert!(spect.contains(&format!("{null}.{}", screens::SPECT_HEADER_EL)));
    assert!(spect.contains(&"dial_complete".to_owned()));
}

fn season_inputs(dir: &Path) -> Vec<Input> {
    let flag_node = "hud_left_notification_dynamic.osd_visibility_option.left_notification.left_message";
    let tower = |addon: &str| {
        let mut v = tower_record();
        let o = v.as_object_mut().unwrap();
        o.remove("restyle");
        o.insert(
            "stock_nodes".into(),
            serde_json::json!([
                { "template": screens::LAPC_PARENT, "scene": screens::LAPC_SCENE,
                  "hide": ["car_category_logo", "session_icon", "centre_lap_value", "separator_bg", "separator_bg_top", "centre_formation"],
                  "wrap": ["edge_base", "edge_border", "lap_bg", "lockup"],
                  "while": { "view_model": "view_model_data.hud_tt_show" } },
                { "template": flag_node, "hide": ["boundless"], "while": { "view_model": "view_model_data.hud_tt_show" } }
            ]),
        );
        Input {
            addon: addon.into(),
            key: "tower".into(),
            dir: dir.to_path_buf(),
            record: HudElement::parse(&v).unwrap(),
        }
    };
    let onboard = |addon: &str| {
        Input {
        addon: addon.into(),
        key: "onboard".into(),
        dir: dir.to_path_buf(),
        record: HudElement::parse(&serde_json::json!({
            "dial": "f1_25",
            "stock": "hide",
            "switch": { "view_model": format!("view_model_data.hud_{}_dial", addon.replace('.', "_")), "default": false },
            "sprites": { "base": { "png": "art/base.png", "bounds": [-60, -20, 60, 20] } },
            "elements": [ { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 } ]
        }))
        .unwrap(),
    }
    };
    vec![
        tower("two.season"),
        onboard("two.season"),
        tower("one.season"),
        onboard("one.season"),
    ]
}

const SEASON_PARTS: [&str; 6] = [
    "f1_25",
    "standings",
    "replay",
    "spectator",
    "lap_counter",
    "flag_box",
];

#[test]
fn a_yield_holds_every_season_record_in_either_load_order() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("yield-seasons");
    let seasons = season_inputs(&dir);
    let y = yield_input("zz.broadcast", &SEASON_PARTS);
    let pfx = prefix_for("zz.broadcast", "yield", None).unwrap();
    let null = yields::null_name(&pfx);
    let without = compose(&stock, &seasons).unwrap();
    let mut first = vec![y.clone()];
    first.extend(seasons.iter().cloned());
    let mut last = seasons.clone();
    last.push(y.clone());
    let a = compose(&stock, &first).unwrap();
    let b = compose(&stock, &last).unwrap();
    for out in [&a, &b] {
        assert!(check::uncovered(&stock, &out.archives).unwrap().is_empty());
        assert_hooks(out, &pfx, &SEASON_PARTS);
        for part in SEASON_PARTS {
            for spot in yields::part(part).unwrap().spots {
                let with = one_copy(out, spot.scene);
                let paths = element_paths(&with);
                for p in paths.iter().filter(|p| p.contains(".s") || p.contains("season")) {
                    let leaf = p.rsplit('.').next().unwrap();
                    if leaf.starts_with("one_season") || leaf.starts_with("two_season") {
                        let held = p
                            .split('.')
                            .any(|s| s == null || s.starts_with(&format!("{null}_")));
                        let around = paths
                            .iter()
                            .any(|q| q.starts_with(&format!("{p}.")) && q.ends_with(&format!(".{null}")));
                        assert!(held || around, "{p} is outside {null} in {}", spot.scene);
                    }
                }
                let before = match copies(&without).remove(spot.scene) {
                    Some(c) => c[0].clone(),
                    None => template_of(&stock, spot.scene),
                };
                assert_eq!(
                    scene_view(&unyield(&with, &null)),
                    scene_view(&before),
                    "{}",
                    spot.scene
                );
            }
        }
        for t in [dials::TEMPLATE, dials::SPECTATOR, screens::REPLAY_UI] {
            let before = match copies(&without).remove(t) {
                Some(c) => c[0].clone(),
                None => template_of(&stock, t),
            };
            assert_eq!(
                ungrouped(&one_copy(out, t), &pfx),
                crate::ui::node::canonical(&Pkg::parse(&before).unwrap().root),
                "{t}"
            );
        }
    }
    let yielded = |out: &Output| -> BTreeMap<String, Vec<u8>> {
        copies(out)
            .into_iter()
            .filter(|(n, _)| {
                SEASON_PARTS
                    .iter()
                    .any(|p| yields::part(p).unwrap().spots.iter().any(|s| s.scene == n))
            })
            .map(|(n, c)| (n, c[0].clone()))
            .collect()
    };
    assert_eq!(
        yielded(&a).keys().collect::<Vec<_>>(),
        yielded(&b).keys().collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(dir).ok();
}

/// A stock with some resources replaced whole, as another addon's archive change leaves them.
struct Replaced<'a> {
    inner: &'a GameStock,
    files: BTreeMap<String, Vec<u8>>,
}

impl Stock for Replaced<'_> {
    fn read(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        match self.files.get(resource) {
            Some(b) => Ok(vec![b.clone()]),
            None => self.inner.read(erp, resource),
        }
    }

    fn layout(&self, name: &str, kind: &str, fragments: usize) -> Result<Resource> {
        self.inner.layout(name, kind, fragments)
    }

    fn header(&self) -> Result<([u8; swofty_formats::erp::HEADER_LEN], u32)> {
        self.inner.header()
    }

    fn carriers(&self, names: &[&str]) -> Result<BTreeMap<String, Vec<String>>> {
        self.inner.carriers(names)
    }

    fn game_copy(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        self.inner.read(erp, resource)
    }
}

#[test]
fn a_yield_holds_another_addons_whole_scene_and_template() {
    let Some(stock) = game() else { return };
    let dir = addon_dir("yield-whole");
    let halo = Input {
        addon: "nick.halo".into(),
        key: "halo".into(),
        dir: dir.clone(),
        record: HudElement::parse(&serde_json::json!({
            "dial": "f1_25",
            "stock": "hide",
            "prefix": "halo",
            "sprites": { "base": { "png": "art/base.png", "bounds": [-60, -20, 60, 20] } },
            "elements": [ { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 } ]
        }))
        .unwrap(),
    };
    let theirs = compose(&stock, &[halo]).unwrap();
    let scene = one_copy(&theirs, dials::F125.scene);
    let tmpl = one_copy(&theirs, dials::TEMPLATE);
    let replaced = Replaced {
        inner: &stock,
        files: BTreeMap::from([
            (dials::F125.scene.to_owned(), scene.clone()),
            (dials::TEMPLATE.to_owned(), tmpl.clone()),
        ]),
    };
    let pfx = prefix_for("zz.broadcast", "yield", None).unwrap();
    let null = yields::null_name(&pfx);
    let out = compose(&replaced, &[yield_input("zz.broadcast", &["f1_25"])]).unwrap();
    let with = one_copy(&out, dials::F125.scene);
    let paths = element_paths(&with);
    let kids: Vec<&String> = paths
        .iter()
        .filter(|p| p.starts_with(&format!("{}.", dials::CUSTOMISATION)) && p.matches('.').count() == 1)
        .collect();
    assert_eq!(kids, [&format!("{}.{null}", dials::CUSTOMISATION)]);
    assert!(paths.contains(&format!("{}.{null}.halo", dials::CUSTOMISATION)));
    assert_eq!(scene_view(&unyield(&with, &null)), scene_view(&scene));
    let t = one_copy(&out, dials::TEMPLATE);
    assert_eq!(
        ungrouped(&t, &pfx),
        crate::ui::node::canonical(&Pkg::parse(&tmpl).unwrap().root)
    );
    let tp = Pkg::parse(&t).unwrap();
    assert!(crate::ui::node::find_path(&tp.root, &format!("{}.halo_ctl", dials::F125.race_parent)).is_ok());
    assert_hooks(&out, &pfx, &["f1_25"]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn two_yields_on_one_part_nest_their_nulls_in_prefix_order() {
    let Some(stock) = game() else { return };
    let parts = ["f1_25", "standings"];
    let inputs = [
        yield_input("swofty.b-yielder", &parts),
        yield_input("swofty.a-yielder", &parts),
    ];
    let pfx: Vec<String> = ["swofty.a-yielder", "swofty.b-yielder"]
        .iter()
        .map(|a| prefix_for(a, "yield", None).unwrap())
        .collect();
    let (inner, outer) = (yields::null_name(&pfx[0]), yields::null_name(&pfx[1]));
    let out = compose(&stock, &inputs).unwrap();
    for part in parts {
        let spot = &yields::part(part).unwrap().spots[0];
        let paths = element_paths(&one_copy(&out, spot.scene));
        let at = |name: &str| paths.iter().find(|p| p.rsplit('.').next() == Some(name)).cloned();
        let outer_at = at(&outer).unwrap_or_else(|| panic!("{part}: no {outer}"));
        assert_eq!(at(&inner), Some(format!("{outer_at}.{inner}")), "{part}");
        let kids = |of: &str| {
            paths
                .iter()
                .filter(|p| {
                    p.strip_prefix(&format!("{of}."))
                        .is_some_and(|r| !r.contains('.'))
                })
                .count()
        };
        assert_eq!(kids(&outer_at), 1, "{part}: {outer} holds more than {inner}");
        assert!(
            kids(&format!("{outer_at}.{inner}")) > 0,
            "{part}: {inner} holds nothing"
        );
    }
    assert_hooks(&out, &pfx[0], &parts);
    assert_hooks(&out, &pfx[1], &parts);
}

#[test]
fn a_yield_record_takes_known_parts_and_nothing_to_draw() {
    let bad = [
        serde_json::json!({ "yield": { "parts": ["tyres"], "while": { "view_model": YIELD_LEAF } } }),
        serde_json::json!({ "yield": { "parts": [], "while": { "view_model": YIELD_LEAF } } }),
        serde_json::json!({ "yield": { "parts": ["f1_25"], "while": { "game": "revs" } } }),
        serde_json::json!({ "yield": { "parts": ["f1_25"], "while": { "view_model": YIELD_LEAF } }, "elements": [] , "stock": "hide" }),
        serde_json::json!({ "yield": { "parts": ["f1_25"], "while": { "view_model": YIELD_LEAF } }, "dial": "f1_25" }),
    ];
    for v in bad {
        assert!(HudElement::parse(&v).is_err(), "{v}");
    }
    let ok = HudElement::parse(&serde_json::json!({
        "prefix": "fbo_dial",
        "yield": { "parts": ["f1_25", "f1_26", "f2"], "while": { "view_model": YIELD_LEAF } }
    }))
    .unwrap();
    assert_eq!(ok.yields.unwrap().parts.len(), 3);
}
