use std::collections::BTreeMap;

use crate::asset::Op;
pub use crate::asset::{Features, Kind};
use crate::ops::{MovedStatId, stat_id_ceiling, unlock_list};

/// The build warning for `n` removals of stock records that addon `name` asked for.
pub fn stock_removals_line(name: &str, n: usize) -> String {
    format!(
        "{name}: {n} removal(s) of stock database records skipped; the game crashes when a record it reads is missing"
    )
}

/// The build warning for `n` removals of records the tracks add that addon `name` asked for.
pub fn track_removals_line(name: &str, n: usize) -> String {
    format!(
        "{name}: {n} removal(s) of database records the tracks add skipped; the game crashes when a record it reads is missing"
    )
}

pub(super) fn stat_row(row: &str) -> &str {
    row.rsplit_once('_').map_or(row, |(table, _)| table)
}

/// The build warning for a row of `addon` whose `stat_id` a row of `holder` already had, or
/// which was above the table's ceiling when `holder` is `None`.
pub fn moved_stat_id_line(addon: &str, holder: Option<&str>, m: &MovedStatId) -> String {
    let label = |l: &Option<String>, row: &str| l.clone().unwrap_or_else(|| row.to_owned());
    let why = match (holder, &m.holder, &m.record) {
        (Some(holder), Some(row), _) => {
            format!("is taken by {} from {holder}", label(&m.holder_label, row))
        }
        (_, _, Some(record)) => {
            format!("is the id careers hold for {record}")
        }
        _ => {
            let table = stat_row(&m.row);
            let max = table
                .rsplit_once('.')
                .and_then(|(t, r)| stat_id_ceiling(t, r))
                .map_or_else(String::new, |c| format!(" {}", c.max));
            format!("is above{max}, the highest the game reads in {table}")
        }
    };
    let field = m.field;
    let advice = match field {
        "stat_id" => "leave stat_id out of add-row",
        _ if stat_row(&m.row)
            .rsplit_once('.')
            .is_some_and(|(t, _)| unlock_list(t).is_some()) =>
        {
            "leave statId out of add-row"
        }
        _ => "give the row a statId no other addon uses",
    };
    format!(
        "{addon}: {field} {} of {} {why}; the build gave it {field} {}. The addon's author should {advice}",
        m.old,
        label(&m.label, &m.row),
        m.new
    )
}

/// The build warning for the entrant `lists` addon `name` renames or removes.
pub fn kept_lists_line(name: &str, lists: &[String]) -> String {
    let shown: Vec<&str> = lists.iter().take(8).map(String::as_str).collect();
    let more = match lists.len() > shown.len() {
        true => format!(" and {} more", lists.len() - shown.len()),
        false => String::new(),
    };
    format!(
        "{name}: renames or removes the entrant list(s) {}{more}; F2 menus may crash with it",
        shown.join(", ")
    )
}

pub fn waiting_lines(waiting: &[Op], features: &Features) -> Vec<String> {
    let mut counts: BTreeMap<(String, Kind), usize> = BTreeMap::new();
    for op in waiting {
        if let Some(kind) = op.op.kind() {
            *counts.entry((op.origin.to_string(), kind)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|((origin, kind), n)| match features.experimental.contains(&kind) {
            true => format!(
                "{origin}: {n} {kind} op(s) wait; the runtime {kind} feature is experimental and \"runtime_assets\" in config.json does not turn it on"
            ),
            false => format!("{origin}: {n} {kind} op(s) wait for {kind} support in swofty_core"),
        })
        .collect()
}
