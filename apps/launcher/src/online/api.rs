use serde::Deserialize;
use std::time::Duration;
use swofty_i18n::trf;

use super::model::*;

pub trait BoardsApi: Send + Sync {
    fn overview(&self) -> Result<Overview, OnlineError>;
    fn season(&self, id: &str) -> Result<SeasonPage, OnlineError>;
    fn board(&self, key: &BoardKey, filter: &Filter, offset: u32) -> Result<BoardPage, OnlineError>;
    fn profile(&self, id: &str) -> Result<Profile, OnlineError>;
}

#[derive(Deserialize, Default)]
struct UserDto {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    avatar: Option<String>,
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
struct RankDto {
    #[serde(default)]
    rank: u32,
    user: UserDto,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    wins: u32,
    #[serde(default)]
    podiums: u32,
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
struct RowsDto {
    #[serde(default)]
    rows: Vec<RankDto>,
}

#[derive(Deserialize, Default)]
struct CountsDto {
    #[serde(default)]
    tracks: u32,
    #[serde(default)]
    drivers: u32,
}

#[derive(Deserialize)]
struct SeasonTopDto {
    user: UserDto,
}

#[derive(Deserialize)]
struct SeasonDto {
    id: String,
    name: String,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    counts: CountsDto,
    #[serde(default)]
    top: Vec<SeasonTopDto>,
    #[serde(default)]
    tracks: Vec<TrackDto>,
    #[serde(default)]
    ranking: RowsDto,
}

#[derive(Deserialize)]
struct RecordDto {
    time_ms: u64,
    user: UserDto,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    weather: Option<String>,
}

#[derive(Deserialize)]
struct PodiumDto {
    user: UserDto,
    time_ms: u64,
}

#[derive(Deserialize)]
struct TrackDto {
    #[serde(default)]
    track: String,
    #[serde(default)]
    weather: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    drivers: u32,
    #[serde(default)]
    laps: u32,
    #[serde(default)]
    record: Option<RecordDto>,
    #[serde(default)]
    podium: Vec<PodiumDto>,
}

fn absolute(server: &str, url: Option<String>) -> Option<String> {
    url.filter(|u| !u.is_empty()).map(|u| match u.starts_with('/') {
        true => format!("{server}{u}"),
        false => u,
    })
}

fn track_card(server: &str, content: &str, t: TrackDto, custom: bool) -> TrackCard {
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
    }
}

pub fn track_title(code: &str) -> String {
    let code = code.rsplit(':').next().unwrap_or(code);
    code.split(['_', '-'])
        .map(crate::format::title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

fn season_card(server: &str, s: &SeasonDto) -> SeasonCard {
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
struct OverviewDto {
    #[serde(default)]
    seasons: Vec<SeasonDto>,
    #[serde(default)]
    global: RowsDto,
    #[serde(default)]
    custom: CustomDto,
}

#[derive(Deserialize, Default)]
struct CustomDto {
    #[serde(default)]
    tracks: Vec<TrackDto>,
}

#[derive(Deserialize)]
struct BoardRowDto {
    rank: u32,
    lap_id: String,
    user: UserDto,
    time_ms: u64,
    #[serde(default)]
    sectors_ms: Vec<u64>,
    #[serde(default)]
    car_label: Option<String>,
    #[serde(default)]
    set_at: String,
    #[serde(default)]
    verification: String,
}

#[derive(Deserialize)]
struct HistoryDto {
    time_ms: u64,
    #[serde(default)]
    set_at: String,
    user: UserDto,
}

#[derive(Deserialize)]
struct BoardDto {
    content: String,
    track: String,
    weather: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    track_name: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    custom: bool,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    record: Option<RecordDto>,
    #[serde(default)]
    rows: Vec<BoardRowDto>,
    #[serde(default)]
    history: Vec<HistoryDto>,
}

fn row(r: BoardRowDto) -> Row {
    Row {
        rank: r.rank,
        lap_id: r.lap_id,
        driver: r.user.into(),
        time_ms: r.time_ms,
        sectors_ms: r.sectors_ms,
        car: r.car_label.filter(|c| !c.is_empty()),
        date: r.set_at,
        verified: r.verification == "verified",
    }
}

#[derive(Deserialize)]
struct StandingDto {
    #[serde(default)]
    name: String,
    #[serde(default)]
    rank: Option<u32>,
    #[serde(default)]
    score: f32,
}

#[derive(Deserialize)]
struct BestDto {
    #[serde(default)]
    content: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    track: String,
    #[serde(default)]
    track_name: Option<String>,
    #[serde(default)]
    weather: String,
    lap_id: String,
    time_ms: u64,
    #[serde(default)]
    rank: Option<u32>,
    #[serde(default)]
    position: Option<u32>,
    #[serde(default)]
    set_at: String,
}

fn best(b: BestDto) -> BestLap {
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
    }
}

#[derive(Deserialize)]
struct RecentDto {
    lap_id: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    track: String,
    #[serde(default)]
    track_name: Option<String>,
    #[serde(default)]
    weather: String,
    time_ms: u64,
    #[serde(default)]
    set_at: String,
}

#[derive(Deserialize)]
struct ProfileDto {
    user: UserDto,
    #[serde(default)]
    global: Option<RankDto2>,
    #[serde(default)]
    seasons: Vec<StandingDto>,
    #[serde(default)]
    bests: Vec<BestDto>,
    #[serde(default)]
    recent: Vec<RecentDto>,
}

#[derive(Deserialize)]
struct RankDto2 {
    #[serde(default)]
    rank: u32,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    wins: u32,
    #[serde(default)]
    podiums: u32,
}

pub struct HttpBoards {
    server: String,
}

impl HttpBoards {
    pub fn new() -> Self {
        Self::with_server(&swofty_core::catalog::base_url())
    }

    pub fn with_server(server: &str) -> Self {
        Self {
            server: server.trim().trim_end_matches('/').to_owned(),
        }
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, OnlineError> {
        let url = format!("{}{path}", self.server);
        let sent = ureq::get(&url)
            .header("User-Agent", "swofty-launcher")
            .header("Accept", "application/json")
            .config()
            .timeout_global(Some(Duration::from_secs(8)))
            .timeout_connect(Some(Duration::from_secs(5)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .call();
        let mut response = sent.map_err(|e| {
            OnlineError::network(trf!(
                "couldn't reach {server}: {error}",
                server = self.server,
                error = e
            ))
        })?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| {
                OnlineError::network(trf!(
                    "reading the answer to GET {path}: {error}",
                    path = path,
                    error = e
                ))
            })?;
        if !(200..300).contains(&status) {
            return Err(OnlineError {
                status,
                message: error_text(status, &body),
            });
        }
        serde_json::from_slice(&body).map_err(|e| OnlineError {
            status,
            message: trf!(
                "the answer to GET {path} is not valid: {error}",
                path = path,
                error = e
            ),
        })
    }
}

impl Default for HttpBoards {
    fn default() -> Self {
        Self::new()
    }
}

fn error_text(status: u16, body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(|| trf!("the boards server answered HTTP {status}", status = status))
}

pub fn encode(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for b in part.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl BoardsApi for HttpBoards {
    fn overview(&self) -> Result<Overview, OnlineError> {
        let dto: OverviewDto = self.get("/v1/tt/overview")?;
        Ok(Overview {
            seasons: dto.seasons.iter().map(|s| season_card(&self.server, s)).collect(),
            global: dto.global.rows.into_iter().map(Into::into).collect(),
            custom: dto
                .custom
                .tracks
                .into_iter()
                .map(|t| track_card(&self.server, "", t, true))
                .collect(),
            me: None,
        })
    }

    fn season(&self, id: &str) -> Result<SeasonPage, OnlineError> {
        let dto: SeasonDto = self.get(&format!("/v1/tt/seasons/{}", encode(id)))?;
        let season = season_card(&self.server, &dto);
        let tracks = dto
            .tracks
            .into_iter()
            .map(|t| track_card(&self.server, id, t, false))
            .collect();
        Ok(SeasonPage {
            season,
            ranking: dto.ranking.rows.into_iter().map(Into::into).collect(),
            tracks,
        })
    }

    fn board(&self, key: &BoardKey, filter: &Filter, offset: u32) -> Result<BoardPage, OnlineError> {
        let mut path = format!(
            "/v1/tt/boards/{}/{}?weather={}&limit={PAGE_SIZE}&offset={offset}",
            encode(&key.content),
            encode(&key.track),
            encode(&key.weather)
        );
        if filter.ranked_only {
            path.push_str("&ranked=1");
        }
        if filter.assists_off {
            path.push_str("&assists=none");
        }
        if filter.wheel {
            path.push_str("&device=wheel");
        }
        let dto: BoardDto = self.get(&path)?;
        let track = TrackCard {
            key: BoardKey {
                content: dto.content.clone(),
                track: dto.track.clone(),
                weather: dto.weather.clone(),
            },
            name: dto.track_name.clone().unwrap_or_else(|| track_title(&dto.track)),
            flag: dto.country,
            layout: absolute(&self.server, dto.layout),
            record: dto.record.map(|r| PodiumEntry {
                driver: r.user.into(),
                time_ms: r.time_ms,
            }),
            podium: Vec::new(),
            drivers: dto.total,
            laps: dto.total,
            custom: dto.custom,
        };
        Ok(BoardPage {
            track,
            season: dto.name,
            rows: dto.rows.into_iter().map(row).collect(),
            total: dto.total,
            history: dto
                .history
                .into_iter()
                .map(|h| HistoryPoint {
                    time_ms: h.time_ms,
                    driver: h.user.into(),
                    date: h.set_at,
                })
                .collect(),
            mine: None,
        })
    }

    fn profile(&self, id: &str) -> Result<Profile, OnlineError> {
        let dto: ProfileDto = self.get(&format!("/v1/tt/users/{}", encode(id)))?;
        let driver: Driver = dto.user.into();
        Ok(Profile {
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
                .filter_map(|s| {
                    s.rank.map(|rank| SeasonStanding {
                        season: s.name,
                        rank,
                        points: s.score,
                    })
                })
                .collect(),
            bests: dto.bests.into_iter().map(best).collect(),
            recent: dto
                .recent
                .into_iter()
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
                    })
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_overview() {
        let dto: OverviewDto = serde_json::from_str(
            r#"{"seasons":[{"id":"a/f1_2022","name":"F1 2022","cover":"https://x/icon","counts":{"tracks":22,"drivers":17,"laps":410},
              "top":[{"rank":1,"user":{"id":"1","name":"swofty","avatar":null},"score":1834.21}]}],
              "global":{"total":1,"rows":[{"rank":1,"user":{"id":"1","name":"swofty","avatar":null},"score":187.4,"boards":31,"wins":12,"podiums":20}]},
              "custom":{"tracks":[{"track":"a.h:hockenheim","name":"Hockenheimring","country":"de","layout":null,"boards":2,"drivers":4,"laps":9,
                "record":{"time_ms":58815,"user":{"id":"2","name":"d2","avatar":null},"content":"a/f1_2022","weather":"dry"}}]}}"#,
        )
        .unwrap();
        let card = season_card("https://s", &dto.seasons[0]);
        assert_eq!((card.tracks, card.drivers, card.top.len()), (22, 17, 1));
        let track = track_card(
            "https://s",
            "",
            dto.custom.tracks.into_iter().next().unwrap(),
            true,
        );
        assert_eq!(track.key.content, "a/f1_2022");
        assert_eq!(track.record.unwrap().time_ms, 58815);
    }

    #[test]
    fn relative_art_is_pointed_at_the_server() {
        assert_eq!(
            absolute("https://s", Some("/v1/x.svg".into())).as_deref(),
            Some("https://s/v1/x.svg")
        );
        assert_eq!(
            absolute("https://s", Some("https://c/a.png".into())).as_deref(),
            Some("https://c/a.png")
        );
        assert_eq!(absolute("https://s", None), None);
        assert_eq!(absolute("https://s", Some(String::new())), None);
    }

    #[test]
    fn encodes_the_slash_and_colon() {
        assert_eq!(encode("a.b/c_d"), "a.b%2Fc_d");
        assert_eq!(encode("addon:track"), "addon%3Atrack");
    }

    #[test]
    fn an_unreachable_server_is_a_network_error() {
        let e = HttpBoards::with_server("http://127.0.0.1:1")
            .overview()
            .unwrap_err();
        assert_eq!(e.status, 0);
        assert!(e.message.contains("couldn't reach"));
    }
}
