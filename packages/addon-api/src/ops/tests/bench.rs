use std::collections::HashSet;
use std::time::Instant;

use super::*;
use crate::ops::{DbOps, DbReport, apply_db_ops_lenient};

/// Times one op file applied to a database: SWOFTY_BENCH_DB (a game database), SWOFTY_BENCH_OPS (its op
/// file), and writes the database it makes to SWOFTY_BENCH_OUT when set.
#[test]
#[ignore]
fn bench_an_op_file_on_a_database() {
    let (Some(db), Some(ops)) = (
        std::env::var_os("SWOFTY_BENCH_DB"),
        std::env::var_os("SWOFTY_BENCH_OPS"),
    ) else {
        return;
    };
    let bytes = std::fs::read(db).unwrap();
    let ops: DbOps = serde_json::from_slice(&std::fs::read(ops).unwrap()).unwrap();
    let applied = |scan: bool| {
        crate::ops::game_db::SCAN_SELECTORS.with(|s| s.set(scan));
        let mut gvdb = Gvdb::parse(&bytes).unwrap();
        let stock: HashSet<u64> = gvdb.records.iter().map(|r| r.key).collect();
        let mut report = DbReport::default();
        let at = Instant::now();
        let (replaced, failed) = apply_db_ops_lenient(&mut gvdb, &ops, &stock, &mut report);
        let took = at.elapsed();
        crate::ops::game_db::SCAN_SELECTORS.with(|s| s.set(false));
        eprintln!(
            "[bench] {} {} ops in {:.3} s: {} replaced, {} failed",
            if scan { "scan" } else { "kept" },
            ops.ops.len(),
            took.as_secs_f64(),
            replaced.len(),
            failed.len()
        );
        let failed: Vec<String> = failed.iter().map(|(i, e)| format!("{i}: {e}")).collect();
        (gvdb.to_bytes(), replaced, failed, report.warnings, report.lines)
    };
    let kept = applied(false);
    let scanned = applied(true);
    assert!(kept == scanned, "the kept values made another database");
    eprintln!("[bench] same database, {} bytes", kept.0.len());
    if let Some(out) = std::env::var_os("SWOFTY_BENCH_OUT") {
        std::fs::write(out, &kept.0).unwrap();
    }
}
