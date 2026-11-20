use swofty_formats::hash::fnv1a64;

use super::*;
use crate::ops::DbOps;

const LIST: &str = "unlockable_content.unlockable_items.unlockable_item_list_2";
const LIST_1: &str = "unlockable_content.unlockable_items.unlockable_item_list_1";
const LIVERIES: &str = "unlockable_content.unlockable_items.unlockable_item_list_2.folder_0";
const DECALS: &str = "unlockable_content.unlockable_items.unlockable_item_list_2.folder_1";
const LIST_1_DECALS: &str = "unlockable_content.unlockable_items.unlockable_item_list_1.folder_0";
const SE: &str = "unlockable_se_livery";
const DECAL: &str = "unlockable_decal_set";

type UnlockRows = &'static [(&'static str, i64)];

// A stock database with unlock list 2 (special liveries 5279 and 5280 in folder 0, decal sets 40 and
// 41 in folder 1) and unlock list 1 (decal set 3552 in folder 0), as Steam build 26566308 has them.
fn unlock_db() -> Gvdb {
    let mut records = vec![
        Record::new(&format!("{LIST}._folder_instancecount_"), 0, "2"),
        Record::new(&format!("{LIST_1}._folder_instancecount_"), 0, "1"),
    ];
    let tables: [(&str, &str, UnlockRows); 3] = [
        (
            LIVERIES,
            SE,
            &[
                ("force_india_monaco_2026", 5279),
                ("toro_rosso_barcelona_26", 5280),
            ],
        ),
        (DECALS, DECAL, &[("avolo_s1", 40), ("avolo_s2", 41)]),
        (LIST_1_DECALS, DECAL, &[("avolo_s1", 3552)]),
    ];
    for (folder, kind, rows) in tables {
        records.push(Record::new(
            &format!("{folder}._{kind}_instancecount_"),
            0,
            rows.len().to_string(),
        ));
        for (i, (label, id)) in rows.iter().enumerate() {
            records.push(Record::new(&format!("{folder}.{kind}_{i}._metadata_"), 4, *label));
            records.push(Record::new(
                &format!("{folder}.{kind}_{i}.customisation_item"),
                4,
                *label,
            ));
            records.push(Record::new(
                &format!("{folder}.{kind}_{i}.statId"),
                0,
                id.to_string(),
            ));
        }
    }
    Gvdb::new(records)
}

struct Unlock<'a> {
    folder: &'a str,
    kind: &'a str,
    label: &'a str,
    id: Option<i64>,
}

fn row<'a>(folder: &'a str, kind: &'a str, label: &'a str, id: Option<i64>) -> Unlock<'a> {
    Unlock {
        folder,
        kind,
        label,
        id,
    }
}

struct UnlockBuilt {
    // `<folder>.<kind>/<label>` to the added row's statId.
    ids: BTreeMap<String, i64>,
    store: Store,
    lines: Vec<String>,
    warnings: Vec<String>,
}

const UNLOCK_FIELDS: [&str; 3] = ["_metadata_", "customisation_item", "statId"];

fn unlock_build(addons: &[(&str, &[Unlock])], store: Store) -> UnlockBuilt {
    let mut gvdb = unlock_db();
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    for (addon, rows) in addons {
        let mut fields = BTreeMap::new();
        for r in *rows {
            fields.insert(
                format!("{}.{}", r.folder, r.kind),
                UNLOCK_FIELDS.map(String::from).to_vec(),
            );
            let id = r.id.map_or(String::new(), |id| format!(r#","statId":"{id}""#));
            let json = format!(
                r#"{{"op":"db.add-row","table":"{f}","from":"{f}.{k}_0","set":{{"_metadata_":"{l}","customisation_item":"{l}"{id}}}}}"#,
                f = r.folder,
                k = r.kind,
                l = r.label
            );
            p.push(Op {
                op: serde_json::from_str(&json).unwrap(),
                origin: Origin {
                    addon: (*addon).to_owned(),
                    item: None,
                },
            });
        }
        p.add_fields(addon, fields);
    }
    p.keep_row_ids(store);
    let stock_keys: HashSet<u64> = gvdb.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut gvdb, &stock_keys, &HashSet::new(), &mut report)
        .unwrap();
    let mut ids = BTreeMap::new();
    for folder in [LIVERIES, DECALS, LIST_1_DECALS] {
        for kind in [SE, DECAL] {
            for i in 0..gvdb.instance_count(folder, kind).unwrap_or(0) {
                let name = format!("{folder}.{kind}_{i}");
                if stock_keys.contains(&fnv1a64(format!("{name}._metadata_"))) {
                    continue;
                }
                let label = gvdb.get_text(&format!("{name}._metadata_")).unwrap();
                let id = gvdb.get_text(&format!("{name}.statId")).unwrap();
                ids.insert(format!("{folder}.{kind}/{label}"), id.parse().unwrap());
            }
        }
    }
    UnlockBuilt {
        ids,
        store: p.take_row_ids().unwrap(),
        lines: report.lines,
        // The builtin key catalogue doesn't list unlockable_se_livery's keys: not an id warning.
        warnings: report
            .warnings
            .into_iter()
            .filter(|w| !w.contains("the key catalogue has no entry"))
            .collect(),
    }
}

fn at(folder: &str, kind: &str, label: &str) -> String {
    format!("{folder}.{kind}/{label}")
}

fn unlock_key(addon: &str, list: &str, kind: &str, label: &str) -> String {
    format!("{addon}/{list}.unlockable_item/{kind}/{label}")
}

#[test]
fn an_unlock_row_without_a_stat_id_gets_its_own_from_10000_across_folders_and_kinds() {
    let rows = [
        row(LIVERIES, SE, "my_livery", None),
        row(DECALS, DECAL, "acme_s1", None),
        row(DECALS, DECAL, "acme_s2", None),
    ];
    let built = unlock_build(&[("a", &rows)], Store::default());
    assert_eq!(built.ids[&at(LIVERIES, SE, "my_livery")], 10000);
    assert_eq!(built.ids[&at(DECALS, DECAL, "acme_s1")], 10001);
    assert_eq!(built.ids[&at(DECALS, DECAL, "acme_s2")], 10002);
    let line =
        format!("add-row {LIVERIES}.{SE}_2 (from {LIVERIES}.{SE}_0): statId 10000 (the source's is 5279)");
    assert!(
        built.lines.iter().any(|l| l.ends_with(&line)),
        "{:#?}",
        built.lines
    );
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    assert_eq!(
        built.store.get(&unlock_key("a", LIST, SE, "my_livery")),
        Some(10000)
    );
}

#[test]
fn an_unlock_rows_kept_stat_id_holds_whatever_else_the_build_adds() {
    let a = [row(LIVERIES, SE, "a_livery", None)];
    let b = [
        row(DECALS, DECAL, "b_decal", None),
        row(LIVERIES, SE, "b_livery", None),
    ];
    let first = unlock_build(&[("a", &a), ("b", &b)], Store::default());
    assert_eq!(first.ids[&at(LIVERIES, SE, "a_livery")], 10000);
    assert_eq!(first.ids[&at(DECALS, DECAL, "b_decal")], 10001);
    assert_eq!(first.ids[&at(LIVERIES, SE, "b_livery")], 10002);
    let alone = unlock_build(&[("b", &b)], first.store);
    assert_eq!(alone.ids[&at(DECALS, DECAL, "b_decal")], 10001);
    assert_eq!(alone.ids[&at(LIVERIES, SE, "b_livery")], 10002);
    assert!(
        alone
            .lines
            .iter()
            .any(|l| l.ends_with("statId 10001 (the source's is 40; kept from row-ids.json)")),
        "{:#?}",
        alone.lines
    );
    let c = [row(LIVERIES, SE, "c_livery", None)];
    let swapped = unlock_build(&[("c", &c), ("b", &b), ("a", &a)], alone.store);
    assert_eq!(swapped.ids[&at(LIVERIES, SE, "a_livery")], 10000);
    assert_eq!(swapped.ids[&at(DECALS, DECAL, "b_decal")], 10001);
    assert_eq!(swapped.ids[&at(LIVERIES, SE, "b_livery")], 10002);
    // c comes first now, but the ids kept for a and b stay theirs.
    assert_eq!(swapped.ids[&at(LIVERIES, SE, "c_livery")], 10003);
}

#[test]
fn each_unlock_list_has_its_own_ids_and_keep_copies_the_source_id() {
    let rows = [
        row(DECALS, DECAL, "acme_s1", None),
        row(LIST_1_DECALS, DECAL, "acme_s1", None),
    ];
    let built = unlock_build(&[("a", &rows)], Store::default());
    assert_eq!(built.ids[&at(DECALS, DECAL, "acme_s1")], 10000);
    assert_eq!(built.ids[&at(LIST_1_DECALS, DECAL, "acme_s1")], 10000);
    let mut gvdb = unlock_db();
    let stock: HashSet<u64> = gvdb.records.iter().map(|r| r.key).collect();
    let keep: DbOps = serde_json::from_str(&format!(
        r#"{{"schema_version":1,"fields":{{"{DECALS}.{DECAL}":["_metadata_","customisation_item","statId"]}},
        "ops":[{{"op":"add-row","table":"{DECALS}","from":"{DECALS}.{DECAL}_0","stat_id":"keep","set":{{"_metadata_":"x"}}}}]}}"#
    ))
    .unwrap();
    crate::ops::apply_db_ops(&mut gvdb, &keep, &stock).unwrap();
    assert_eq!(
        gvdb.get_text(&format!("{DECALS}.{DECAL}_2.statId")).as_deref(),
        Some("40")
    );
}

#[test]
fn an_explicit_unlock_stat_id_the_game_now_holds_moves_and_stays_moved() {
    // An addon that took 5280, the highest stock id + 1, before Steam build 26566308 gave 5280 to
    // toro_rosso_barcelona_26.
    let rows = [row(LIVERIES, SE, "generic_livery", Some(5280))];
    let built = unlock_build(&[("lm", &rows)], Store::default());
    assert_eq!(built.ids[&at(LIVERIES, SE, "generic_livery")], 10000);
    assert_eq!(
        built.warnings,
        [
            "lm: statId 5280 of generic_livery is taken by toro_rosso_barcelona_26 from the game; the build gave it statId 10000. The addon's author should leave statId out of add-row"
        ]
    );
    let line = format!("statId 5280 is taken by {LIVERIES}.{SE}_1 (toro_rosso_barcelona_26); statId 10000");
    assert!(
        built.lines.iter().any(|l| l.ends_with(&line)),
        "{:#?}",
        built.lines
    );
    let again = unlock_build(&[("lm", &rows)], built.store);
    assert_eq!(again.ids[&at(LIVERIES, SE, "generic_livery")], 10000);
    assert!(
        again
            .lines
            .iter()
            .any(|l| l.ends_with("statId 10000 (kept from row-ids.json)")),
        "{:#?}",
        again.lines
    );
}

#[test]
fn an_explicit_unlock_stat_id_an_earlier_addon_holds_in_another_folder_moves() {
    let first = [row(DECALS, DECAL, "f1_sticker", Some(5281))];
    let second = [
        row(LIVERIES, SE, "generic_livery", Some(5281)),
        row(LIVERIES, SE, "free", Some(6000)),
    ];
    let built = unlock_build(&[("stickers", &first), ("lm", &second)], Store::default());
    assert_eq!(built.ids[&at(DECALS, DECAL, "f1_sticker")], 5281);
    assert_eq!(built.ids[&at(LIVERIES, SE, "generic_livery")], 10000);
    assert_eq!(built.ids[&at(LIVERIES, SE, "free")], 6000);
    assert_eq!(
        built.warnings,
        [
            "lm: statId 5281 of generic_livery is taken by f1_sticker from stickers; the build gave it statId 10000. The addon's author should leave statId out of add-row"
        ]
    );
}

#[test]
fn a_fresh_unlock_stat_id_skips_ids_later_addons_set_and_the_cores_livery_unlock_ids() {
    let data = data_dir("unlock-pins");
    put(
        &reports::path(&data, reports::IDS),
        r#"{"version":1,"ids":{"livery.unlock":{"x/livery":10001}}}"#,
    );
    let (store, _) = Store::load(&data, None, "p", None);
    let early = [
        row(DECALS, DECAL, "early", None),
        row(DECALS, DECAL, "early_2", None),
    ];
    let late = [row(LIVERIES, SE, "late", Some(10000))];
    let built = unlock_build(&[("a", &early), ("b", &late)], store);
    assert_eq!(built.ids[&at(DECALS, DECAL, "early")], 10002);
    assert_eq!(built.ids[&at(DECALS, DECAL, "early_2")], 10003);
    assert_eq!(built.ids[&at(LIVERIES, SE, "late")], 10000);
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    built.store.save(&data).unwrap();
    let kept: Vec<(String, String, i64)> = kept_rows(&data, "a")
        .unwrap()
        .into_iter()
        .map(|r| (r.table, r.label, r.id))
        .collect();
    let table = format!("{LIST}.unlockable_item");
    assert_eq!(
        kept,
        [
            (table.clone(), "unlockable_decal_set/early".to_owned(), 10002),
            (table, "unlockable_decal_set/early_2".to_owned(), 10003),
        ]
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn a_selector_on_a_moved_unlock_stat_id_finds_the_new_row_in_the_same_file() {
    let mut gvdb = unlock_db();
    let stock: HashSet<u64> = gvdb.records.iter().map(|r| r.key).collect();
    let ops: DbOps = serde_json::from_str(&format!(
        r#"{{"schema_version":1,"fields":{{"{LIVERIES}.{SE}":["_metadata_","customisation_item","statId"]}},"ops":[
        {{"op":"add-row","table":"{LIVERIES}","from":"{LIVERIES}.{SE}_0","set":{{"_metadata_":"mine","statId":"5280"}}}},
        {{"op":"set","select":"{LIVERIES}.{SE}[statId=5280]","field":"customisation_item","value":"moved"}}]}}"#
    ))
    .unwrap();
    let mut report = DbReport::default();
    crate::ops::apply_db_ops_report(&mut gvdb, &ops, &stock, &mut report).unwrap();
    let text = |f: &str| gvdb.get_text(&format!("{LIVERIES}.{f}"));
    assert_eq!(text(&format!("{SE}_2.statId")).as_deref(), Some("10000"));
    assert_eq!(
        text(&format!("{SE}_2.customisation_item")).as_deref(),
        Some("moved")
    );
    assert_eq!(
        text(&format!("{SE}_1.customisation_item")).as_deref(),
        Some("toro_rosso_barcelona_26")
    );
    assert_eq!(report.moved_stat_ids.len(), 1);
    assert_eq!(report.moved_stat_ids[0].field, "statId");
}
