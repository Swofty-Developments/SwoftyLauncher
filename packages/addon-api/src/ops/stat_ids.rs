use swofty_formats::hash::fnv1a64;

use crate::ops::apply::add_row::add_row_name;
use crate::ops::db::{DbOp, DbOps};
use crate::ops::field_names::FieldNames;
use crate::ops::game_db::Db;

/// An `add-row` that set a `stat_id` a row added earlier holds, or one above the table's
/// [`stat_id_ceiling`], and the id it got instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedStatId {
    pub row: String,
    /// The row's id key: `stat_id`, or `statId` in `game_setup.scenarios`.
    pub field: &'static str,
    pub label: Option<String>,
    pub old: i64,
    pub new: i64,
    /// The row added earlier that holds `old`; `None` when `old` is above the ceiling or kept for `record`.
    pub holder: Option<String>,
    pub holder_label: Option<String>,
    /// The record of the core's registry `old` is kept for in `ids.json`.
    pub record: Option<String>,
}

/// The highest `stat_id` the game reads for a row of a table it keeps the id of in a byte, and
/// the lowest id a fresh row past it may take (`None`: above every stock row's id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatIdCeiling {
    pub max: i64,
    pub from: Option<i64>,
}

/// The record that lists the virtual entrant lists (comma-separated labels) for the build's settle step.
pub const VIRTUAL_LISTS: &str = "swofty.virtual_entrant_lists";

pub(crate) const VIRTUAL_LISTS_TAG: u8 = 4;

pub(crate) fn virtual_lists(db: &Db) -> Vec<String> {
    db.text(fnv1a64(VIRTUAL_LISTS))
        .map(|t| {
            t.split(',')
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

const STAT_ID_CEILINGS: &[(&str, &str, StatIdCeiling)] = &[
    // Vehicle and team ids are 14 bits in the game's packed keys (0x3fff = none), like driver ids; the core's
    // registry pools end at the same 16382 (VEHICLE_STAT_MAX / TEAM_STAT_MAX in apply/content/lower.hpp).
    (
        "entrants.vehicles",
        "vehicle",
        StatIdCeiling {
            max: 16382,
            from: None,
        },
    ),
    (
        "entrants.teams",
        "team",
        StatIdCeiling {
            max: 16382,
            from: None,
        },
    ),
    // VehicleIdentity +0xcd8 and VehiclePackageRow +0x0 are u8, 0xff = none.
    (
        "entrants.vehicles.vehicle_packages",
        "vehicle_package",
        StatIdCeiling { max: 254, from: None },
    ),
    // Driver ids are u16 in memory and 14 bits in saves and net (0x3fff = none); 254 is default_driver, and UDP 2024/2025 read a low byte of 255 as a network player.
    (
        "entrants.drivers",
        "driver",
        StatIdCeiling {
            max: 16382,
            from: Some(208),
        },
    ),
    // The core's entrant list (content.package) id pool ends at 255.
    (
        "entrants.entrant_lists",
        "entrant_list",
        StatIdCeiling { max: 255, from: None },
    ),
    // VehicleIdentity +0xcd9 is u8; the R&D tree's row flag 0xff means a team row.
    (
        "entrants.engines",
        "Engine",
        StatIdCeiling { max: 254, from: None },
    ),
    // TeamRecord +0x581 team_boss is u8, 0xff = none.
    (
        "entrants.characters",
        "character",
        StatIdCeiling { max: 254, from: None },
    ),
    // TeamRecord +0x560 animation_set is u8, 0xff = none.
    (
        "entrants.animation.driver_animation_sets",
        "driver_animation_set",
        StatIdCeiling { max: 254, from: None },
    ),
    // TeamRecord +0x580 nationality is u8.
    (
        "entrants.nationality",
        "nationality",
        StatIdCeiling { max: 255, from: None },
    ),
    // The core's scenario id pool ends at 255.
    (
        "game_setup.scenarios",
        "scenario",
        StatIdCeiling { max: 254, from: None },
    ),
    // The mask parser 0x142a0ddb0 sets bit `stat_id` of a u32 for each mode a row lists and throws
    // 'invalid bitset position' at boot above 31.
    (
        "game_setup.game_setup_modes",
        "game_setup_mode",
        StatIdCeiling { max: 31, from: None },
    ),
    // The same parser's season mask, by `statId`.
    (
        "game_setup.seasons",
        "season",
        StatIdCeiling { max: 31, from: None },
    ),
    // The Time Trial leaderboard descriptor id is read as a byte (list +0xe0) and goes into the board name LBV<id><track><D|W>.
    (
        "leaderboards",
        "leaderboard_descriptor",
        StatIdCeiling { max: 255, from: None },
    ),
    // Each game setup mode names its entrant setup mode; kept to the mode mask's range with it.
    (
        "entrants.entrant_setup_modes",
        "entrant_setup_mode",
        StatIdCeiling { max: 31, from: None },
    ),
];

/// The key that holds the id of `table`'s rows named `row`: `statId` for scenarios and seasons, else `stat_id`.
pub fn stat_id_field(table: &str, row: &str) -> &'static str {
    match (table, row) {
        ("game_setup.scenarios", "scenario") | ("game_setup.seasons", "season") => "statId",
        _ => "stat_id",
    }
}

/// The [`StatIdCeiling`] of `table`'s rows named `row`, for a table the game keeps the id of in a byte.
pub fn stat_id_ceiling(table: &str, row: &str) -> Option<StatIdCeiling> {
    STAT_ID_CEILINGS
        .iter()
        .find(|(t, r, _)| *t == table && *r == row)
        .map(|(_, _, c)| *c)
}

/// Why `table`'s ceiling is where it is, for tables whose limit an addon author can't work around by
/// picking another id: appended to the "no free stat_id" error so the message says what to change.
pub fn stat_id_ceiling_reason(table: &str, row: &str) -> Option<&'static str> {
    match (table, row) {
        ("game_setup.game_setup_modes", "game_setup_mode")
        | ("entrants.entrant_setup_modes", "entrant_setup_mode") => Some(concat!(
            "the game keeps setup modes in a 32-bit mask (bit = stat_id) and stops at boot with ",
            "'invalid bitset position' above 31, so no install holds more than 32; share a mode ",
            "instead of adding one per box or season (list-append the entrant list to swofty_f1 ",
            "or to one mode the addons share)"
        )),
        ("game_setup.seasons", "season") => Some(concat!(
            "the game keeps seasons in a 32-bit mask (bit = statId) and stops at boot with ",
            "'invalid bitset position' above 31; reuse a stock season row instead of adding one"
        )),
        _ => None,
    }
}

/// Whether a fresh row of `table`'s rows named `row` may not take `id`: driver ids 254 and 255, and any with a low byte of 0xff.
pub fn stat_id_unusable(table: &str, row: &str, id: i64) -> bool {
    table == "entrants.drivers" && row == "driver" && (id == 254 || id & 0xff == 0xff)
}

/// The ids `add-row` ops in `ops` set on their rows, as `("<table>.<row>", id)`.
pub fn explicit_stat_ids(ops: &DbOps) -> impl Iterator<Item = (String, i64)> + '_ {
    ops.ops.iter().filter_map(|op| match op {
        DbOp::AddRow { table, from, set, .. } => {
            let row = add_row_name(table, from)?;
            let id = set.get(stat_id_field(table, row))?.trim().parse().ok()?;
            Some((format!("{table}.{row}"), id))
        }
        _ => None,
    })
}

// The lowest id from the ceiling's `from`, or above every stock row's id, up to its `max` that no row holds.
pub(crate) fn free_stat_id(
    db: &Db,
    names: &dyn FieldNames,
    table: &str,
    row: &str,
    rows: usize,
    ceiling: StatIdCeiling,
    skip: &dyn Fn(i64) -> bool,
) -> Option<i64> {
    let skip = |id: i64| skip(id) || stat_id_unusable(table, row, id);
    let mut ids = db.stat_ids(names, table, row, rows);
    let from = ceiling.from.unwrap_or(ids.stock + 1);
    // Every id below the lowest free one is held or unreadable, so the search starts there.
    let low = ids.lowest_free(from);
    (low..=ceiling.max).find(|id| !ids.used.contains(id) && !skip(*id))
}
