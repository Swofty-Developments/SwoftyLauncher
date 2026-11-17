//! The landing page: drivers, seasons, tracks and what the overview lists.

use std::fmt;
use swofty_core::online::timetrial::boards::track_addon;

use super::category::Category;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineError {
    pub status: u16,
    pub message: String,
}

impl OnlineError {
    pub fn network(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            message: message.into(),
        }
    }
}

impl fmt::Display for OnlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankEntry {
    pub rank: u32,
    pub driver: Driver,
    pub points: f32,
    pub wins: u32,
    pub podiums: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonCard {
    pub id: String,
    pub name: String,
    pub cover: Option<String>,
    pub tracks: u32,
    pub drivers: u32,
    pub top: Vec<Driver>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodiumEntry {
    pub driver: Driver,
    pub time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackCard {
    pub key: BoardKey,
    pub name: String,
    pub flag: Option<String>,
    pub layout: Option<String>,
    pub record: Option<PodiumEntry>,
    pub podium: Vec<PodiumEntry>,
    pub drivers: u32,
    pub laps: u32,
    pub custom: bool,
    /// The addon the track comes from; none for a stock track, whatever box the board is in.
    pub addon: Option<String>,
}

impl TrackCard {
    /// A board's track known by its key and name alone, as a link to its board.
    pub fn bare(key: BoardKey, name: String) -> Self {
        Self {
            addon: track_addon(&key.track, None).map(str::to_owned),
            key,
            name,
            flag: None,
            layout: None,
            record: None,
            podium: Vec::new(),
            drivers: 0,
            laps: 0,
            custom: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoardKey {
    pub content: String,
    pub track: String,
    pub weather: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    pub seasons: Vec<SeasonCard>,
    pub global: Vec<RankEntry>,
    pub custom: Vec<TrackCard>,
    pub me: Option<RankEntry>,
}

impl Overview {
    pub fn is_empty(&self) -> bool {
        self.seasons.is_empty() && self.global.is_empty() && self.custom.is_empty()
    }
}

/// Why the landing page has nothing to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingEmpty {
    /// Nobody has uploaded a lap at all.
    NothingYet,
    /// Laps exist, but none match the chosen assists or input; the filter bar stays so it can be changed.
    Filtered,
}

pub fn landing_empty(overview: &Overview, cat: &Category) -> Option<LandingEmpty> {
    if !overview.is_empty() {
        return None;
    }
    Some(if cat.is_default() {
        LandingEmpty::NothingYet
    } else {
        LandingEmpty::Filtered
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonPage {
    pub season: SeasonCard,
    pub ranking: Vec<RankEntry>,
    pub tracks: Vec<TrackCard>,
}
