use std::collections::HashSet;

use super::*;
use crate::ops::*;

// The ids fresh rows took before 1.7.1, when each add-row read every row's id again: the next above
// the table's highest, or for a table with a ceiling the lowest free one from its `from` (or above
// the stock rows) that the game reads. None once nothing is free.
fn ids_before_the_cache(stock: &[i64], path: &str, row: &str, adds: usize) -> Vec<Option<i64>> {
    let ceiling = stat_id_ceiling(path, row);
    let stock_max = stock.iter().copied().max().unwrap_or(-1);
    let mut all = stock.to_vec();
    (0..adds)
        .map(|_| {
            let mut top = all.iter().copied().max().map_or(0, |m| m + 1);
            while stat_id_unusable(path, row, top) {
                top += 1;
            }
            let id = match ceiling {
                Some(c) if top > c.max || c.from.is_some() => {
                    let used: HashSet<i64> = all.iter().copied().collect();
                    (c.from.unwrap_or(stock_max + 1)..=c.max)
                        .find(|id| !used.contains(id) && !stat_id_unusable(path, row, *id))
                }
                _ => Some(top),
            };
            all.extend(id);
            id
        })
        .collect()
}

fn added_ids(stock: &[i64], path: &str, row: &str, adds: usize) -> Vec<Option<i64>> {
    let mut db = table(path, row, stock);
    let names: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let labels: Vec<String> = (0..adds).map(|i| format!("added_{i}")).collect();
    let rows: Vec<(&str, Option<i64>)> = labels.iter().map(|l| (l.as_str(), None)).collect();
    let (_, failed) = apply_db_ops_lenient(
        &mut db,
        &add_rows(path, row, &rows),
        &names,
        &mut DbReport::default(),
    );
    let mut got: Vec<Option<i64>> = ids(&db, path, row)[stock.len()..]
        .iter()
        .copied()
        .map(Some)
        .collect();
    got.extend(failed.iter().map(|_| None));
    got
}

#[test]
fn fresh_driver_ids_match_the_search_before_the_cache() {
    let stock = [1, 7, 30, 207, 254];
    assert_eq!(
        added_ids(&stock, "entrants.drivers", "driver", 400),
        ids_before_the_cache(&stock, "entrants.drivers", "driver", 400)
    );
}

#[test]
fn fresh_driver_ids_up_to_the_14_bit_ceiling_match_the_search_before_the_cache() {
    let mut stock: Vec<i64> = (208..=16100)
        .filter(|id| !stat_id_unusable("entrants.drivers", "driver", *id) && id % 97 != 0)
        .collect();
    stock.push(254);
    let before = ids_before_the_cache(&stock, "entrants.drivers", "driver", 450);
    assert_eq!(before.iter().flatten().max(), Some(&16382));
    assert!(before.last().unwrap().is_none());
    assert_eq!(added_ids(&stock, "entrants.drivers", "driver", 450), before);
}

#[test]
fn fresh_package_ids_past_the_byte_ceiling_match_the_search_before_the_cache() {
    let stock = [2, 5, 58];
    let before = ids_before_the_cache(&stock, PACKAGES, "vehicle_package", 300);
    assert!(before.last().unwrap().is_none());
    assert_eq!(added_ids(&stock, PACKAGES, "vehicle_package", 300), before);
}

#[test]
fn an_id_set_between_add_rows_frees_the_one_it_replaced() {
    let mut db = table("entrants.drivers", "driver", &[1, 7]);
    let names: HashSet<u64> = db.records.iter().map(|r| r.key).collect();
    let ops = self::ops(
        r#"{"schema_version":1,"fields":{"entrants.drivers.driver":["_metadata_","stat_id"]},"ops":[
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"a"}},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"b"}},
            {"op":"set","select":"entrants.drivers.driver_2","field":"stat_id","value":"300"},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"c"}},
            {"op":"add-row","table":"entrants.drivers","from":"entrants.drivers.driver_0","set":{"_metadata_":"d"}}]}"#,
    );
    apply_db_ops(&mut db, &ops, &names).unwrap();
    assert_eq!(ids(&db, "entrants.drivers", "driver"), [1, 7, 300, 209, 208, 210]);
}
