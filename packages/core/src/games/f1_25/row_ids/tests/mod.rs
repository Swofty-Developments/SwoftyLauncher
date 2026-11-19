use std::collections::BTreeSet;

use serde_json::Value;
use swofty_formats::gvdb::{Gvdb, Record, instance_count_path};

use super::*;
use crate::asset::{Features, Op, Origin};
use crate::ops::{DbReport, stat_id_ceiling};
use crate::planner::Planner;

mod aliases;
mod full_tables;
mod lend;
mod order;
mod owners;
mod registry;
mod seed;
mod tracks;
mod unlocks;

const VEHICLES: (&str, &str) = ("entrants.vehicles", "vehicle");
const PACKAGES: (&str, &str) = ("entrants.vehicles.vehicle_packages", "vehicle_package");

fn db((table, row): (&str, &str), stock: i64) -> Gvdb {
    Gvdb::new(vec![
        Record::new(&format!("{table}._{row}_instancecount_"), 0, "1"),
        Record::new(&format!("{table}.{row}_0._metadata_"), 4, "base"),
        Record::new(&format!("{table}.{row}_0.stat_id"), 0, stock.to_string()),
    ])
}

pub(super) struct Built {
    pub(super) ids: BTreeMap<String, i64>,
    pub(super) store: Store,
    pub(super) lines: Vec<String>,
    pub(super) left_out: BTreeMap<String, usize>,
}

fn try_build(
    table: (&str, &str),
    stock: i64,
    addons: &[(&str, &[&str])],
    store: Store,
) -> crate::Result<Built> {
    try_build_on(db(table, stock), table, addons, store)
}

pub(super) fn try_build_on(
    mut gvdb: Gvdb,
    (table, row): (&str, &str),
    addons: &[(&str, &[&str])],
    store: Store,
) -> crate::Result<Built> {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    for (addon, labels) in addons {
        p.add_fields(
            addon,
            BTreeMap::from([(
                format!("{table}.{row}"),
                vec!["_metadata_".to_owned(), "stat_id".to_owned()],
            )]),
        );
        for label in *labels {
            let (label, set_id) = match label.split_once('@') {
                Some((l, id)) => (l, format!(r#","stat_id":"{id}""#)),
                None => (*label, String::new()),
            };
            let json = format!(
                r#"{{"op":"db.add-row","table":"{table}","from":"{table}.{row}_0","set":{{"_metadata_":"{label}"{set_id}}}}}"#
            );
            p.push(Op {
                op: serde_json::from_str(&json).unwrap(),
                origin: Origin {
                    addon: (*addon).to_owned(),
                    item: None,
                },
            });
        }
    }
    p.keep_row_ids(store);
    let first = gvdb.instance_count(table, row).unwrap();
    let stock_keys = gvdb.records.iter().map(|r| r.key).collect();
    let mut report = DbReport::default();
    p.apply_db_report(&mut gvdb, &stock_keys, &HashSet::new(), &mut report)?;
    let n = gvdb.instance_count(table, row).unwrap();
    let field = stat_id_field(table, row);
    let ids = (first..n)
        .map(|i| {
            let label = gvdb.get_text(&format!("{table}.{row}_{i}._metadata_")).unwrap();
            let id = gvdb.get_text(&format!("{table}.{row}_{i}.{field}")).unwrap();
            (label, id.parse().unwrap())
        })
        .collect();
    let left_out = p.take_id_left_out();
    Ok(Built {
        ids,
        store: p.take_row_ids().unwrap(),
        lines: report.lines,
        left_out,
    })
}

fn build(table: (&str, &str), stock: i64, addons: &[(&str, &[&str])], store: Store) -> Built {
    try_build(table, stock, addons, store).unwrap()
}

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-row-ids-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn key(addon: &str, (table, row): (&str, &str), label: &str) -> String {
    format!("{addon}/{table}.{row}/{label}")
}

pub(super) const TEAMS: (&str, &str) = ("entrants.teams", "team");

const STOCK_TRACKS: i64 = 46;

fn track_build(store: &mut Store, on: &[(&str, &str)], seeds: &[(&str, i64)]) -> BTreeMap<String, i64> {
    store.building(on.iter().map(|(a, _)| (*a).to_owned()));
    let mut rows: BTreeSet<i64> = (0..STOCK_TRACKS).collect();
    let mut out = BTreeMap::new();
    for (addon, code) in on {
        let seed = seeds.iter().find(|(c, _)| c == code).map(|(_, id)| *id);
        let top = rows.iter().max().map_or(0, |m| m + 1);
        let held = rows.clone();
        let (id, _) = store
            .track_stat_id(addon, code, seed, top, &|id| held.contains(&id))
            .unwrap();
        rows.insert(id);
        out.insert((*code).to_owned(), id);
    }
    out
}

fn sixteen() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = (0..15).map(|i| (format!("mod.t{i}"), format!("t{i}"))).collect();
    v.insert(9, ("mod.sokol".to_owned(), "sokol".to_owned()));
    v
}

fn refs(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(a, c)| (a.as_str(), c.as_str())).collect()
}

fn saved(data: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path(data)).unwrap()).unwrap()
}

const DRIVERS: (&str, &str) = ("entrants.drivers", "driver");

fn career_holds_247(data: &Path) {
    put(
        &reports::path(data, reports::IDS),
        r#"{"version":1,"ids":{"driver.identity":{"sw.2022/sw22_vettel":{"id":247,"since":"2026-11-12"}},
            "vehicle.package":{"sw.2022/sw_2022":{"id":247,"since":"2026-11-12"}}},
            "careers":{"dc-1":{"driver.identity":{"sw.2022/sw22_vettel":247}}}}"#,
    );
}

fn pinned_vehicles(upto: i64) -> String {
    let records: Vec<String> = (1..=upto).map(|id| format!(r#""a/car_{id}":{id}"#)).collect();
    format!(r#"{{"version":1,"ids":{{"vehicle":{{{}}}}}}}"#, records.join(","))
}

fn leaves_out_rows(labels: &[&str], career: Option<i64>) -> crate::Result<(BTreeMap<String, i64>, Store)> {
    let mut store = Store::default();
    for (i, id) in (251..=254).enumerate() {
        store.put(&key("b", PACKAGES, &format!("old{i}")), id, IdFrom::Seeded);
    }
    if let Some(id) = career {
        store
            .career_held
            .entry("entrants.vehicles.vehicle_packages.vehicle_package".to_owned())
            .or_default()
            .insert(id);
    }
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    let (table, row) = PACKAGES;
    p.add_fields(
        "a",
        BTreeMap::from([(
            format!("{table}.{row}"),
            vec!["_metadata_".to_owned(), "stat_id".to_owned()],
        )]),
    );
    for label in labels {
        let json = format!(
            r#"{{"op":"db.add-row","table":"{table}","from":"{table}.{row}_0","set":{{"_metadata_":"{label}"}}}}"#
        );
        p.push(Op {
            op: serde_json::from_str(&json).unwrap(),
            origin: Origin {
                addon: "a".to_owned(),
                item: None,
            },
        });
    }
    p.push(Op {
        op: serde_json::from_str(&format!(
            r#"{{"op":"db.set","select":"{table}.{row}_0","field":"_metadata_","value":"base"}}"#
        ))
        .unwrap(),
        origin: Origin {
            addon: "b".to_owned(),
            item: None,
        },
    });
    p.keep_row_ids(store);
    let mut gvdb = db(PACKAGES, 250);
    let stock_keys = gvdb.records.iter().map(|r| r.key).collect();
    p.apply_db_report(&mut gvdb, &stock_keys, &HashSet::new(), &mut DbReport::default())?;
    let n = gvdb.instance_count(table, row).unwrap();
    let ids = (1..n)
        .map(|i| {
            let label = gvdb.get_text(&format!("{table}.{row}_{i}._metadata_")).unwrap();
            let id = gvdb.get_text(&format!("{table}.{row}_{i}.stat_id")).unwrap();
            (label, id.parse().unwrap())
        })
        .collect();
    Ok((ids, p.take_row_ids().unwrap()))
}

const MODES: (&str, &str) = ("game_setup.game_setup_modes", "game_setup_mode");
const ENTRANT_MODES: (&str, &str) = ("entrants.entrant_setup_modes", "entrant_setup_mode");

// A table of stock rows holding the ids 0 to `top`.
pub(super) fn table_to((table, row): (&str, &str), top: i64) -> Gvdb {
    let field = stat_id_field(table, row);
    let mut records = vec![Record::new(
        &format!("{table}._{row}_instancecount_"),
        0,
        (top + 1).to_string(),
    )];
    for i in 0..=top {
        records.push(Record::new(
            &format!("{table}.{row}_{i}._metadata_"),
            4,
            format!("stock_{i}"),
        ));
        records.push(Record::new(
            &format!("{table}.{row}_{i}.{field}"),
            0,
            i.to_string(),
        ));
    }
    Gvdb::new(records)
}

fn host_ids(rows: &[(&str, &str, i64)]) -> Overrides {
    let mut o = Overrides::default();
    for (addon, label, id) in rows {
        o.rows
            .entry((*addon).to_owned())
            .or_default()
            .insert(format!("{}.{}/{label}", VEHICLES.0, VEHICLES.1), *id);
    }
    o
}

fn order(addons: &[&str]) -> Vec<String> {
    addons.iter().map(|a| (*a).to_owned()).collect()
}

fn players_store(name: &str) -> (PathBuf, Built) {
    let data = data_dir(name);
    let first = build(
        VEHICLES,
        10,
        &[("a", &["a1", "a2"]), ("b", &["b1"])],
        Store::default(),
    );
    first.store.save(&data).unwrap();
    (data, first)
}
