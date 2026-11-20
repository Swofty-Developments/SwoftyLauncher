use std::collections::HashSet;

use super::*;
use crate::ops::game_db::{Db, SCAN_SELECTORS};
use crate::ops::resolve::{resolve_key, resolve_path};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const ROWS: usize = 12;
const VALUES: [&str; 4] = ["a", "b", "c", ""];
const FIELDS: [&str; 3] = ["_metadata_", "speed", "kind"];

fn world() -> u64 {
    fnv1a64("hidden.world_table_")
}

/// Every record a write in the test may touch, by the key the database files it under.
fn keys() -> Vec<(u64, String)> {
    let mut out = vec![];
    for table in ["t.counted", "t.uncounted"] {
        out.push((fnv1a64(instance_count_path(table, "row")), "count".to_owned()));
        for idx in 0..ROWS {
            for field in FIELDS {
                out.push((fnv1a64(format!("{table}.row_{idx}.{field}")), field.to_owned()));
            }
        }
    }
    for idx in 0..ROWS {
        for field in FIELDS {
            out.push((fnv1a64_from(world(), format!("{idx}.{field}")), field.to_owned()));
        }
    }
    out
}

fn value(rng: &mut Rng, what: &str) -> String {
    match what {
        "count" if rng.below(8) == 0 => "x".to_owned(),
        "count" => rng.below(ROWS + 1).to_string(),
        _ => VALUES[rng.below(VALUES.len())].to_owned(),
    }
}

fn seeded(rng: &mut Rng) -> Gvdb {
    let mut records = vec![Record::new(&instance_count_path("t.counted", "row"), 0, "8")];
    for (key, what) in keys() {
        if what != "count" && rng.below(4) != 0 {
            records.push(Record {
                key,
                tag: 4,
                value: value(rng, &what).into_bytes(),
            });
        }
    }
    Gvdb::new(records)
}

fn write(db: &mut Db, rng: &mut Rng, keys: &[(u64, String)]) {
    let (key, what) = &keys[rng.below(keys.len())];
    let now = value(rng, what).into_bytes();
    match rng.below(3) {
        0 => {
            db.remove(*key);
        }
        _ => match db.get_mut(*key) {
            Some(mut r) => r.set_value(now),
            None => db.push(Record {
                key: *key,
                tag: 4,
                value: now,
            }),
        },
    }
}

fn answer(db: &mut Db, scan: bool, path: &str, first_wins: bool) -> (String, Vec<String>) {
    db.scan_selectors = scan;
    let got = match first_wins {
        true => resolve_path(db, path).map(|b| b.key("")),
        false => resolve_key(db, path),
    };
    let got = match got {
        Ok(key) => format!("{key:016x}"),
        Err(e) => e.to_string(),
    };
    (got, db.ties.take())
}

#[test]
fn every_selector_the_kept_values_answer_is_the_one_the_scan_gives() {
    let keys = keys();
    let mut kept = 0;
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut gvdb = seeded(&mut rng);
        let mut db = Db::new(&mut gvdb);
        for _ in 0..400 {
            if rng.below(3) == 0 {
                write(&mut db, &mut rng, &keys);
            }
            let field = FIELDS[rng.below(FIELDS.len())];
            let want = VALUES[rng.below(VALUES.len())];
            let path = match rng.below(3) {
                0 => format!("t.counted.row[{field}={want}].speed"),
                1 => format!("t.uncounted.row[{field}={want}]"),
                _ => format!("{{{:016x}}}[{field}={want}]", world()),
            };
            let first_wins = rng.below(2) == 0;
            let indexed = answer(&mut db, false, &path, first_wins);
            let scanned = answer(&mut db, true, &path, first_wins);
            assert_eq!(indexed, scanned, "seed {seed}: {path}");
            kept = kept.max(db.selectors.borrow().kept());
        }
    }
    assert!(kept > 1, "the values were never kept across selectors");
}

#[test]
fn a_write_to_a_row_drops_only_what_it_can_change() {
    let mut gvdb = db();
    let mut db = Db::new(&mut gvdb);
    let team = |db: &Db, field: &str, want: &str| {
        resolve_path(db, &format!("entrants.teams.team[{field}={want}]")).ok()
    };
    assert!(team(&db, "_metadata_", "gm_cadillac_26").is_some());
    assert!(team(&db, "stat_id", "101").is_some());
    assert_eq!(db.selectors.borrow().kept(), 2);
    db.get_mut(fnv1a64("entrants.teams.team_1.stat_id"))
        .unwrap()
        .set_value(b"105".to_vec());
    assert_eq!(
        db.selectors.borrow().kept(),
        1,
        "a write to a row's field drops that field's values only"
    );
    assert!(team(&db, "stat_id", "101").is_none());
    assert!(team(&db, "stat_id", "105").is_some());
    db.get_mut(fnv1a64("entrants.teams.team_0._metadata_"))
        .unwrap()
        .set_value(b"red_bull_26".to_vec());
    assert_eq!(
        db.selectors.borrow().kept(),
        0,
        "a write to a row's _metadata_ drops every field's"
    );
    assert!(team(&db, "_metadata_", "red_bull_26").is_some());
    assert!(team(&db, "stat_id", "100").is_some());
    db.get_mut(fnv1a64(instance_count_path("entrants.teams", "team")))
        .unwrap()
        .set_value(b"1".to_vec());
    assert_eq!(
        db.selectors.borrow().kept(),
        0,
        "a write to the instance count drops every field's"
    );
    assert!(team(&db, "_metadata_", "gm_cadillac_26").is_none());
}

#[test]
fn an_op_file_makes_the_same_database_with_the_kept_values_or_the_scan() {
    let ops = |n: usize| DbOps {
        schema_version: 1,
        fields: BTreeMap::new(),
        strict: false,
        side_data: Default::default(),
        ops: (0..n)
            .map(|i| match i % 3 {
                0 => DbOp::Set {
                    select: "entrants.teams.team[_metadata_=gm_cadillac_26]".into(),
                    field: "display_name_code".into(),
                    value: Some(format!("C{i}")),
                    value_from: None,
                    datasets: Vec::new(),
                    create: false,
                    tag: None,
                },
                1 => DbOp::Set {
                    select: format!("entrants.teams.team[stat_id={}]", 100 + i % 2),
                    field: "stat_id".into(),
                    value: Some((100 + (i + 1) % 2).to_string()),
                    value_from: None,
                    datasets: Vec::new(),
                    create: false,
                    tag: None,
                },
                _ => DbOp::Set {
                    select: "entrants.teams.team[display_name_code=XXX]".into(),
                    field: "_metadata_".into(),
                    value: Some(format!("team_{i}")),
                    value_from: None,
                    datasets: Vec::new(),
                    create: false,
                    tag: None,
                },
            })
            .collect(),
    };
    let built = |scan: bool| {
        SCAN_SELECTORS.with(|s| s.set(scan));
        let mut gvdb = db();
        let mut report = DbReport::default();
        let (replaced, failed) =
            apply_db_ops_lenient(&mut gvdb, &ops(30), &HashSet::<u64>::new(), &mut report);
        SCAN_SELECTORS.with(|s| s.set(false));
        let failed: Vec<String> = failed.iter().map(|(i, e)| format!("{i}: {e}")).collect();
        (gvdb.to_bytes(), replaced, failed, report.warnings, report.lines)
    };
    assert_eq!(built(false), built(true));
}
