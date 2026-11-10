use serde::Deserialize;
use std::fmt;
use std::time::Duration;
use swofty_i18n::trf;

pub const SITE: &str = "https://tt.f1.swofty.net";
pub const PAGE_SIZE: u32 = 50;

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SeasonTrack {
    pub track: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub custom: bool,
    pub weather: String,
    pub laps: u32,
    pub players: u32,
    #[serde(default)]
    pub best_ms: Option<u64>,
}

impl SeasonTrack {
    pub fn title(&self) -> String {
        track_title(&self.track, self.name.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Season {
    pub content: String,
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<SeasonTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrackBoard {
    pub content: String,
    pub name: String,
    pub weather: String,
    pub laps: u32,
    #[serde(default)]
    pub best_ms: Option<u64>,
    #[serde(default)]
    pub best_user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrackEntry {
    pub track: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub addon: Option<String>,
    #[serde(default)]
    pub custom: bool,
    #[serde(default)]
    pub boards: Vec<TrackBoard>,
}

impl TrackEntry {
    pub fn title(&self) -> String {
        track_title(&self.track, self.name.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Car {
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub driver: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BoardRow {
    pub rank: u32,
    pub lap_id: String,
    pub user: User,
    pub time_ms: u64,
    #[serde(default)]
    pub sectors_ms: Vec<u64>,
    #[serde(default)]
    pub car: Option<Car>,
    pub set_at: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Board {
    pub content: String,
    pub track: String,
    pub weather: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub track_name: Option<String>,
    pub total: u32,
    #[serde(default)]
    pub rows: Vec<BoardRow>,
}

impl Board {
    pub fn title(&self) -> String {
        track_title(&self.track, self.track_name.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DriverBest {
    pub content: String,
    #[serde(default)]
    pub name: String,
    pub track: String,
    #[serde(default)]
    pub track_name: Option<String>,
    pub weather: String,
    pub lap_id: String,
    pub time_ms: u64,
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(default)]
    pub status: String,
    pub set_at: String,
}

impl DriverBest {
    pub fn title(&self) -> String {
        track_title(&self.track, self.track_name.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Driver {
    pub user: User,
    #[serde(default)]
    pub bests: Vec<DriverBest>,
}

pub fn track_title(code: &str, name: Option<&str>) -> String {
    if let Some(n) = name.filter(|n| !n.is_empty()) {
        return n.to_owned();
    }
    let code = code.rsplit(':').next().unwrap_or(code);
    code.split(['_', '-'])
        .map(crate::format::title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn lap_url(lap_id: &str) -> String {
    format!("{SITE}/lap/{lap_id}")
}

pub trait BoardsApi: Send + Sync {
    fn seasons(&self) -> Result<Vec<Season>, OnlineError>;
    fn tracks(&self) -> Result<Vec<TrackEntry>, OnlineError>;
    fn board(
        &self,
        content: &str,
        track: &str,
        weather: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Board, OnlineError>;
    fn driver(&self, id: &str) -> Result<Driver, OnlineError>;
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
    fn seasons(&self) -> Result<Vec<Season>, OnlineError> {
        self.get("/v1/tt/boards")
    }

    fn tracks(&self) -> Result<Vec<TrackEntry>, OnlineError> {
        self.get("/v1/tt/tracks")
    }

    fn board(
        &self,
        content: &str,
        track: &str,
        weather: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Board, OnlineError> {
        self.get(&format!(
            "/v1/tt/boards/{}/{}?weather={}&limit={limit}&offset={offset}",
            encode(content),
            encode(track),
            encode(weather)
        ))
    }

    fn driver(&self, id: &str) -> Result<Driver, OnlineError> {
        self.get(&format!("/v1/tt/users/{}", encode(id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_boards_list() {
        let seasons: Vec<Season> = serde_json::from_str(
            r#"[{"content":"swofty.swofty-2022/f1_2022","name":"F1 2022",
              "tracks":[{"track":"bahrain","weather":"dry","laps":41,"players":17,"best_ms":91234}]},
              {"content":"game/f1_2025","name":"F1 25 (stock)","tracks":[]}]"#,
        )
        .unwrap();
        assert_eq!(seasons[0].tracks[0].title(), "Bahrain");
        assert!(seasons[1].tracks.is_empty());
    }

    #[test]
    fn parses_a_board_and_a_custom_track() {
        let board: Board = serde_json::from_str(
            r#"{"content":"c/x","track":"bahrain","weather":"dry","name":"F1 2022","total":1,"limit":50,"offset":0,
               "rows":[{"rank":1,"lap_id":"3f9c","user":{"id":"1","name":"swofty","avatar":null},"time_ms":91234,
               "sectors_ms":[29001,31002,31231],"car":{"team":"ferrari","vehicle":"sf-22","driver":"leclerc"},
               "assists":{"abs":true},"set_at":"2026-11-10T04:10:11Z","status":"ranked"}]}"#,
        )
        .unwrap();
        assert_eq!(board.rows[0].sectors_ms.len(), 3);
        assert_eq!(track_title("swofty.hockenheim:hockenheim", None), "Hockenheim");
        assert_eq!(track_title("x", Some("Hockenheimring")), "Hockenheimring");
    }

    #[test]
    fn encodes_the_slash_and_colon() {
        assert_eq!(encode("a.b/c_d"), "a.b%2Fc_d");
        assert_eq!(encode("addon:track"), "addon%3Atrack");
    }

    #[test]
    fn reads_the_servers_error() {
        assert_eq!(error_text(404, br#"{"error":"no such lap"}"#), "no such lap");
        assert!(error_text(502, b"<html>").contains("502"));
    }

    #[test]
    fn an_unreachable_server_is_a_network_error() {
        let e = HttpBoards::with_server("http://127.0.0.1:1")
            .seasons()
            .unwrap_err();
        assert_eq!(e.status, 0);
        assert!(e.message.contains("couldn't reach"));
    }
}
