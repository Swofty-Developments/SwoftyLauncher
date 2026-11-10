use super::api::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Full,
    Empty,
    Failing,
    Slow,
}

impl Mode {
    pub fn from_env() -> Option<Self> {
        match std::env::var("SWOFTY_ONLINE_MOCK").ok()?.as_str() {
            "empty" => Some(Self::Empty),
            "error" => Some(Self::Failing),
            "slow" => Some(Self::Slow),
            _ => Some(Self::Full),
        }
    }
}

pub struct MockBoards {
    pub mode: Mode,
}

const NAMES: [&str; 12] = [
    "swofty",
    "Tommaso23",
    "driver1",
    "apex_hunter",
    "kerbs",
    "lateBraker",
    "slipstream",
    "ersTom",
    "pitwall",
    "Nyx",
    "oversteer",
    "darkMode",
];

fn user(i: usize) -> User {
    User {
        id: format!("{}", 1000 + i),
        name: NAMES[i % NAMES.len()].to_owned(),
        avatar: None,
    }
}

fn gate(mode: Mode) -> Result<(), OnlineError> {
    match mode {
        Mode::Failing => Err(OnlineError::network(
            "couldn't reach the boards server: connection refused",
        )),
        Mode::Slow => {
            std::thread::sleep(Duration::from_secs(3600));
            Ok(())
        }
        _ => Ok(()),
    }
}

impl BoardsApi for MockBoards {
    fn seasons(&self) -> Result<Vec<Season>, OnlineError> {
        gate(self.mode)?;
        if self.mode == Mode::Empty {
            return Ok(Vec::new());
        }
        let track = |track: &str, laps, players, best| SeasonTrack {
            track: track.to_owned(),
            name: None,
            custom: false,
            weather: "dry".to_owned(),
            laps,
            players,
            best_ms: Some(best),
        };
        Ok(vec![
            Season {
                content: "swofty.swofty-2022/f1_2022".into(),
                name: "F1 2022".into(),
                tracks: vec![
                    track("bahrain", 41, 17, 91234),
                    track("jeddah", 28, 12, 88410),
                    track("monza", 19, 9, 80102),
                    track("spa", 33, 14, 103877),
                ],
            },
            Season {
                content: "swofty.classic-2019/f1_2019".into(),
                name: "F1 2019".into(),
                tracks: vec![track("melbourne", 12, 6, 84551)],
            },
            Season {
                content: "game/f1_2025".into(),
                name: "F1 25 (stock)".into(),
                tracks: Vec::new(),
            },
        ])
    }

    fn tracks(&self) -> Result<Vec<TrackEntry>, OnlineError> {
        gate(self.mode)?;
        if self.mode == Mode::Empty {
            return Ok(Vec::new());
        }
        let board = |content: &str, name: &str, laps, best, who: &str| TrackBoard {
            content: content.to_owned(),
            name: name.to_owned(),
            weather: "dry".to_owned(),
            laps,
            best_ms: Some(best),
            best_user: Some(who.to_owned()),
        };
        Ok(vec![
            TrackEntry {
                track: "bahrain".into(),
                name: None,
                addon: None,
                custom: false,
                boards: vec![board(
                    "swofty.swofty-2022/f1_2022",
                    "F1 2022",
                    41,
                    91234,
                    "swofty",
                )],
            },
            TrackEntry {
                track: "swofty.hockenheim:hockenheim".into(),
                name: Some("Hockenheimring".into()),
                addon: Some("swofty.hockenheim".into()),
                custom: true,
                boards: vec![
                    board("swofty.swofty-2022/f1_2022", "F1 2022", 12, 58815, "driver1"),
                    board("swofty.classic-2019/f1_2019", "F1 2019", 4, 61002, "kerbs"),
                ],
            },
            TrackEntry {
                track: "monza".into(),
                name: None,
                addon: None,
                custom: false,
                boards: vec![board(
                    "swofty.swofty-2022/f1_2022",
                    "F1 2022",
                    19,
                    80102,
                    "Tommaso23",
                )],
            },
        ])
    }

    fn board(
        &self,
        content: &str,
        track: &str,
        weather: &str,
        _limit: u32,
        _offset: u32,
    ) -> Result<Board, OnlineError> {
        gate(self.mode)?;
        let rows = if self.mode == Mode::Empty {
            Vec::new()
        } else {
            (0..12)
                .map(|i| {
                    let time = 91234 + i as u64 * 437 + (i as u64 * i as u64) * 11;
                    let s1 = time * 32 / 100;
                    let s2 = time * 34 / 100;
                    BoardRow {
                        rank: i as u32 + 1,
                        lap_id: format!("3f9c0a1e5b7d4c{i:02}"),
                        user: user(i),
                        time_ms: time,
                        sectors_ms: vec![s1, s2, time - s1 - s2],
                        car: Some(Car {
                            team: Some("ferrari".into()),
                            driver: Some("leclerc".into()),
                        }),
                        set_at: format!("2026-11-{:02}T04:10:11Z", 10 - (i % 9)),
                        status: if i == 4 { "unranked" } else { "ranked" }.into(),
                    }
                })
                .collect()
        };
        Ok(Board {
            content: content.to_owned(),
            track: track.to_owned(),
            weather: weather.to_owned(),
            name: "F1 2022".into(),
            track_name: None,
            total: rows.len() as u32,
            rows,
        })
    }

    fn driver(&self, id: &str) -> Result<Driver, OnlineError> {
        gate(self.mode)?;
        let i = id.parse::<usize>().unwrap_or(1000) - 1000;
        let best = |track: &str, time, rank| DriverBest {
            content: "swofty.swofty-2022/f1_2022".into(),
            name: "F1 2022".into(),
            track: track.to_owned(),
            track_name: None,
            weather: "dry".into(),
            lap_id: format!("a1b2c3d4e5f6{:04}", time % 10000),
            time_ms: time,
            rank: Some(rank),
            status: "ranked".into(),
            set_at: "2026-11-09T18:22:01Z".into(),
        };
        Ok(Driver {
            user: user(i),
            bests: vec![
                best("bahrain", 91234, 1),
                best("jeddah", 88910, 3),
                best("monza", 80502, 2),
                best("spa", 104310, 5),
            ],
        })
    }
}
