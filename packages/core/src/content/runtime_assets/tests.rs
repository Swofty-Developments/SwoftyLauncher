use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use swofty_formats::erp::{Erp, Resource};
use swofty_formats::gvdb::Gvdb;
use swofty_formats::lng::Lng;

use crate::asset::{Kind, Op, Source};

use super::assets::*;
use super::db::*;
use super::*;
use crate::asset::Origin;
use swofty_formats::erp::{Fragment as ErpFragment, STORED};
use swofty_formats::gvdb::Record;

fn op(addon: &str, json: &str) -> Op {
    Op {
        op: serde_json::from_str(json).unwrap(),
        origin: Origin {
            addon: addon.into(),
            item: None,
        },
    }
}

fn fragment(name: &[u8; 4], flags: u32, data: &[u8]) -> ErpFragment {
    ErpFragment::new(*name, data.len() as u64, flags, STORED, data.to_vec())
}

#[test]
fn a_build_without_database_edits_writes_no_assets_json_to_replay() {
    let out = std::env::temp_dir().join(format!("swofty-runtime-empty-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let mut with = Runtime::new(Vec::new());
    with.db(vec![DbEdit {
        hash: "0000000000000001".into(),
        tag: 0,
        value: "1".into(),
        retype: false,
    }]);
    let kept = with.finish(&out).unwrap().unwrap();
    assert!(out.join(ASSETS_FILE).is_file() && kept.files.contains_key(ASSETS_FILE));
    let none = out.join("next");
    assert!(Runtime::new(Vec::new()).finish(&none).unwrap().is_none());
    assert!(!none.join(ASSETS_FILE).exists());
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn writes_assets_json_in_the_core_schema() {
    let dir = std::env::temp_dir().join(format!("swofty-runtime-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let addon = dir.join("addon");
    std::fs::create_dir_all(&addon).unwrap();
    let mut table = Lng::default();
    table.set("lng_a", "from table");
    table.set("lng_b", "kept");
    std::fs::write(addon.join("eng.lng"), table.to_bytes()).unwrap();
    std::fs::write(addon.join("one.dds"), b"one").unwrap();
    std::fs::write(addon.join("two.dds"), b"two").unwrap();
    let resolve = |_: &Op, s: &Source| Ok(s.resolve(&addon)?);
    let mut rt = Runtime::new(vec![Kind::File]);
    for (id, json) in [
        (
            "a",
            r#"{"op":"group.register","group":"asset_groups/x/y.assetgroup","erp":"2025_asset_groups/x/y.erp","deps":["asset_groups/z.assetgroup"]}"#,
        ),
        ("a", r#"{"op":"lang.table","language":"eng","source":"eng.lng"}"#),
        (
            "b",
            r#"{"op":"lang.set","language":"eng","key":"lng_a","value":"from set"}"#,
        ),
        (
            "b",
            r#"{"op":"lang.set","language":"*","key":"lng_c","value":"every language"}"#,
        ),
        (
            "a",
            r#"{"op":"file.put","source":"one.dds","target":"livery/s.dds"}"#,
        ),
        (
            "b",
            r#"{"op":"file.put","source":"two.dds","target":"livery/s.dds"}"#,
        ),
    ] {
        rt.push(op(id, json), &resolve).unwrap();
    }
    let mut kind = [0; 16];
    kind[..13].copy_from_slice(b"GfxSurfaceRes");
    let res = |name: &str, fragments: Vec<ErpFragment>| Resource {
        name: name.into(),
        kind,
        unk: 1u32.to_le_bytes().to_vec(),
        fragments,
        hash: [0; 16],
    };
    let x = "eaid://ui_package/x.tif.image";
    let y = "eaid://ui_package/y.tif.image";
    let z = "eaid://ui_package/z.tif.image";
    let mut header = [0; 0x30];
    header[..4].copy_from_slice(b"ERPK");
    header[4] = 3;
    let stock = Erp {
        header,
        version: 3,
        resources: vec![
            res(
                x,
                vec![fragment(b"temp", 16, b"HEAD"), fragment(b"pixl", 0, b"STOCK!")],
            ),
            res(y, vec![fragment(b"temp", 16, b"ABCD")]),
        ],
    };
    let replaced = res(
        x,
        vec![fragment(b"temp", 16, b"HEAD"), fragment(b"pixl", 0, b"PIXELS")],
    );
    let resized = res(y, vec![fragment(b"temp", 16, b"ABCDE")]);
    let added = res(z, vec![fragment(b"temp", 16, b"NEW")]);
    let boot = "2025_asset_groups/ui_package/boot.erp";
    let other = "2025_asset_groups/ui_package/other.erp";
    let out = dir.join("overlay");
    let stale = out.join("swofty/assets/resources/0.0.bin");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, b"old").unwrap();
    let warnings = rt
        .resources(
            [
                (boot, &replaced, &stock, ""),
                (other, &replaced, &stock, ""),
                (boot, &resized, &stock, ""),
                (boot, &added, &stock, ""),
            ],
            &[],
            &[],
            &out,
        )
        .unwrap();
    assert_eq!(
        warnings,
        [format!(
            "runtime: {y} in {boot} is left out: fragment 0 is 5 bytes, the stock one 4; stock archives that bind to the replacement read it with the stock layout"
        )]
    );
    let mut base = Gvdb::default();
    base.records.push(Record::new("a.int", 0, "1"));
    let mut built = base.clone();
    built.set("a.int", "2", None).unwrap();
    built.set("a.new", "hello", Some(2)).unwrap();
    rt.db(db_edits(&base, &built).unwrap());
    let rt = rt.finish(&out).unwrap().unwrap();
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join(ASSETS_FILE)).unwrap()).unwrap();
    let hash = |p: &str| format!("{:016x}", swofty_formats::hash::fnv1a64(p));
    assert_eq!(
        written,
        serde_json::json!({
            "early_stage": true,
            "features": ["file"],
            "asset_groups": [{
                "group": "asset_groups/x/y.assetgroup",
                "erps": ["2025_asset_groups/x/y.erp"],
                "deps": ["asset_groups/z.assetgroup"],
                "origin": "a",
            }],
            "resource_erps": [
                { "file": "assets/resources/0.erp", "erps": [boot] },
                { "file": "assets/resources/1.erp", "erps": [boot, other] },
            ],
            "db_edits": [
                { "hash": hash("a.int"), "type": 0, "value": "2" },
                { "hash": hash("a.new"), "type": 2, "value": "hello" },
            ],
            "lang": {
                "*": { "table": "assets/lang/all.lng" },
                "eng": { "table": "assets/lang/eng.lng" },
            },
            "overlay": true,
        })
    );
    let eng = Lng::parse(&std::fs::read(out.join("swofty/assets/lang/eng.lng")).unwrap()).unwrap();
    assert_eq!(eng.get("lng_a"), Some("from set"));
    assert_eq!(eng.get("lng_b"), Some("kept"));
    let names = |n: u32| {
        let erp = Erp::parse(&std::fs::read(out.join(format!("swofty/assets/resources/{n}.erp"))).unwrap())
            .unwrap();
        assert_eq!((&erp.header[..8], erp.version), (&header[..8], 3));
        erp.resources
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    r.fragments.iter().map(|f| f.data().unwrap()).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(names(0), [(z.to_owned(), vec![b"NEW".to_vec()])]);
    assert_eq!(
        names(1),
        [(x.to_owned(), vec![b"HEAD".to_vec(), b"PIXELS".to_vec()])]
    );
    assert!(rt.summary().contains("2 resource(s)"));
    assert!(!stale.exists());
    let placed: Vec<&str> = rt.files.keys().map(String::as_str).collect();
    assert_eq!(
        placed,
        [
            "swofty/assets.json",
            "swofty/assets/lang/all.lng",
            "swofty/assets/lang/eng.lng",
            "swofty/assets/resources/0.erp",
            "swofty/assets/resources/1.erp",
            "swofty/overlay/livery/s.dds",
        ]
    );
    assert_eq!(rt.files["swofty/overlay/livery/s.dds"], addon.join("two.dds"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_default_applies_before_every_set_and_never_conflicts() {
    use crate::asset::Features;
    use crate::planner::Planner;
    let out = std::env::temp_dir().join(format!("swofty-runtime-default-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let features = Features {
        stable: BTreeSet::from([Kind::Lang]),
        ..Features::default()
    };
    let opt_in = BTreeSet::new();
    let mut planner = Planner::new(&features, &opt_in);
    for (id, json) in [
        (
            "a",
            r#"{"op":"lang.set","language":"eng","key":"lng_a","value":"set by a"}"#,
        ),
        (
            "b",
            r#"{"op":"lang.default","language":"eng","key":"lng_a","value":"default from b"}"#,
        ),
        (
            "b",
            r#"{"op":"lang.default","language":"eng","key":"lng_b","value":"default from b"}"#,
        ),
        (
            "c",
            r#"{"op":"lang.default","language":"eng","key":"lng_b","value":"default from c"}"#,
        ),
        (
            "c",
            r#"{"op":"lang.default","language":"*","key":"lng_c","value":"every language"}"#,
        ),
        (
            "c",
            r#"{"op":"lang.default","language":"eng","key":"lng_d","value":"default from c"}"#,
        ),
        (
            "d",
            r#"{"op":"lang.set","language":"eng","key":"lng_d","value":"set by d"}"#,
        ),
    ] {
        planner.push(op(id, json));
    }
    let planned = planner.finish();
    assert!(planned.conflicts.is_empty(), "{:?}", planned.conflicts);
    let mut rt = Runtime::new(Vec::new());
    for op in planned.runtime {
        rt.push(op, &|_: &Op, s: &Source| Ok(PathBuf::from(&s.path)))
            .unwrap();
    }
    rt.finish(&out).unwrap().unwrap();
    let table =
        |name: &str| Lng::parse(&std::fs::read(out.join("swofty/assets/lang").join(name)).unwrap()).unwrap();
    let eng = table("eng.lng");
    assert_eq!(
        ["lng_a", "lng_b", "lng_d"].map(|k| eng.get(k)),
        [Some("set by a"), Some("default from c"), Some("set by d")]
    );
    assert_eq!(table("all.lng").get("lng_c"), Some("every language"));
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn the_loaded_estimate_never_drops_below_the_stock_database() {
    assert_eq!(runtime_loaded_estimate(734_637), 742_857);
    assert_eq!(runtime_loaded_estimate(743_668), 743_668);
    let room = |staged| runtime_record_room(runtime_loaded_estimate(staged));
    assert!(room(734_637) < 100_127);
    assert!(room(743_668) < 100_127);
    assert_eq!(room(742_857), 838_860 - 742_857 - RUNTIME_RECORD_MARGIN);
}

#[test]
fn the_runtime_record_room_stops_before_the_game_database_grows() {
    assert_eq!(
        runtime_record_room(742_857),
        838_860 - 742_857 - RUNTIME_RECORD_MARGIN
    );
    assert_eq!(
        runtime_record_room(758_755),
        838_860 - 758_755 - RUNTIME_RECORD_MARGIN
    );
    assert_eq!(runtime_record_room(838_860), 0);
    assert_eq!(
        runtime_record_room(838_861),
        1_677_721 - 838_861 - RUNTIME_RECORD_MARGIN
    );
    assert_eq!(runtime_record_room(0), 0);
}

#[test]
fn database_edits_retype_and_refuse_removals() {
    let mut base = Gvdb::default();
    base.records.push(Record::new("a.x", 0, "1"));
    base.records.push(Record::new("a.y", 0, "1"));
    let mut built = base.clone();
    built.set("a.x", "1.5", Some(1)).unwrap();
    let edits = db_edits(&base, &built).unwrap();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].retype);
    built.remove("a.y");
    assert!(
        db_edits(&base, &built)
            .unwrap_err()
            .contains("1 record(s) are removed")
    );
    let mut empty = Runtime::new(vec![Kind::File]);
    empty.db(Vec::new());
    assert!(empty.finish(Path::new("/nonexistent")).unwrap().is_none());
}

#[test]
fn runtime_text_takes_what_the_core_applies() {
    for (tag, value) in [
        (11, ""),
        (11, "-1710619"),
        (0, ""),
        (0, "+5"),
        (1, ""),
        (1, "0.1"),
        (5, ""),
        (5, "True"),
        (12, ""),
        (12, "-1"),
        (2, ""),
        (13, ""),
    ] {
        assert_eq!(runtime_text(tag, value), Ok(()), "tag {tag} {value:?}");
    }
    for (tag, value) in [
        (11, "abc"),
        (0, "1.5E+07"),
        (0, "3000000000"),
        (0, "-"),
        (12, "-x"),
        (12, "18446744073709551616"),
        (1, "Level_0|DevelopmentLevel"),
        (1, "inf"),
        (5, "yes"),
        (16, ""),
    ] {
        assert!(runtime_text(tag, value).is_err(), "tag {tag} {value:?}");
    }
}

#[test]
fn database_edits_keep_empty_colours_and_refuse_text_the_core_ignores() {
    let mut base = Gvdb::default();
    base.records
        .push(Record::new("gloves.Gloves_0.colour_1_default", 11, ""));
    base.records.push(Record::new("modes.mode_0.prize", 0, "1.5E+07"));
    let mut built = base.clone();
    built
        .records
        .push(Record::new("gloves.Gloves_1.colour_1_default", 11, ""));
    let edits = db_edits(&base, &built).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!((edits[0].tag, edits[0].value.as_str()), (11, ""));
    built
        .records
        .push(Record::new("modes.mode_1.prize", 0, "1.5E+07"));
    let why = db_edits(&base, &built).unwrap_err();
    assert!(
        why.contains("swofty_core.dll would ignore record") && why.contains("\"1.5E+07\""),
        "{why}"
    );
}
