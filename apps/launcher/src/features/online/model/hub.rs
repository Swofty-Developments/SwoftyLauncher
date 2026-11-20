//! What the Online page previews of Time Trial: the season it lists and the track it has open in place.

use std::collections::HashMap;

use super::category::Row;
use super::overview::{BoardKey, SeasonCard};
use super::trail::RecentSeason;

/// How many of a board's laps a track open in place lists.
pub const PEEK_ROWS: usize = 10;

/// How Time Trial looks: as in 1.10 for a player without access to the rooms, and linked with the rooms for one
/// with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrialLook {
    Classic,
    Linked,
}

impl TrialLook {
    pub fn of(access: bool) -> Self {
        match access {
            true => Self::Linked,
            false => Self::Classic,
        }
    }

    /// The page's root lists every season, each opening its own page, rather than one season's tracks under its
    /// chips.
    pub fn lists_seasons(self) -> bool {
        self == Self::Classic
    }

    /// A track's row opens its board in place under it, rather than on the board's own page.
    pub fn opens_in_place(self) -> bool {
        self == Self::Linked
    }

    /// The trail shows at the page's root too, with Time Trial as its first crumb.
    pub fn trail_at_root(self) -> bool {
        self == Self::Classic
    }
}

/// The track the Online page has open in place, one at a time, and the board read for each track opened, kept
/// until the boards reload so a track opened again shows at once.
#[derive(Debug)]
pub struct Peek<T> {
    open: Option<BoardKey>,
    pages: HashMap<BoardKey, T>,
}

impl<T> Default for Peek<T> {
    fn default() -> Self {
        Self {
            open: None,
            pages: HashMap::new(),
        }
    }
}

impl<T> Peek<T> {
    /// Opens `key`, closing the track open before, or closes `key` when it is the open one. True when the track
    /// is now open with no board read for it yet.
    pub fn toggle(&mut self, key: BoardKey) -> bool {
        if self.open.as_ref() == Some(&key) {
            self.open = None;
            return false;
        }
        let unread = !self.pages.contains_key(&key);
        self.open = Some(key);
        unread
    }

    pub fn is_open(&self, key: &BoardKey) -> bool {
        self.open.as_ref() == Some(key)
    }

    pub fn page(&self, key: &BoardKey) -> Option<&T> {
        self.pages.get(key)
    }

    pub fn put(&mut self, key: BoardKey, page: T) {
        self.pages.insert(key, page);
    }

    /// Forgets every board read, as the boards reload. The open track stays open; it is returned to read again.
    pub fn reset(&mut self) -> Option<BoardKey> {
        self.pages.clear();
        self.open.clone()
    }
}

/// This player's lap on a board: the one the server marks as theirs, else the row of the driver `me` among the
/// rows read. The boards server marks none today, so the driver is what finds it.
pub fn own_row<'a>(rows: &'a [Row], marked: Option<&'a Row>, me: Option<&str>) -> Option<&'a Row> {
    marked.or_else(|| me.and_then(|id| rows.iter().find(|r| r.driver.id == id)))
}

/// The own lap a full board pins above its rows: the own lap when none of the rows read is it.
pub fn pinned_row<'a>(rows: &[Row], own: Option<&'a Row>) -> Option<&'a Row> {
    own.filter(|m| rows.iter().all(|r| r.lap_id != m.lap_id))
}

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

/// How many tracks of a season the Multiplayer page's Time Trial card lists.
pub const CARD_TRACKS: usize = 4;

/// The board a debug start opens in place: `value` names a track of `season`, with `/weather` after it when the
/// board is not the dry one.
pub fn start_track(value: &str, season: &str) -> Option<BoardKey> {
    let (track, weather) = value.trim().split_once('/').unwrap_or((value.trim(), "dry"));
    (!track.is_empty() && !weather.is_empty()).then(|| BoardKey {
        content: season.to_owned(),
        track: track.to_owned(),
        weather: weather.to_owned(),
    })
}

/// The season the Online page lists: the one picked there, else the season viewed last, else the first.
pub fn hub_season<'a>(
    seasons: &'a [SeasonCard],
    picked: Option<&str>,
    recents: &[RecentSeason],
) -> Option<&'a SeasonCard> {
    let find = |id: &str| seasons.iter().find(|s| s.id == id);
    picked
        .and_then(find)
        .or_else(|| recents.iter().find_map(|r| find(&r.id)))
        .or_else(|| seasons.first())
}
