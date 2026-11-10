use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const STATE_FILE: &str = "swofty/home_menu.json";
pub const OPEN_FILE: &str = "swofty/open_url.json";
const CACHE_DIR: &str = "cache/home_menu";
const VIDEO_CACHE: &str = "video.json";
const THUMBNAIL: &str = "thumbnail.jpg";
const AVATAR: &str = "multiplayer/avatar.png";
const VIDEO_ROUTE: &str = "/v1/video/latest";
const THUMBNAIL_LIMIT: u64 = 2 * 1024 * 1024;
const HOSTS: [&str; 3] = ["www.youtube.com", "youtube.com", "youtu.be"];
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub url: String,
    pub thumbnail_url: String,
    #[serde(default)]
    pub published_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoCard {
    pub title: String,
    pub url: String,
    pub thumbnail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Discord {
    pub signed_in: bool,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct State {
    pub schema: u32,
    pub launcher_version: String,
    pub discord: Discord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoCard>,
}

#[derive(Debug, Deserialize)]
struct OpenRequest {
    url: String,
}

pub fn video_url_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.contains('@') && !authority.contains(':') && HOSTS.contains(&authority.to_ascii_lowercase().as_str())
}

pub fn parse_open_request(text: &str) -> Option<String> {
    let request: OpenRequest = serde_json::from_str(text).ok()?;
    video_url_allowed(&request.url).then_some(request.url)
}

pub fn take_open_request(game: &Path) -> Option<String> {
    let path = game.join(OPEN_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    std::fs::remove_file(&path).ok();
    parse_open_request(&text)
}

fn base_url() -> String {
    std::env::var(crate::catalog::URL_ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::catalog::DEFAULT_URL.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

fn get(url: &str) -> Option<ureq::Body> {
    let response = ureq::get(url)
        .header("User-Agent", crate::catalog::agent_name())
        .config()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .call()
        .ok()?;
    response.status().is_success().then(|| response.into_body())
}

fn fetch_video(registry: &str) -> Option<Video> {
    let text = get(&format!("{registry}{VIDEO_ROUTE}"))?
        .with_config()
        .limit(64 * 1024)
        .read_to_string()
        .ok()?;
    let video: Video = serde_json::from_str(&text).ok()?;
    (video_url_allowed(&video.url) && !video.id.is_empty() && !video.thumbnail_url.is_empty()).then_some(video)
}

fn fetch_thumbnail(url: &str, to: &Path) -> bool {
    let Some(mut body) = get(url) else {
        return false;
    };
    let Ok(bytes) = body.with_config().limit(THUMBNAIL_LIMIT).read_to_vec() else {
        return false;
    };
    let looks_like_image = bytes.starts_with(&[0xff, 0xd8]) || bytes.starts_with(b"\x89PNG");
    if !looks_like_image {
        return false;
    }
    std::fs::create_dir_all(to.parent().unwrap_or(Path::new("."))).is_ok() && std::fs::write(to, bytes).is_ok()
}

fn cached_video(data: &Path) -> Option<Video> {
    let dir = data.join(CACHE_DIR);
    let video: Video = serde_json::from_slice(&std::fs::read(dir.join(VIDEO_CACHE)).ok()?).ok()?;
    (video_url_allowed(&video.url) && dir.join(THUMBNAIL).is_file()).then_some(video)
}

pub fn latest_video(data: &Path, registry: &str) -> Option<VideoCard> {
    let dir = data.join(CACHE_DIR);
    let fresh = fetch_video(registry).filter(|v| fetch_thumbnail(&v.thumbnail_url, &dir.join(THUMBNAIL)));
    if let Some(video) = &fresh {
        std::fs::write(dir.join(VIDEO_CACHE), serde_json::to_vec(video).ok()?).ok()?;
    }
    let video = fresh.or_else(|| cached_video(data))?;
    Some(VideoCard {
        title: video.title,
        url: video.url,
        thumbnail: dir.join(THUMBNAIL).to_string_lossy().into_owned(),
    })
}

pub fn discord(data: &Path) -> Discord {
    match crate::mpauth::load(data) {
        Some(session) => {
            let avatar: PathBuf = data.join(AVATAR);
            Discord {
                signed_in: true,
                name: session.name,
                avatar: avatar.is_file().then(|| avatar.to_string_lossy().into_owned()),
            }
        }
        None => Discord {
            signed_in: false,
            name: String::new(),
            avatar: None,
        },
    }
}

pub fn state(data: &Path, version: &str, video: Option<VideoCard>) -> State {
    State {
        schema: 1,
        launcher_version: version.to_owned(),
        discord: discord(data),
        video,
    }
}

pub fn write(game: &Path, state: &State) -> crate::Result<()> {
    crate::write_json(&game.join(STATE_FILE), state)
}

pub fn publish(data: &Path, game: &Path, version: &str) {
    let cached = cached_video(data).map(|v| VideoCard {
        title: v.title,
        url: v.url,
        thumbnail: data.join(CACHE_DIR).join(THUMBNAIL).to_string_lossy().into_owned(),
    });
    let _ = write(game, &state(data, version, cached));
    let (data, game, version) = (data.to_path_buf(), game.to_path_buf(), version.to_owned());
    std::thread::spawn(move || {
        if let Some(card) = latest_video(&data, &base_url()) {
            let _ = write(&game, &state(&data, &version, Some(card)));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-home-menu-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn video_json(thumb: &str, url: &str) -> Vec<u8> {
        format!(
            r#"{{"id":"abc123","title":"Latest","url":"{url}","thumbnail_url":"{thumb}","published_at":"2026-11-09T10:00:00Z"}}"#
        )
        .into_bytes()
    }

    #[test]
    fn only_youtube_hosts_over_https_are_opened() {
        assert!(video_url_allowed("https://www.youtube.com/watch?v=abc"));
        assert!(video_url_allowed("https://youtu.be/abc"));
        assert!(video_url_allowed("https://YOUTUBE.com/watch?v=abc"));
        assert!(!video_url_allowed("http://www.youtube.com/watch?v=abc"));
        assert!(!video_url_allowed("https://www.youtube.com.evil.example/watch"));
        assert!(!video_url_allowed("https://evil.example/https://www.youtube.com/"));
        assert!(!video_url_allowed("https://www.youtube.com@evil.example/"));
        assert!(!video_url_allowed("https://www.youtube.com:8443/"));
        assert!(!video_url_allowed("file:///etc/passwd"));
        assert!(!video_url_allowed(""));
    }

    #[test]
    fn an_open_request_is_taken_once_and_validated() {
        let game = temp("open");
        std::fs::create_dir_all(game.join("swofty")).unwrap();
        std::fs::write(game.join(OPEN_FILE), r#"{"url":"https://youtu.be/abc"}"#).unwrap();
        assert_eq!(take_open_request(&game).as_deref(), Some("https://youtu.be/abc"));
        assert_eq!(take_open_request(&game), None);
        std::fs::write(game.join(OPEN_FILE), r#"{"url":"https://evil.example/"}"#).unwrap();
        assert_eq!(take_open_request(&game), None);
        assert!(!game.join(OPEN_FILE).exists());
        std::fs::remove_dir_all(&game).ok();
    }

    #[test]
    fn a_fetch_saves_the_thumbnail_and_a_dead_server_falls_back_to_the_cache() {
        let data = temp("cache");
        let jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 9, 9];
        let registry = serve_at(free_port(), jpeg.clone(), "https://youtu.be/abc");
        let card = latest_video(&data, &registry).expect("fresh card");
        assert_eq!(card.url, "https://youtu.be/abc");
        assert_eq!(std::fs::read(&card.thumbnail).unwrap(), jpeg);
        let dead = "http://127.0.0.1:9";
        let again = latest_video(&data, dead).expect("cached card");
        assert_eq!(again, card);
        let empty = temp("none");
        assert!(latest_video(&empty, dead).is_none());
        std::fs::remove_dir_all(&data).ok();
        std::fs::remove_dir_all(&empty).ok();
    }

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    fn serve_at(port: u16, jpeg: Vec<u8>, url: &'static str) -> String {
        let registry = format!("http://127.0.0.1:{port}");
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        let thumb = format!("{registry}/t.jpg");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let (kind, body) = if head.contains(VIDEO_ROUTE) {
                    ("application/json", video_json(&thumb, url))
                } else {
                    ("image/jpeg", jpeg.clone())
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).ok();
                stream.write_all(&body).ok();
            }
        });
        registry
    }

    #[test]
    fn a_video_with_a_foreign_url_is_refused() {
        let data = temp("foreign");
        let registry = serve_at(free_port(), vec![0xff, 0xd8, 1], "https://evil.example/x");
        assert!(latest_video(&data, &registry).is_none());
        std::fs::remove_dir_all(&data).ok();
    }

    #[test]
    fn the_state_file_carries_version_sign_in_and_video() {
        let data = temp("state");
        let game = temp("game");
        let signed_out = state(&data, "1.8.37", None);
        assert!(!signed_out.discord.signed_in);
        let json = serde_json::to_value(&signed_out).unwrap();
        assert_eq!(json["launcher_version"], "1.8.37");
        assert_eq!(json["discord"]["signed_in"], false);
        assert!(json.get("video").is_none());
        let with = state(
            &data,
            "1.8.37",
            Some(VideoCard {
                title: "t".into(),
                url: "https://youtu.be/a".into(),
                thumbnail: "x.jpg".into(),
            }),
        );
        write(&game, &with).unwrap();
        let back: serde_json::Value = serde_json::from_slice(&std::fs::read(game.join(STATE_FILE)).unwrap()).unwrap();
        assert_eq!(back["video"]["url"], "https://youtu.be/a");
        std::fs::remove_dir_all(&data).ok();
        std::fs::remove_dir_all(&game).ok();
    }
}
