use std::fmt;

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

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonPage {
    pub season: SeasonCard,
    pub ranking: Vec<RankEntry>,
    pub tracks: Vec<TrackCard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub rank: u32,
    pub lap_id: String,
    pub driver: Driver,
    pub time_ms: u64,
    pub sectors_ms: Vec<u64>,
    pub car: Option<String>,
    pub date: String,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPoint {
    pub time_ms: u64,
    pub driver: Driver,
    pub date: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub ranked_only: bool,
    pub assists_off: bool,
    pub wheel: bool,
}

impl Filter {
    pub fn any(&self) -> bool {
        self.ranked_only || self.assists_off || self.wheel
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardPage {
    pub track: TrackCard,
    pub season: String,
    pub rows: Vec<Row>,
    pub total: u32,
    pub history: Vec<HistoryPoint>,
    pub mine: Option<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestLap {
    pub season: String,
    pub track: String,
    pub key: BoardKey,
    pub time_ms: u64,
    pub position: u32,
    pub lap_id: String,
    pub date: String,
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

pub fn lap_url(lap_id: &str) -> String {
    format!("{SITE}/lap/{lap_id}")
}

pub const SITE: &str = "https://tt.f1.swofty.net";
pub const PAGE_SIZE: u32 = 50;

pub fn gap_text(ms: u64, leader: u64) -> String {
    if ms <= leader {
        return String::new();
    }
    format!("+{}", crate::format::sector_time(ms - leader))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_and_scores_read_like_a_leaderboard() {
        assert_eq!(gap_text(91_671, 91_234), "+0.437");
        assert_eq!(gap_text(91_234, 91_234), "");
        assert_eq!(gap_text(160_000, 91_234), "+68.766");
        assert_eq!(score_text(8430.4), "8,430");
        assert_eq!(score_text(12.), "12");
        assert_eq!(score_text(1_234_567.), "1,234,567");
    }

    #[test]
    fn sectors_are_purple_for_the_overall_best_and_green_for_a_personal_best() {
        assert_eq!(
            sector_mark(29_000, Some(29_000), Some(29_000)),
            SectorMark::Overall
        );
        assert_eq!(
            sector_mark(29_400, Some(29_000), Some(29_400)),
            SectorMark::Personal
        );
        assert_eq!(sector_mark(29_800, Some(29_000), Some(29_400)), SectorMark::Plain);
    }

    #[test]
    fn the_best_sector_of_each_column_is_found() {
        let d = Driver {
            id: "1".into(),
            name: "a".into(),
            avatar: None,
        };
        let row = |s: Vec<u64>| Row {
            rank: 1,
            lap_id: "x".into(),
            driver: d.clone(),
            time_ms: s.iter().sum(),
            sectors_ms: s,
            car: None,
            date: String::new(),
            verified: true,
        };
        let rows = [row(vec![30, 31, 32]), row(vec![29, 33, 31])];
        assert_eq!(best_sectors(&rows, 3), [Some(29), Some(31), Some(31)]);
        assert_eq!(best_sectors(&[], 3), [None, None, None]);
    }

    #[test]
    fn the_trail_pushes_pops_and_cuts_back_to_a_crumb() {
        let mut t = Trail::new();
        assert_eq!(*t.current(), Screen::Landing);
        assert!(!t.back());
        t.push(Screen::Season("2016".into()), "2016");
        t.push(
            Screen::Board(BoardKey {
                content: "c".into(),
                track: "suzuka".into(),
                weather: "dry".into(),
            }),
            "Suzuka",
        );
        assert_eq!(t.crumbs(), [(2, "2016"), (3, "Suzuka")]);
        t.jump(2);
        assert_eq!(*t.current(), Screen::Season("2016".into()));
        t.push(Screen::Profile("7".into()), "swofty");
        assert!(t.back());
        assert_eq!(t.depth(), 2);
        t.push(Screen::Season("2016".into()), "2016");
        assert_eq!(t.depth(), 2);
    }
}
