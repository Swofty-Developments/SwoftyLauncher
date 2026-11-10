use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Setup {
    pub values: Vec<(&'static str, f32)>,
    pub compound: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Plain,
    Percent,
    Degrees,
    Psi,
    Kg,
}

pub type SetupRow = (&'static str, &'static str, Unit);

pub const SETUP_GROUPS: [(&str, &[SetupRow]); 7] = [
    (
        "Aerodynamics",
        &[
            ("front_wing", "Front wing", Unit::Plain),
            ("rear_wing", "Rear wing", Unit::Plain),
        ],
    ),
    (
        "Transmission",
        &[
            ("diff_on_throttle", "Differential on throttle", Unit::Percent),
            ("diff_off_throttle", "Differential off throttle", Unit::Percent),
            ("engine_braking", "Engine braking", Unit::Percent),
        ],
    ),
    (
        "Suspension geometry",
        &[
            ("front_camber", "Front camber", Unit::Degrees),
            ("rear_camber", "Rear camber", Unit::Degrees),
            ("front_toe", "Front toe", Unit::Degrees),
            ("rear_toe", "Rear toe", Unit::Degrees),
        ],
    ),
    (
        "Suspension",
        &[
            ("front_suspension", "Front suspension", Unit::Plain),
            ("rear_suspension", "Rear suspension", Unit::Plain),
            ("front_anti_roll_bar", "Front anti-roll bar", Unit::Plain),
            ("rear_anti_roll_bar", "Rear anti-roll bar", Unit::Plain),
            ("front_ride_height", "Front ride height", Unit::Plain),
            ("rear_ride_height", "Rear ride height", Unit::Plain),
        ],
    ),
    (
        "Brakes",
        &[
            ("brake_pressure", "Brake pressure", Unit::Percent),
            ("brake_bias", "Brake bias", Unit::Percent),
        ],
    ),
    (
        "Tyres",
        &[
            ("tyre_pressure_fl", "Front left pressure", Unit::Psi),
            ("tyre_pressure_fr", "Front right pressure", Unit::Psi),
            ("tyre_pressure_rl", "Rear left pressure", Unit::Psi),
            ("tyre_pressure_rr", "Rear right pressure", Unit::Psi),
        ],
    ),
    ("Fuel", &[("fuel_load", "Fuel load", Unit::Kg)]),
];

pub fn setup_value_text(v: f32, unit: Unit) -> String {
    let n = if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}").trim_end_matches('0').to_owned()
    };
    match unit {
        Unit::Plain => n,
        Unit::Percent => format!("{n}%"),
        Unit::Degrees => format!("{n}\u{b0}"),
        Unit::Psi => format!("{n}\u{a0}psi"),
        Unit::Kg => format!("{n} kg"),
    }
}

impl Setup {
    pub fn get(&self, key: &str) -> Option<f32> {
        self.values.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.compound.is_none()
    }

    pub fn difference(&self, mine: &Setup, key: &str) -> Option<f32> {
        let (a, b) = (self.get(key)?, mine.get(key)?);
        ((a - b).abs() > 0.004).then_some(a - b)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for (group, rows) in SETUP_GROUPS {
            let lines: Vec<String> = rows
                .iter()
                .filter_map(|(key, label, unit)| {
                    self.get(key)
                        .map(|v| format!("  {label}: {}", setup_value_text(v, *unit)))
                })
                .collect();
            let compound = (group == "Tyres").then(|| self.compound.clone()).flatten();
            if lines.is_empty() && compound.is_none() {
                continue;
            }
            out.push_str(group);
            out.push('\n');
            if let Some(c) = compound {
                out.push_str(&swofty_i18n::trf!("  Compound: {c}\n", c = c));
            }
            for l in lines {
                out.push_str(&l);
                out.push('\n');
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub rank: u32,
    pub lap_id: String,
    pub driver: Driver,
    pub time_ms: u64,
    pub sectors_ms: Vec<u64>,
    pub car: Option<String>,
    pub date: String,
    pub verified: bool,
    pub setup: Option<Setup>,
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
        std::fs::read(Self::path(data))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
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
            setup: None,
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

    fn card(name: &str, record: Option<u64>, drivers: u32, laps: u32, weather: &str) -> TrackCard {
        let d = Driver {
            id: "1".into(),
            name: "a".into(),
            avatar: None,
        };
        TrackCard {
            key: BoardKey {
                content: "c".into(),
                track: name.to_lowercase(),
                weather: weather.into(),
            },
            name: name.into(),
            flag: None,
            layout: None,
            record: record.map(|t| PodiumEntry {
                driver: d.clone(),
                time_ms: t,
            }),
            podium: Vec::new(),
            drivers,
            laps,
            custom: false,
        }
    }

    fn sample() -> Vec<TrackCard> {
        vec![
            card("Suzuka", Some(88_000), 5, 40, "dry"),
            card("Monza", Some(80_000), 9, 12, "dry"),
            card("Baku", None, 0, 0, "dry"),
            card("Spa", Some(103_000), 7, 90, "wet"),
        ]
    }

    #[test]
    fn tracks_filter_by_name_laps_weather_and_mine() {
        let tracks = sample();
        let none = HashSet::new();
        let names = |f: &TrackFilter, mine: &HashSet<BoardKey>| -> Vec<String> {
            f.apply(&tracks, mine).iter().map(|t| t.name.clone()).collect()
        };
        assert_eq!(
            names(&TrackFilter::default(), &none),
            ["Baku", "Monza", "Spa", "Suzuka"]
        );
        let q = TrackFilter {
            query: " su".into(),
            ..Default::default()
        };
        assert_eq!(names(&q, &none), ["Suzuka"]);
        let has = TrackFilter {
            laps: LapsFilter::Has,
            ..Default::default()
        };
        assert_eq!(names(&has, &none), ["Monza", "Spa", "Suzuka"]);
        let empty = TrackFilter {
            laps: LapsFilter::None,
            ..Default::default()
        };
        assert_eq!(names(&empty, &none), ["Baku"]);
        let wet = TrackFilter {
            weather: Some("wet".into()),
            ..Default::default()
        };
        assert_eq!(names(&wet, &none), ["Spa"]);
        let mine_only = TrackFilter {
            mine: true,
            ..Default::default()
        };
        let mine: HashSet<BoardKey> = [tracks[1].key.clone()].into();
        assert_eq!(names(&mine_only, &mine), ["Monza"]);
        assert!(mine_only.active() && !TrackFilter::default().active());
    }

    #[test]
    fn tracks_sort_by_record_drivers_and_laps() {
        let tracks = sample();
        let none = HashSet::new();
        let order = |sort| -> Vec<String> {
            TrackFilter {
                sort,
                ..Default::default()
            }
            .apply(&tracks, &none)
            .iter()
            .map(|t| t.name.clone())
            .collect()
        };
        assert_eq!(order(Sort::Record), ["Monza", "Suzuka", "Spa", "Baku"]);
        assert_eq!(order(Sort::Drivers), ["Monza", "Spa", "Suzuka", "Baku"]);
        assert_eq!(order(Sort::Laps), ["Spa", "Suzuka", "Monza", "Baku"]);
        assert_eq!(weathers(&tracks), ["dry", "wet"]);
        assert!(weathers(&tracks[..3]).is_empty());
    }

    #[test]
    fn recent_seasons_keep_the_newest_first_without_repeats_and_survive_a_restart() {
        let mut r = Recents::default();
        r.visit("a", "A");
        r.visit("b", "B");
        r.visit("a", "A");
        assert_eq!(
            r.list().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        for i in 0..20 {
            r.visit(&format!("s{i}"), "S");
        }
        assert_eq!(r.list().len(), RECENT_CAP);
        assert_eq!(r.list()[0].id, "s19");
        let dir = std::env::temp_dir().join(format!("swofty-tt-recent-{}", std::process::id()));
        r.save(&dir);
        assert_eq!(Recents::load(&dir), r);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(Recents::load(&dir), Recents::default());
    }

    fn setup_of(front: f32, compound: &str) -> Setup {
        Setup {
            values: vec![
                ("front_wing", front),
                ("brake_bias", 56.),
                ("front_camber", -3.5),
                ("fuel_load", 30.),
            ],
            compound: Some(compound.into()),
            changed: false,
        }
    }

    #[test]
    fn setup_values_read_like_the_game_screen() {
        assert_eq!(setup_value_text(25., Unit::Plain), "25");
        assert_eq!(setup_value_text(56., Unit::Percent), "56%");
        assert_eq!(setup_value_text(-3.5, Unit::Degrees), "-3.5\u{b0}");
        assert_eq!(setup_value_text(22.5, Unit::Psi), "22.5\u{a0}psi");
        assert_eq!(setup_value_text(30., Unit::Kg), "30 kg");
    }

    #[test]
    fn a_setup_is_grouped_copied_and_compared() {
        let a = setup_of(25., "soft");
        let b = setup_of(20., "soft");
        let text = a.text();
        assert!(text.contains("Aerodynamics\n  Front wing: 25\n"));
        assert!(text.contains("Tyres\n  Compound: soft"));
        assert!(!text.contains("Rear wing"));
        assert_eq!(a.difference(&b, "front_wing"), Some(5.));
        assert_eq!(a.difference(&b, "brake_bias"), None);
        assert_eq!(a.difference(&b, "rear_wing"), None);
        assert!(Setup::default().is_empty() && !a.is_empty());
    }
}
