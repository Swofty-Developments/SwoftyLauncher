//! Game database edits the core applies at runtime, and the room the game's database has for them.

use std::collections::HashMap;

use swofty_formats::gvdb::Gvdb;

use super::assets::DbEdit;

pub(super) const NO_VALUE_TAGS: [u8; 3] = [10, 15, 16];

/// Why `swofty_core.dll` ignores an edit of `value` as a record of `tag`, by the
/// rule of its `check_text`; `Ok` when it applies it. Empty text is the game's 0,
/// 0.0 or false for the scalar tags, as in the 7,007 stock colours that hold it.
pub fn runtime_text(tag: u8, value: &str) -> std::result::Result<(), String> {
    if value.is_empty() && matches!(tag, 0 | 1 | 5 | 11 | 12) {
        return Ok(());
    }
    let integer = || {
        let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    };
    match tag {
        0 | 11 if !integer() => Err(format!(
            "an int needs integer text (the game parses it with atoi), got {value:?}"
        )),
        0 | 11 if value.parse::<i32>().is_err() => Err(format!(
            "{value:?} is outside the 32-bit int range the game stores"
        )),
        12 if !integer() => Err(format!(
            "a t12 value needs integer text (the game parses it with strtoull), got {value:?}"
        )),
        12 if value.trim_start_matches(['-', '+']).parse::<u64>().is_err() => {
            Err(format!("{value:?} is outside the unsigned 64-bit range"))
        }
        1 if !value
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v.abs() <= f32::MAX as f64) =>
        {
            Err(format!("{value:?} is not a finite 32-bit float"))
        }
        5 if !value.eq_ignore_ascii_case("true") && !value.eq_ignore_ascii_case("false") => {
            Err(format!("a bool needs true or false, got {value:?}"))
        }
        10 | 15 | 16 => Err(format!("tag {tag} stores nothing")),
        19.. => Err(format!("tag {tag} is not a type the core knows")),
        _ => Ok(()),
    }
}

/// Records the core may add at runtime on top of a game database file of `loaded` records. F1 25 sizes the
/// database's robin_hood map while it loads the file, to the smallest power-of-two slot count whose 80% holds
/// the records, and grows it when an insert finds it 80% full. Growing it after the load leaves the game's
/// pointers into the old slots dangling, and the game crashes at boot (Gdb_Init_TableCursor under locale
/// init), so the core's inserts have to stay under that 80%. The margin covers the core's class probe
/// records and inserts the game makes later.
pub fn runtime_record_room(loaded: usize) -> usize {
    let max_load = |slots: usize| slots / 100 * 80 + slots % 100 * 80 / 100;
    let mut slots = 8usize;
    while max_load(slots) < loaded {
        slots *= 2;
    }
    max_load(slots).saturating_sub(loaded + RUNTIME_RECORD_MARGIN)
}

pub const RUNTIME_RECORD_MARGIN: usize = 4096;

pub const STOCK_LOADED_RECORDS: usize = 742_857;

pub fn runtime_loaded_estimate(staged: usize) -> usize {
    staged.max(STOCK_LOADED_RECORDS)
}

pub fn db_edits(base: &Gvdb, built: &Gvdb) -> std::result::Result<Vec<DbEdit>, String> {
    let stock: HashMap<u64, _> = base.records.iter().map(|r| (r.key, r)).collect();
    let kept: std::collections::HashSet<u64> = built.records.iter().map(|r| r.key).collect();
    let removed = stock.keys().filter(|k| !kept.contains(k)).count();
    if removed > 0 {
        return Err(format!(
            "{removed} record(s) are removed, which runtime database edits cannot do"
        ));
    }
    let mut edits = Vec::new();
    for r in &built.records {
        let before = stock.get(&r.key);
        if before.is_some_and(|b| *b == r) {
            continue;
        }
        if NO_VALUE_TAGS.contains(&r.tag) {
            return Err(format!(
                "record {:#018x} has tag {}, which stores nothing",
                r.key, r.tag
            ));
        }
        let value = String::from_utf8(r.value.clone())
            .map_err(|_| format!("record {:#018x} does not hold text", r.key))?;
        runtime_text(r.tag, &value).map_err(|why| {
            format!(
                "swofty_core.dll would ignore record {:#018x} (tag {}): {why}",
                r.key, r.tag
            )
        })?;
        edits.push(DbEdit {
            hash: format!("{:016x}", r.key),
            tag: r.tag,
            value,
            retype: before.is_some_and(|b| b.tag != r.tag),
        });
    }
    Ok(edits)
}
