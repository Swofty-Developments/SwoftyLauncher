use std::collections::BTreeSet;

use swofty_formats::hash::fnv1a64;

use crate::ops::field_names::{ANY_SHAPE, FieldNames, row_shape};
use crate::ops::game_db::Db;

#[derive(Default)]
pub(crate) struct Walk {
    pub(crate) keys: BTreeSet<String>,
    pub(crate) uncatalogued: BTreeSet<String>,
}

pub(crate) fn walk(db: &Db, names: &dyn FieldNames, path: &str, rel: &str, out: &mut Walk) {
    let shape = row_shape(path);
    let entries = match names.row_keys(&shape) {
        Some(e) => e,
        None => {
            out.uncatalogued.insert(shape);
            match names.row_keys(ANY_SHAPE) {
                Some(e) => e,
                None => return,
            }
        }
    };
    for entry in entries {
        let Some(sub) = entry.strip_prefix('+') else {
            if db.get(fnv1a64(format!("{path}.{entry}"))).is_some() {
                out.keys.insert(format!("{rel}{entry}"));
            }
            continue;
        };
        let (head, word) = match sub.rsplit_once('.') {
            Some((h, w)) => (format!("{h}."), w),
            None => (String::new(), sub),
        };
        let container = format!("{path}.{}", head.trim_end_matches('.'));
        let container = container.trim_end_matches('.');
        let count_key = format!("{rel}{head}_{word}_instancecount_");
        let rows = match db.instance_count(container, word) {
            Some(n) => {
                out.keys.insert(count_key);
                n
            }
            None => (0..4096)
                .take_while(|i| {
                    db.get(fnv1a64(format!("{container}.{word}_{i}._metadata_")))
                        .is_some()
                })
                .count(),
        };
        for i in 0..rows {
            walk(
                db,
                names,
                &format!("{container}.{word}_{i}"),
                &format!("{rel}{head}{word}_{i}."),
                out,
            );
        }
    }
}
