use std::time::Duration;
use swofty_core::online::timetrial::boards::{content_listed, track_addon};
use swofty_i18n::trf;

use super::model::*;

mod dto;
#[cfg(test)]
mod tests;

use dto::{
    BoardDto, MeDto, OverviewDto, ProfileDto, RankingDto, SeasonDto, absolute, overview_of, profile_of, row,
    season_card, track_card, track_title,
};

pub trait BoardsApi: Send + Sync {
    fn overview(&self, cat: &Category) -> Result<Overview, OnlineError>;
    fn season(&self, id: &str, cat: &Category) -> Result<SeasonPage, OnlineError>;
    fn board(&self, key: &BoardKey, cat: &Category, offset: u32) -> Result<BoardPage, OnlineError>;
    fn profile(&self, id: &str, cat: &Category) -> Result<Profile, OnlineError>;
    fn rankings(&self, scope: &str, cat: &Category, offset: u32) -> Result<RankingPage, OnlineError>;
    fn me(&self) -> Result<Option<Me>, OnlineError>;
    fn set_token(&self, token: Option<String>);
}

pub struct HttpBoards {
    server: String,
    token: std::sync::Mutex<Option<String>>,
}

impl HttpBoards {
    pub fn new() -> Self {
        Self::with_server(&swofty_core::catalog::base_url())
    }

    pub fn with_server(server: &str) -> Self {
        Self {
            server: server.trim().trim_end_matches('/').to_owned(),
            token: std::sync::Mutex::new(None),
        }
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, OnlineError> {
        let url = format!("{}{path}", self.server);
        let token = self.token.lock().ok().and_then(|t| t.clone());
        let mut request = ureq::get(&url)
            .header("User-Agent", "swofty-launcher")
            .header("Accept", "application/json");
        if let Some(token) = token {
            request = request.header("Authorization", &["Bearer ", token.as_str()].concat());
        }
        let sent = request
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

fn lead(cat: &Category) -> String {
    let q = cat.query();
    if q.is_empty() {
        q
    } else {
        format!("?{}", q.trim_start_matches('&'))
    }
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
    fn set_token(&self, token: Option<String>) {
        if let Ok(mut t) = self.token.lock() {
            *t = token;
        }
    }

    fn rankings(&self, scope: &str, cat: &Category, offset: u32) -> Result<RankingPage, OnlineError> {
        let dto: RankingDto = self.get(&format!(
            "/v1/tt/rankings?season={}&limit={RANK_PAGE}&offset={offset}{}",
            encode(scope),
            cat.query()
        ))?;
        Ok(RankingPage {
            rows: dto.rows.into_iter().map(Into::into).collect(),
            total: dto.total,
            next_offset: dto.next_offset,
        })
    }

    fn me(&self) -> Result<Option<Me>, OnlineError> {
        if self.token.lock().ok().is_none_or(|t| t.is_none()) {
            return Ok(None);
        }
        match self.get::<MeDto>("/v1/tt/me") {
            Ok(dto) => {
                let driver = Driver {
                    id: dto.id,
                    name: dto.name,
                    avatar: dto.avatar,
                };
                Ok(Some(Me {
                    entry: dto.global.map(|g| RankEntry {
                        rank: g.rank,
                        driver: driver.clone(),
                        points: g.score,
                        wins: g.wins,
                        podiums: g.podiums,
                    }),
                    driver,
                }))
            }
            Err(e) if e.status == 401 => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn overview(&self, cat: &Category) -> Result<Overview, OnlineError> {
        let dto: OverviewDto = self.get(&["/v1/tt/overview", &lead(cat)].concat())?;
        Ok(overview_of(&self.server, dto))
    }

    fn season(&self, id: &str, cat: &Category) -> Result<SeasonPage, OnlineError> {
        let dto: SeasonDto = self.get(&format!("/v1/tt/seasons/{}{}", encode(id), lead(cat)))?;
        let season = season_card(&self.server, &dto);
        let tracks = dto
            .tracks
            .into_iter()
            .map(|t| track_card(&self.server, id, t, false))
            .filter(|t| content_listed(&t.key.content))
            .collect();
        Ok(SeasonPage {
            season,
            ranking: dto.ranking.rows.into_iter().map(Into::into).collect(),
            tracks,
        })
    }

    fn board(&self, key: &BoardKey, cat: &Category, offset: u32) -> Result<BoardPage, OnlineError> {
        let mut path = format!(
            "/v1/tt/boards/{}/{}?weather={}&limit={PAGE_SIZE}&offset={offset}",
            encode(&key.content),
            encode(&key.track),
            encode(&key.weather)
        );
        if SHOW_SETUPS {
            path.push_str("&setup=1");
        }
        path.push_str(&cat.query());
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
            addon: track_addon(&dto.track, dto.track_addon.as_deref()).map(str::to_owned),
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

    fn profile(&self, id: &str, cat: &Category) -> Result<Profile, OnlineError> {
        let dto: ProfileDto = self.get(&format!("/v1/tt/users/{}{}", encode(id), lead(cat)))?;
        Ok(profile_of(dto))
    }
}
