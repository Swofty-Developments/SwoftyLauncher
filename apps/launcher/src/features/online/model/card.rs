//! What the Multiplayer page's Time Trial card previews: the season it lists and how the player's best stands on
//! each track.

use super::overview::SeasonCard;
use super::trail::RecentSeason;

/// How many tracks of a season the Multiplayer page's Time Trial card lists.
pub const CARD_TRACKS: usize = 4;

/// How the player's best lap on a track stands against its record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnGap {
    NoLap,
    Record,
    Behind(u64),
}

/// The player's best `mine` against the track's `record`, both in milliseconds.
pub fn own_gap(mine: Option<u64>, record: Option<u64>) -> OwnGap {
    match (mine, record) {
        (None, _) => OwnGap::NoLap,
        (Some(mine), Some(record)) if mine > record => OwnGap::Behind(mine - record),
        (Some(_), _) => OwnGap::Record,
    }
}

/// The season the Time Trial card lists: the season viewed last, else the first.
pub fn preview_season<'a>(seasons: &'a [SeasonCard], recents: &[RecentSeason]) -> Option<&'a SeasonCard> {
    recents
        .iter()
        .find_map(|r| seasons.iter().find(|s| s.id == r.id))
        .or_else(|| seasons.first())
}
