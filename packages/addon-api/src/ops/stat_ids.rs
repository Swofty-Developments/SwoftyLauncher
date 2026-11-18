use swofty_formats::hash::fnv1a64;

use crate::ops::apply::add_row::add_row_name;
use crate::ops::db_ops::{DbOp, DbOps};
use crate::ops::field_names::FieldNames;
use crate::ops::game_db::Db;

/// An `add-row` that set a `stat_id` a row added earlier holds, or one above the table's
/// [`stat_id_ceiling`], and the id it got instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedStatId {
    pub row: String,
    /// The row's id key: `stat_id`, or `statId` in `game_setup.scenarios` and unlock lists.
    pub field: &'static str,
    pub label: Option<String>,
    pub old: i64,
    pub new: i64,
    /// The row added earlier that holds `old` (in an unlock list, also a stock row); `None` when
    /// `old` is above the ceiling or kept for `record`.
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
    // The F1 Life scene holds two car customisation slots (+0xd00, +0xe20) and writes car group `stat_id`
    // into slot `stat_id` with no bounds check (0x14252ccd0 on build 26566308).
    (
        "customisation.customisation_cars",
        "customisation_car_group",
        StatIdCeiling { max: 1, from: None },
    ),
];

/// The key that holds the id of `table`'s rows named `row`: `statId` for scenarios and seasons, else `stat_id`.
pub fn stat_id_field(table: &str, row: &str) -> &'static str {
    match (table, row) {
        ("game_setup.scenarios", "scenario") | ("game_setup.seasons", "season") => "statId",
        _ if unlock_list(table).is_some() => "statId",
        _ => "stat_id",
    }
}

const UNLOCK_LISTS: &str = "unlockable_content.unlockable_items.unlockable_item_list_";

/// The name the row ids of an unlock list go under: one id space for every folder and kind of the
/// list, kept as `<list>.unlockable_item`, as in
/// `unlockable_content.unlockable_items.unlockable_item_list_2.unlockable_item`.
pub const UNLOCK_ROW: &str = "unlockable_item";

/// The highest `statId` the launcher gives an unlock item, the top of the core's livery unlock pool.
pub const UNLOCK_STAT_MAX: i64 = 65535;

/// Where a fresh unlock `statId` starts when the list's stock rows stay below it. Game updates add their
/// unlock rows right above the highest stock id (`toro_rosso_barcelona_26` took 5280 in Steam build
/// 26566308), so an id just above it is the next update's.
pub const UNLOCK_FRESH_FROM: i64 = 10000;

/// The kinds of rows an unlock list's folders hold, each a table `<folder>.<kind>` with a `statId`.
pub const UNLOCK_KINDS: &[&str] = &[
    "consumable_xp_booster",
    "currency_reward",
    "unlockable_badge_bg",
    "unlockable_badge_emblem",
    "unlockable_casual_footwear",
    "unlockable_casual_headwear",
    "unlockable_decal_group",
    "unlockable_decal_set",
    "unlockable_driver",
    "unlockable_driver_number",
    "unlockable_eyewear",
    "unlockable_gloves",
    "unlockable_head",
    "unlockable_helmet",
    "unlockable_item_pack",
    "unlockable_livery",
    "unlockable_personal_sticker",
    "unlockable_podium_emote",
    "unlockable_pose",
    "unlockable_se_livery",
    "unlockable_suit",
    "unlockable_team",
    "unlockable_tops",
    "unlockable_trousers",
    "unlockable_vehicle",
    "unlockable_victory_vo",
];

/// The unlock list `unlockable_content.unlockable_items.unlockable_item_list_<n>` whose folder
/// `table` (`….unlockable_item_list_<n>.folder_<m>`) is.
pub fn unlock_list(table: &str) -> Option<&str> {
    let rest = table.strip_prefix(UNLOCK_LISTS)?;
    let (n, folder) = rest.split_once('.')?;
    let m = folder.strip_prefix("folder_")?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(n) && digits(m)).then(|| &table[..UNLOCK_LISTS.len() + n.len()])
}

/// Whether `table` (`<table>.<row>`, as [`RowIds`](crate::ops::RowIds) keys name it) is an unlock list's
/// id space, `<list>.`[`UNLOCK_ROW`].
pub fn is_unlock_ids(table: &str) -> bool {
    table
        .strip_prefix(UNLOCK_LISTS)
        .and_then(|r| r.strip_suffix(UNLOCK_ROW))
        .and_then(|n| n.strip_suffix('.'))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
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
        ("customisation.customisation_cars", "customisation_car_group") => Some(concat!(
            "the F1 Life scene behind the main menu has two car slots and indexes them by the car ",
            "group's stat_id, so a third car group crashes the game at the main menu; leave the car ",
            "groups alone or edit one of the two"
        )),
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

/// The ids `add-row` ops in `ops` set on their rows, as `("<table>.<row>", id)`; a row of an unlock
/// list as `("<list>.unlockable_item", id)`, since the list's folders and kinds share one id space.
pub fn explicit_stat_ids(ops: &DbOps) -> impl Iterator<Item = (String, i64)> + '_ {
    ops.ops.iter().filter_map(|op| match op {
        DbOp::AddRow { table, from, set, .. } => {
            let row = add_row_name(table, from)?;
            let id = set.get(stat_id_field(table, row))?.trim().parse().ok()?;
            Some(match unlock_list(table) {
                Some(list) => (format!("{list}.{UNLOCK_ROW}"), id),
                None => (format!("{table}.{row}"), id),
            })
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
