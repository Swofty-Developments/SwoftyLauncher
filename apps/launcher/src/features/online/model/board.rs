//! A board's page, profiles and rankings, and how their rows are laid out and read.

use super::category::{HistoryPoint, Row};
use super::overview::{BoardKey, Driver, RankEntry, TrackCard};
use super::setup::Setup;

#[derive(Debug, Clone, PartialEq)]
pub struct BoardPage {
    pub track: TrackCard,
    pub season: String,
    pub rows: Vec<Row>,
    pub total: u32,
    pub history: Vec<HistoryPoint>,
    pub mine: Option<Row>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BestLap {
    pub season: String,
    pub track: String,
    pub key: BoardKey,
    pub time_ms: u64,
    pub position: u32,
    pub lap_id: String,
    pub date: String,
    pub setup: Option<Setup>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonStanding {
    pub season: String,
    pub rank: u32,
    pub points: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub driver: Driver,
    pub global: Option<RankEntry>,
    pub seasons: Vec<SeasonStanding>,
    pub bests: Vec<BestLap>,
    pub recent: Vec<BestLap>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankingPage {
    pub rows: Vec<RankEntry>,
    pub total: u32,
    pub next_offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Me {
    pub driver: Driver,
    pub entry: Option<RankEntry>,
}

pub const SHOW_SETUPS: bool = false;
pub const RANK_PAGE: u32 = 50;
pub const CHUNK: usize = 5;
pub const TOP_ROW_H: f32 = 64.;
pub const ROW_H: f32 = 38.4;
pub const CHUNK_H: f32 = 192.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Top(usize),
    Row(usize),
    Footer,
}

pub fn chunk_count(rows: usize) -> usize {
    let top = rows.min(3);
    1 + (rows - top + 1).div_ceil(CHUNK)
}

pub fn chunk_slots(chunk: usize, rows: usize) -> Vec<Slot> {
    let top = rows.min(3);
    if chunk == 0 {
        return (0..top).map(Slot::Top).collect();
    }
    (0..CHUNK)
        .filter_map(|v| {
            let idx = top + (chunk - 1) * CHUNK + v;
            match idx.cmp(&rows) {
                std::cmp::Ordering::Less => Some(Slot::Row(idx)),
                std::cmp::Ordering::Equal => Some(Slot::Footer),
                std::cmp::Ordering::Greater => None,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    pub rows: Vec<RankEntry>,
    pub total: u32,
    pub next: Option<u32>,
    pub loading: bool,
    pub error: Option<String>,
    pub started: bool,
}

impl Feed {
    pub fn apply(&mut self, offset: u32, page: RankingPage) {
        self.loading = false;
        self.error = None;
        self.started = true;
        self.total = page.total;
        self.next = page.next_offset;
        self.rows.truncate(offset as usize);
        self.rows.extend(page.rows);
    }

    pub fn failed(&mut self, message: String) {
        self.loading = false;
        self.started = true;
        self.error = Some(message);
    }

    pub fn wants_more(&self, visible_end: usize) -> Option<u32> {
        let next = self.next?;
        (!self.loading && self.error.is_none() && visible_end + 12 >= self.rows.len()).then_some(next)
    }

    pub fn finished(&self) -> bool {
        self.started && self.next.is_none() && self.error.is_none() && !self.loading
    }
}

pub fn lap_url(lap_id: &str) -> String {
    format!("{SITE}/lap/{lap_id}")
}

pub const SITE: &str = "https://tt.f1.swofty.net";
pub const PAGE_SIZE: u32 = 50;

pub fn gap_text(ms: u64, leader: u64) -> String {
    if ms <= leader {
        return String::new();
    }
    format!("+{}", crate::ui::format::sector_time(ms - leader))
}

pub fn score_text(points: f32) -> String {
    let whole = points.round().max(0.) as u64;
    let digits = whole.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorMark {
    Overall,
    Personal,
    Plain,
}

pub fn sector_mark(time: u64, overall_best: Option<u64>, personal_best: Option<u64>) -> SectorMark {
    if overall_best == Some(time) {
        SectorMark::Overall
    } else if personal_best == Some(time) {
        SectorMark::Personal
    } else {
        SectorMark::Plain
    }
}

pub fn best_sectors(rows: &[Row], n: usize) -> Vec<Option<u64>> {
    (0..n)
        .map(|i| rows.iter().filter_map(|r| r.sectors_ms.get(i).copied()).min())
        .collect()
}
