//! What the boards server sends, and how it reads into the model.

use serde::Deserialize;
use swofty_core::online::timetrial::boards::{content_listed, track_addon};

use crate::features::online::model::*;

#[derive(Deserialize, Default)]
pub(super) struct UserDto {
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) avatar: Option<String>,
}

impl From<UserDto> for Driver {
    fn from(u: UserDto) -> Self {
        Driver {
            id: u.id,
            name: u.name,
            avatar: u.avatar,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RankDto {
    #[serde(default)]
    pub(super) rank: u32,
    pub(super) user: UserDto,
    #[serde(default)]
    pub(super) score: f32,
    #[serde(default)]
    pub(super) wins: u32,
    #[serde(default)]
    pub(super) podiums: u32,
}

impl From<RankDto> for RankEntry {
    fn from(r: RankDto) -> Self {
        RankEntry {
            rank: r.rank,
            driver: r.user.into(),
            points: r.score,
            wins: r.wins,
            podiums: r.podiums,
        }
    }
}

#[derive(Deserialize, Default)]
pub(super) struct RowsDto {
    #[serde(default)]
    pub(super) rows: Vec<RankDto>,
}

#[derive(Deserialize, Default)]
pub(super) struct CountsDto {
    #[serde(default)]
    pub(super) tracks: u32,
    #[serde(default)]
    pub(super) drivers: u32,
}

#[derive(Deserialize)]
pub(super) struct SeasonTopDto {
    pub(super) user: UserDto,
}

#[derive(Deserialize)]
pub(super) struct SeasonDto {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) cover: Option<String>,
    #[serde(default)]
    pub(super) counts: CountsDto,
    #[serde(default)]
    pub(super) top: Vec<SeasonTopDto>,
    #[serde(default)]
    pub(super) tracks: Vec<TrackDto>,
    #[serde(default)]
    pub(super) ranking: RowsDto,
}

#[derive(Deserialize)]
pub(super) struct RecordDto {
    pub(super) time_ms: u64,
    pub(super) user: UserDto,
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) weather: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct PodiumDto {
    pub(super) user: UserDto,
    pub(super) time_ms: u64,
}

#[derive(Deserialize)]
pub(super) struct TrackDto {
    #[serde(default)]
    pub(super) track: String,
    #[serde(default)]
    pub(super) weather: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) layout: Option<String>,
    #[serde(default)]
    pub(super) drivers: u32,
    #[serde(default)]
    pub(super) laps: u32,
    #[serde(default)]
    pub(super) record: Option<RecordDto>,
    #[serde(default)]
    pub(super) podium: Vec<PodiumDto>,
    #[serde(default, alias = "track_addon")]
    pub(super) addon: Option<String>,
}

pub(super) fn absolute(server: &str, url: Option<String>) -> Option<String> {
    url.filter(|u| !u.is_empty()).map(|u| match u.starts_with('/') {
        true => format!("{server}{u}"),
        false => u,
    })
}

pub(super) fn track_card(server: &str, content: &str, t: TrackDto, custom: bool) -> TrackCard {
    let weather = t.weather.clone().unwrap_or_else(|| "dry".to_owned());
    let content = t
        .record
        .as_ref()
        .and_then(|r| r.content.clone())
        .unwrap_or_else(|| content.to_owned());
    let weather = t
        .record
        .as_ref()
        .and_then(|r| r.weather.clone())
        .unwrap_or(weather);
    let name = match t.name.filter(|n| !n.is_empty()) {
        Some(n) => n,
        None => track_title(&t.track),
    };
    let addon = track_addon(&t.track, t.addon.as_deref()).map(str::to_owned);
    TrackCard {
        key: BoardKey {
            content,
            track: t.track,
            weather,
        },
        name,
        flag: t.country,
        layout: absolute(server, t.layout),
        record: t.record.map(|r| PodiumEntry {
            driver: r.user.into(),
            time_ms: r.time_ms,
        }),
        podium: t
            .podium
            .into_iter()
            .map(|p| PodiumEntry {
                driver: p.user.into(),
                time_ms: p.time_ms,
            })
            .collect(),
        drivers: t.drivers,
        laps: t.laps,
        custom,
        addon,
    }
}

pub fn track_title(code: &str) -> String {
    let code = code.rsplit(':').next().unwrap_or(code);
    code.split(['_', '-'])
        .map(crate::ui::format::title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn season_card(server: &str, s: &SeasonDto) -> SeasonCard {
    SeasonCard {
        id: s.id.clone(),
        name: s.name.clone(),
        cover: absolute(server, s.cover.clone()),
        tracks: s.counts.tracks,
        drivers: s.counts.drivers,
        top: s
            .top
            .iter()
            .map(|t| Driver {
                id: t.user.id.clone(),
                name: t.user.name.clone(),
                avatar: t.user.avatar.clone(),
            })
            .collect(),
    }
}

#[derive(Deserialize)]
pub(super) struct OverviewDto {
    #[serde(default)]
    pub(super) seasons: Vec<SeasonDto>,
    #[serde(default)]
    pub(super) global: RowsDto,
    #[serde(default)]
    pub(super) custom: CustomDto,
}

#[derive(Deserialize, Default)]
pub(super) struct CustomDto {
    #[serde(default)]
    pub(super) tracks: Vec<TrackDto>,
}

#[derive(Deserialize)]
pub(super) struct GameReplayDto {
    #[serde(default)]
    pub(super) size: u64,
    #[serde(default)]
    pub(super) setup: serde_json::Value,
}

#[derive(Deserialize)]
pub(super) struct BoardRowDto {
    pub(super) rank: u32,
    pub(super) lap_id: String,
    pub(super) user: UserDto,
    pub(super) time_ms: u64,
    #[serde(default)]
    pub(super) sectors_ms: Vec<u64>,
    #[serde(default)]
    pub(super) car_label: Option<String>,
    #[serde(default)]
    pub(super) set_at: String,
    #[serde(default)]
    pub(super) setup: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) assists: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) input: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) game_replay: Option<GameReplayDto>,
}

#[derive(Deserialize)]
pub(super) struct HistoryDto {
    pub(super) time_ms: u64,
    #[serde(default)]
    pub(super) set_at: String,
    pub(super) user: UserDto,
}

#[derive(Deserialize)]
pub(super) struct BoardDto {
    pub(super) content: String,
    pub(super) track: String,
    pub(super) weather: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) track_name: Option<String>,
    #[serde(default)]
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) custom: bool,
    #[serde(default)]
    pub(super) track_addon: Option<String>,
    #[serde(default)]
    pub(super) layout: Option<String>,
    #[serde(default)]
    pub(super) total: u32,
    #[serde(default)]
    pub(super) record: Option<RecordDto>,
    #[serde(default)]
    pub(super) rows: Vec<BoardRowDto>,
    #[serde(default)]
    pub(super) history: Vec<HistoryDto>,
}

pub(super) fn row(r: BoardRowDto) -> Row {
    Row {
        rank: r.rank,
        lap_id: r.lap_id,
        driver: r.user.into(),
        time_ms: r.time_ms,
        sectors_ms: r.sectors_ms,
        car: r.car_label.filter(|c| !c.is_empty()),
        date: r.set_at,
        input: input_of(r.input.as_ref(), r.assists.as_ref()),
        aids: aids_of(r.assists.as_ref()),
        setup: r.setup.as_ref().and_then(setup_of),
        game_replay: r.game_replay.is_some_and(|g| g.size > 0 && g.setup.is_object()),
    }
}

pub(in crate::features::online) fn input_of(
    input: Option<&serde_json::Value>,
    assists: Option<&serde_json::Value>,
) -> Option<InputDevice> {
    input
        .and_then(|v| v.as_str())
        .or_else(|| {
            assists
                .and_then(|a| a.get("input_device"))
                .and_then(|v| v.as_str())
        })
        .and_then(InputDevice::parse)
}

pub(in crate::features::online) fn aids_of(assists: Option<&serde_json::Value>) -> Option<Assists> {
    let map = assists?.as_object()?;
    let mut levels: Vec<(Aid, u32)> = Vec::new();
    for (key, value) in map {
        if let (Some(aid), Some(n)) = (
            Aid::from_key(key),
            value.as_u64().or_else(|| value.as_bool().map(u64::from)),
        ) {
            levels.push((aid, n as u32));
        }
    }
    (!levels.is_empty()).then_some(Assists { levels })
}

#[derive(Deserialize)]
pub(super) struct StandingDto {
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) rank: Option<u32>,
    #[serde(default)]
    pub(super) score: f32,
}

#[derive(Deserialize)]
pub(super) struct BestDto {
    #[serde(default)]
    pub(super) content: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) track: String,
    #[serde(default)]
    pub(super) track_name: Option<String>,
    #[serde(default)]
    pub(super) weather: String,
    pub(super) lap_id: String,
    pub(super) time_ms: u64,
    #[serde(default)]
    pub(super) rank: Option<u32>,
    #[serde(default)]
    pub(super) position: Option<u32>,
    #[serde(default)]
    pub(super) set_at: String,
    #[serde(default)]
    pub(super) setup: Option<serde_json::Value>,
}

pub(in crate::features::online) fn setup_of(v: &serde_json::Value) -> Option<Setup> {
    let map = v.as_object()?;
    let mut values = Vec::new();
    for (_, rows) in SETUP_GROUPS {
        for (key, _, _) in rows {
            if let Some(n) = map.get(*key).and_then(serde_json::Value::as_f64) {
                values.push((*key, n as f32));
            }
        }
    }
    let setup = Setup {
        values,
        compound: map
            .get("tyre_compound")
            .and_then(|c| c.as_str())
            .map(str::to_owned),
        changed: map
            .get("changed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    };
    (!setup.is_empty()).then_some(setup)
}

pub(super) fn best(b: BestDto) -> BestLap {
    BestLap {
        season: b.name,
        track: b.track_name.clone().unwrap_or_else(|| track_title(&b.track)),
        key: BoardKey {
            content: b.content,
            track: b.track,
            weather: b.weather,
        },
        time_ms: b.time_ms,
        position: b.position.or(b.rank).unwrap_or(0),
        lap_id: b.lap_id,
        date: b.set_at,
        setup: b.setup.as_ref().and_then(setup_of),
    }
}

#[derive(Deserialize)]
pub(super) struct RecentDto {
    pub(super) lap_id: String,
    #[serde(default)]
    pub(super) content: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) track: String,
    #[serde(default)]
    pub(super) track_name: Option<String>,
    #[serde(default)]
    pub(super) weather: String,
    pub(super) time_ms: u64,
    #[serde(default)]
    pub(super) set_at: String,
}

#[derive(Deserialize)]
pub(super) struct ProfileDto {
    pub(super) user: UserDto,
    #[serde(default)]
    pub(super) global: Option<RankDto2>,
    #[serde(default)]
    pub(super) seasons: Vec<StandingDto>,
    #[serde(default)]
    pub(super) bests: Vec<BestDto>,
    #[serde(default)]
    pub(super) recent: Vec<RecentDto>,
}

#[derive(Deserialize)]
pub(super) struct RankDto2 {
    #[serde(default)]
    pub(super) rank: u32,
    #[serde(default)]
    pub(super) score: f32,
    #[serde(default)]
    pub(super) wins: u32,
    #[serde(default)]
    pub(super) podiums: u32,
}

#[derive(Deserialize)]
pub(super) struct RankingDto {
    #[serde(default)]
    pub(super) total: u32,
    #[serde(default)]
    pub(super) next_offset: Option<u32>,
    #[serde(default)]
    pub(super) rows: Vec<RankDto>,
}

#[derive(Deserialize)]
pub(super) struct MeDto {
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) avatar: Option<String>,
    #[serde(default)]
    pub(super) global: Option<RankDto2>,
}

pub(super) fn overview_of(server: &str, dto: OverviewDto) -> Overview {
    Overview {
        seasons: dto
            .seasons
            .iter()
            .filter(|s| content_listed(&s.id))
            .map(|s| season_card(server, s))
            .collect(),
        global: dto.global.rows.into_iter().map(Into::into).collect(),
        custom: dto
            .custom
            .tracks
            .into_iter()
            .map(|t| track_card(server, "", t, true))
            .filter(|t| content_listed(&t.key.content))
            .collect(),
        me: None,
    }
}

pub(super) fn profile_of(dto: ProfileDto) -> Profile {
    let driver: Driver = dto.user.into();
    Profile {
        global: dto.global.map(|g| RankEntry {
            rank: g.rank,
            driver: driver.clone(),
            points: g.score,
            wins: g.wins,
            podiums: g.podiums,
        }),
        driver,
        seasons: dto
            .seasons
            .into_iter()
            .filter(|s| content_listed(&s.id))
            .filter_map(|s| {
                s.rank.map(|rank| SeasonStanding {
                    season: s.name,
                    rank,
                    points: s.score,
                })
            })
            .collect(),
        bests: dto
            .bests
            .into_iter()
            .filter(|b| content_listed(&b.content))
            .map(best)
            .collect(),
        recent: dto
            .recent
            .into_iter()
            .filter(|r| content_listed(&r.content))
            .map(|r| {
                best(BestDto {
                    content: r.content,
                    name: r.name,
                    track: r.track,
                    track_name: r.track_name,
                    weather: r.weather,
                    lap_id: r.lap_id,
                    time_ms: r.time_ms,
                    rank: None,
                    position: None,
                    set_at: r.set_at,
                    setup: None,
                })
            })
            .collect(),
    }
}
