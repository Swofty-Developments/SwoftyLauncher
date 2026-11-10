//! The Time Trial boards the launcher shows: a track's board and the laps waiting for an admin.

use serde::Deserialize;
use serde_json::json;

use super::api::{Api, ApiError};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Driver {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Row {
    #[serde(default)]
    pub rank: Option<u32>,
    pub lap_id: String,
    #[serde(alias = "user")]
    pub driver: Driver,
    pub time_ms: u64,
    #[serde(default, alias = "set_at")]
    pub date: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl Row {
    pub fn pending(&self) -> bool {
        self.status == "pending"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PendingLap {
    pub lap_id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub track: String,
    #[serde(default)]
    pub track_name: String,
    pub driver: Driver,
    pub time_ms: u64,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TrackEntry {
    pub track: String,
    #[serde(default = "dry")]
    pub weather: String,
    #[serde(default)]
    pub laps: u32,
    #[serde(default)]
    pub best_ms: Option<u64>,
    #[serde(default, alias = "name")]
    pub track_name: Option<String>,
}

fn dry() -> String {
    "dry".to_owned()
}

impl TrackEntry {
    pub fn title(&self) -> String {
        match &self.track_name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => self.track.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    #[serde(alias = "id")]
    pub content: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<TrackEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Rows<T> {
    List(Vec<T>),
    Wrapped {
        #[serde(alias = "laps", alias = "entries", alias = "boards")]
        rows: Vec<T>,
        #[serde(default)]
        next_offset: Option<u64>,
    },
}

impl<T> Rows<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            Self::List(v) | Self::Wrapped { rows: v, .. } => v,
        }
    }
}

/// `m:ss.mmm` for a lap time.
pub fn format_time(ms: u64) -> String {
    format!("{}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

/// The rows whose driver name contains `query`, ignoring case; every row for a blank query.
pub fn by_driver<'a>(rows: &'a [Row], query: &str) -> Vec<&'a Row> {
    let needle = query.trim().to_lowercase();
    rows.iter()
        .filter(|r| needle.is_empty() || r.driver.name.to_lowercase().contains(&needle))
        .collect()
}

fn segment(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Api {
    pub fn catalog(&self) -> Result<Vec<Group>, ApiError> {
        Ok(self.get_json::<Rows<Group>>("/v1/tt/boards")?.into_vec())
    }

    pub fn board(
        &self,
        content: &str,
        track: &str,
        weather: &str,
        query: Option<&str>,
    ) -> Result<Vec<Row>, ApiError> {
        let mut path = format!(
            "/v1/tt/boards/{}/{}?weather={}",
            content.split('/').map(segment).collect::<Vec<_>>().join("/"),
            segment(track),
            segment(weather)
        );
        if let Some(q) = query.filter(|q| !q.is_empty()) {
            path.push_str(&format!("&q={}", segment(q)));
        }
        Ok(self.get_json::<Rows<Row>>(&path)?.into_vec())
    }

    /// Every lap waiting for review, following the server's pages.
    pub fn pending_laps(&self) -> Result<Vec<PendingLap>, ApiError> {
        let mut all = Vec::new();
        let mut offset = 0u64;
        for _ in 0..40 {
            let page: Rows<PendingLap> =
                self.get_json(&format!("/v1/tt/admin/laps?status=pending&offset={offset}"))?;
            let next = match &page {
                Rows::Wrapped { next_offset, .. } => *next_offset,
                Rows::List(_) => None,
            };
            all.extend(page.into_vec());
            match next {
                Some(n) if n > offset => offset = n,
                _ => break,
            }
        }
        Ok(all)
    }

    pub fn accept_lap(&self, id: &str) -> Result<(), ApiError> {
        self.post_json(&format!("/v1/tt/admin/laps/{}/accept", segment(id)), &json!({}))
    }

    pub fn deny_lap(&self, id: &str) -> Result<(), ApiError> {
        self.post_json(&format!("/v1/tt/admin/laps/{}/deny", segment(id)), &json!({}))
    }

    pub fn ban_driver(&self, lap_id: &str, reason: &str) -> Result<(), ApiError> {
        self.post_json(
            &format!("/v1/tt/admin/laps/{}/ban", segment(lap_id)),
            &json!({ "reason": reason }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_lap_time() {
        assert_eq!(format_time(83_456), "1:23.456");
        assert_eq!(format_time(9_005), "0:09.005");
    }

    #[test]
    fn reads_a_bare_list_or_a_wrapped_one() {
        let bare = r#"[{"lap_id":"a","driver":{"id":"1","name":"X"},"time_ms":1,"status":"ranked"}]"#;
        let wrapped = format!(r#"{{"rows":{bare}}}"#);
        assert_eq!(
            serde_json::from_str::<Rows<Row>>(bare).unwrap().into_vec().len(),
            1
        );
        assert_eq!(
            serde_json::from_str::<Rows<Row>>(&wrapped)
                .unwrap()
                .into_vec()
                .len(),
            1
        );
    }

    #[test]
    fn search_filters_rows_by_driver_name_ignoring_case() {
        let row = |name: &str| Row {
            rank: None,
            lap_id: name.into(),
            driver: Driver {
                id: String::new(),
                name: name.into(),
                avatar: None,
            },
            time_ms: 1,
            date: String::new(),
            status: "ranked".into(),
            reasons: Vec::new(),
        };
        let rows = vec![row("Swofty"), row("Max"), row("swofty2")];
        assert_eq!(by_driver(&rows, "").len(), 3);
        assert_eq!(by_driver(&rows, " SWOF ").len(), 2);
        assert!(by_driver(&rows, "zzz").is_empty());
    }

    #[test]
    fn path_segments_are_escaped() {
        assert_eq!(segment("a b/c"), "a%20b%2Fc");
    }
}
