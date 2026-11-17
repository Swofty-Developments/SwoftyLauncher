//! Which screen is open and the way back, the track filters, and recent seasons.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use swofty_core::online::timetrial::boards::content_listed;

use super::overview::{BoardKey, TrackCard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Landing,
    Season(String),
    Board(BoardKey),
    Profile(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trail {
    stack: Vec<(Screen, String)>,
}

impl Default for Trail {
    fn default() -> Self {
        Self::new()
    }
}

impl Trail {
    pub fn new() -> Self {
        Self {
            stack: vec![(Screen::Landing, String::new())],
        }
    }

    pub fn push(&mut self, view: Screen, label: impl Into<String>) {
        if self.stack.last().is_some_and(|(v, _)| *v == view) {
            return;
        }
        if let Some(at) = self.stack.iter().position(|(v, _)| *v == view) {
            self.stack.truncate(at + 1);
            return;
        }
        self.stack.push((view, label.into()));
    }

    pub fn current(&self) -> &Screen {
        &self.stack.last().expect("a trail is never empty").0
    }

    pub fn back(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    pub fn jump(&mut self, depth: usize) {
        self.stack.truncate(depth.max(1).min(self.stack.len()));
    }

    pub fn crumbs(&self) -> Vec<(usize, &str)> {
        self.stack
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (_, l))| (i + 1, l.as_str()))
            .collect()
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sort {
    #[default]
    Name,
    Record,
    Drivers,
    Laps,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LapsFilter {
    #[default]
    All,
    Has,
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackFilter {
    pub query: String,
    pub sort: Sort,
    pub laps: LapsFilter,
    pub weather: Option<String>,
    pub mine: bool,
}

impl TrackFilter {
    pub fn apply<'a>(&self, tracks: &'a [TrackCard], mine: &HashSet<BoardKey>) -> Vec<&'a TrackCard> {
        let q = self.query.trim().to_lowercase();
        let mut out: Vec<&TrackCard> = tracks
            .iter()
            .filter(|t| q.is_empty() || t.name.to_lowercase().contains(&q) || t.key.track.contains(&q))
            .filter(|t| match self.laps {
                LapsFilter::All => true,
                LapsFilter::Has => t.laps > 0,
                LapsFilter::None => t.laps == 0,
            })
            .filter(|t| self.weather.as_ref().is_none_or(|w| *w == t.key.weather))
            .filter(|t| !self.mine || mine.contains(&t.key))
            .collect();
        match self.sort {
            Sort::Name => out.sort_by_key(|t| t.name.to_lowercase()),
            Sort::Record => out.sort_by_key(|t| t.record.as_ref().map_or(u64::MAX, |r| r.time_ms)),
            Sort::Drivers => out.sort_by(|a, b| b.drivers.cmp(&a.drivers).then_with(|| a.name.cmp(&b.name))),
            Sort::Laps => out.sort_by(|a, b| b.laps.cmp(&a.laps).then_with(|| a.name.cmp(&b.name))),
        }
        out
    }

    pub fn active(&self) -> bool {
        !self.query.trim().is_empty() || self.laps != LapsFilter::All || self.weather.is_some() || self.mine
    }
}

pub fn weathers(tracks: &[TrackCard]) -> Vec<String> {
    let mut all: Vec<String> = tracks.iter().map(|t| t.key.weather.clone()).collect();
    all.sort();
    all.dedup();
    if all.len() > 1 { all } else { Vec::new() }
}

pub const RECENT_CAP: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentSeason {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recents {
    seasons: Vec<RecentSeason>,
}

impl Recents {
    pub fn load(data: &std::path::Path) -> Self {
        let mut recents: Self = std::fs::read(Self::path(data))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        recents.seasons.retain(|s| content_listed(&s.id));
        recents
    }

    fn path(data: &std::path::Path) -> std::path::PathBuf {
        data.join("state").join("tt-recent.json")
    }

    pub fn save(&self, data: &std::path::Path) {
        let path = Self::path(data);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        if let Ok(json) = serde_json::to_vec(self) {
            std::fs::write(path, json).ok();
        }
    }

    pub fn visit(&mut self, id: &str, name: &str) {
        if !content_listed(id) {
            return;
        }
        self.seasons.retain(|s| s.id != id);
        self.seasons.insert(
            0,
            RecentSeason {
                id: id.to_owned(),
                name: name.to_owned(),
            },
        );
        self.seasons.truncate(RECENT_CAP);
    }

    pub fn list(&self) -> &[RecentSeason] {
        &self.seasons
    }
}
