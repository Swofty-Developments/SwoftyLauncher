use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{Image, ImageFormat};

const LIMIT: u64 = 4 * 1024 * 1024;
pub(crate) const IN_FLIGHT: usize = 4;
pub(crate) const PER_MINUTE: usize = 20;
const MINUTE: Duration = Duration::from_secs(60);
const RETRY_AFTER: Duration = Duration::from_secs(60);
const RETRY_AFTER_MAX: Duration = Duration::from_secs(15 * 60);
const FAILED_FOR: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub(crate) enum Art {
    Raster(Arc<Image>),
    Svg(Arc<[u8]>),
}

enum Slot {
    Reading,
    Queued,
    Fetching,
    Loading,
    Ready(Art),
    Failed(Instant),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Job {
    Disk(String),
    Fetch(String),
    Load(String),
}

pub(crate) enum Done {
    Art(Art),
    Miss,
    Refused(Duration),
    Failed,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    pub jobs: Vec<Job>,
    pub wake: Option<Duration>,
}

pub(crate) struct Images {
    slots: HashMap<String, Slot>,
    dir: PathBuf,
    server: String,
    fetching: usize,
    starts: VecDeque<Instant>,
    paused: Option<Instant>,
}

impl Images {
    pub(crate) fn new(data: &Path, server: &str) -> Self {
        Self {
            slots: HashMap::new(),
            dir: data.join("cache").join("tt-images"),
            server: server.trim().trim_end_matches('/').to_owned(),
            fetching: 0,
            starts: VecDeque::new(),
            paused: None,
        }
    }

    pub(crate) fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    pub(crate) fn get(&self, url: &str) -> Option<&Art> {
        match self.slots.get(url) {
            Some(Slot::Ready(art)) => Some(art),
            _ => None,
        }
    }

    pub(crate) fn throttled(&self, url: &str) -> bool {
        on_server(url, &self.server)
    }

    pub(crate) fn wants(&self, url: &str, now: Instant) -> bool {
        match self.slots.get(url) {
            None | Some(Slot::Queued) => true,
            Some(Slot::Failed(until)) => *until <= now,
            _ => false,
        }
    }

    pub(crate) fn prefetch(&mut self, urls: impl IntoIterator<Item = String>, now: Instant) -> Vec<Job> {
        let free: Vec<String> = urls.into_iter().filter(|u| !self.throttled(u)).collect();
        self.plan(free, now).jobs
    }

    pub(crate) fn plan(&mut self, visible: impl IntoIterator<Item = String>, now: Instant) -> Plan {
        while self
            .starts
            .front()
            .is_some_and(|t| now.duration_since(*t) >= MINUTE)
        {
            self.starts.pop_front();
        }
        let mut plan = Plan::default();
        let mut waiting = false;
        let mut picked = HashSet::new();
        for url in visible {
            if !self.wants(&url, now) || !picked.insert(url.clone()) {
                continue;
            }
            let fetched_before = matches!(self.slots.get(&url), Some(Slot::Queued | Slot::Failed(_)));
            if !self.throttled(&url) {
                self.slots.insert(url.clone(), Slot::Loading);
                plan.jobs.push(Job::Load(url));
            } else if !fetched_before {
                self.slots.insert(url.clone(), Slot::Reading);
                plan.jobs.push(Job::Disk(url));
            } else if self.may_fetch(now) {
                self.fetching += 1;
                self.starts.push_back(now);
                self.slots.insert(url.clone(), Slot::Fetching);
                plan.jobs.push(Job::Fetch(url));
            } else {
                waiting = true;
                self.slots.insert(url, Slot::Queued);
            }
        }
        if waiting {
            plan.wake = self.wake(now);
        }
        plan
    }

    pub(crate) fn finish(&mut self, url: String, done: Done, now: Instant) {
        if matches!(self.slots.get(&url), Some(Slot::Fetching)) {
            self.fetching = self.fetching.saturating_sub(1);
        }
        let slot = match done {
            Done::Art(art) => Slot::Ready(art),
            Done::Miss => Slot::Queued,
            Done::Refused(wait) => {
                let until = now + wait;
                if self.throttled(&url) {
                    self.paused = Some(self.paused.map_or(until, |p| p.max(until)));
                }
                Slot::Failed(until)
            }
            Done::Failed => Slot::Failed(now + FAILED_FOR),
        };
        self.slots.insert(url, slot);
    }

    fn may_fetch(&self, now: Instant) -> bool {
        self.fetching < IN_FLIGHT && self.starts.len() < PER_MINUTE && self.paused.is_none_or(|p| p <= now)
    }

    fn wake(&self, now: Instant) -> Option<Duration> {
        let paused = self.paused.filter(|p| *p > now);
        let full = (self.starts.len() >= PER_MINUTE)
            .then(|| self.starts.front().map(|t| *t + MINUTE))
            .flatten();
        paused.max(full).map(|at| at.saturating_duration_since(now))
    }
}

impl Job {
    pub(crate) fn url(&self) -> &str {
        match self {
            Job::Disk(url) | Job::Fetch(url) | Job::Load(url) => url,
        }
    }

    pub(crate) fn run(&self, dir: &Path, token: Option<&str>) -> Done {
        match self {
            Job::Disk(url) => cached(dir, url).map_or(Done::Miss, Done::Art),
            Job::Fetch(url) => fetch(dir, url, token),
            Job::Load(url) => cached(dir, url).map_or_else(|| fetch(dir, url, None), Done::Art),
        }
    }
}

fn on_server(url: &str, server: &str) -> bool {
    !server.is_empty()
        && url
            .strip_prefix(server)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || rest.starts_with('?'))
}

pub(crate) fn retry_after(header: Option<&str>) -> Duration {
    header
        .and_then(|h| h.trim().parse::<u64>().ok())
        .map_or(RETRY_AFTER, |s| Duration::from_secs(s.max(1)))
        .min(RETRY_AFTER_MAX)
}

pub(crate) fn cache_name(url: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub(crate) fn decode(bytes: Vec<u8>) -> Option<Art> {
    let head = bytes.iter().take(256).copied().collect::<Vec<_>>();
    let text = String::from_utf8_lossy(&head);
    if text.contains("<svg") || text.trim_start().starts_with("<?xml") {
        return Some(Art::Svg(Arc::from(bytes)));
    }
    let format = if bytes.starts_with(b"\x89PNG") {
        ImageFormat::Png
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        ImageFormat::Jpeg
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        ImageFormat::Webp
    } else if bytes.starts_with(b"GIF8") {
        ImageFormat::Gif
    } else {
        return None;
    };
    Some(Art::Raster(Arc::new(Image::from_bytes(format, bytes))))
}

pub(crate) fn cached(dir: &Path, url: &str) -> Option<Art> {
    decode(std::fs::read(dir.join(cache_name(url))).ok()?)
}

pub(crate) fn fetch(dir: &Path, url: &str, token: Option<&str>) -> Done {
    if let Some(name) = url.strip_prefix("mock:") {
        if !cfg!(debug_assertions) || name.contains("..") || name.contains('/') {
            return Done::Failed;
        }
        let art = std::env::var_os("SWOFTY_ONLINE_MOCK_ART")
            .and_then(|root| std::fs::read(PathBuf::from(root).join(name)).ok())
            .and_then(decode);
        return art.map_or(Done::Failed, Done::Art);
    }
    if !url.starts_with("https://") && !cfg!(debug_assertions) {
        return Done::Failed;
    }
    let mut request = ureq::get(url).header("User-Agent", "swofty-launcher");
    if let Some(token) = token {
        request = request.header("Authorization", &["Bearer ", token].concat());
    }
    let sent = request
        .config()
        .timeout_global(Some(Duration::from_secs(12)))
        .max_redirects(if token.is_some() { 0 } else { 3 })
        .http_status_as_error(false)
        .build()
        .call();
    let Ok(mut response) = sent else {
        return Done::Failed;
    };
    if response.status().as_u16() == 429 {
        let header = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok());
        return Done::Refused(retry_after(header));
    }
    if !response.status().is_success() {
        return Done::Failed;
    }
    let Ok(bytes) = response.body_mut().with_config().limit(LIMIT).read_to_vec() else {
        return Done::Failed;
    };
    let Some(art) = decode(bytes.clone()) else {
        return Done::Failed;
    };
    if std::fs::create_dir_all(dir).is_ok() {
        let name = cache_name(url);
        let part = dir.join(format!("{name}.part"));
        if std::fs::write(&part, bytes).is_ok() && std::fs::rename(&part, dir.join(&name)).is_err() {
            std::fs::remove_file(&part).ok();
        }
    }
    Done::Art(art)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const SERVER: &str = "https://api.example";

    fn images() -> Images {
        Images::new(Path::new("/nonexistent"), SERVER)
    }

    fn layout(i: usize) -> String {
        format!("{SERVER}/v1/tt/boards/c/t{i}/layout.svg")
    }

    fn svg() -> Art {
        decode(b"<svg xmlns=\"x\"></svg>".to_vec()).unwrap()
    }

    fn missed(images: &mut Images, urls: &[String], now: Instant) {
        let plan = images.plan(urls.to_vec(), now);
        assert!(plan.jobs.iter().all(|j| matches!(j, Job::Disk(_))));
        for url in urls {
            images.finish(url.clone(), Done::Miss, now);
        }
    }

    fn fetches(plan: &Plan) -> Vec<&str> {
        plan.jobs
            .iter()
            .filter_map(|j| match j {
                Job::Fetch(u) => Some(u.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_cache_name_is_stable_and_distinct() {
        assert_eq!(cache_name("https://a/x.png"), cache_name("https://a/x.png"));
        assert_ne!(cache_name("https://a/x.png"), cache_name("https://a/y.png"));
        assert_eq!(cache_name("a").len(), 16);
    }

    #[test]
    fn decoding_recognises_svg_and_rejects_noise() {
        assert!(matches!(
            decode(b"<svg xmlns=\"x\"></svg>".to_vec()),
            Some(Art::Svg(_))
        ));
        assert!(matches!(
            decode(b"\x89PNG\r\n\x1a\n....".to_vec()),
            Some(Art::Raster(_))
        ));
        assert!(decode(b"hello".to_vec()).is_none());
    }

    #[test]
    fn only_urls_on_the_boards_server_are_throttled() {
        let images = images();
        assert!(images.throttled(&layout(1)));
        assert!(images.throttled(&format!("{SERVER}/v1/addons/a/icon")));
        assert!(!images.throttled("https://cdn.discordapp.com/avatars/1/a.png?size=128"));
        assert!(!images.throttled("https://api.example.evil/v1/x.svg"));
        assert!(!images.throttled("mock:a.svg"));
    }

    #[test]
    fn a_url_is_only_requested_once() {
        let mut images = images();
        let now = Instant::now();
        let a = "https://cdn.discordapp.com/a.png".to_owned();
        assert_eq!(
            images.prefetch([a.clone(), a.clone()], now),
            [Job::Load(a.clone())]
        );
        assert!(images.prefetch([a.clone()], now).is_empty());
        images.finish(a.clone(), Done::Art(svg()), now);
        assert!(images.get(&a).is_some());
        assert!(images.prefetch([a], now).is_empty());
    }

    #[test]
    fn prefetching_skips_throttled_urls() {
        let mut images = images();
        assert!(images.prefetch([layout(1)], Instant::now()).is_empty());
        assert!(images.wants(&layout(1), Instant::now()));
    }

    #[test]
    fn only_visible_layouts_are_fetched() {
        let mut images = images();
        let now = Instant::now();
        let all: Vec<String> = (0..505).map(layout).collect();
        missed(&mut images, &all[..3], now);
        let plan = images.plan(all[..3].to_vec(), now);
        assert_eq!(
            fetches(&plan),
            [all[0].as_str(), all[1].as_str(), all[2].as_str()]
        );
        assert!(images.plan(Vec::<String>::new(), now).jobs.is_empty());
        assert!(all[3..].iter().all(|u| images.wants(u, now)));
    }

    #[test]
    fn a_disk_copy_is_read_before_any_request() {
        let mut images = images();
        let now = Instant::now();
        assert_eq!(images.plan([layout(1)], now).jobs, [Job::Disk(layout(1))]);
        assert!(images.plan([layout(1)], now).jobs.is_empty());
        images.finish(layout(1), Done::Art(svg()), now);
        assert!(images.get(&layout(1)).is_some());
        assert!(images.plan([layout(1)], now).jobs.is_empty());
    }

    #[test]
    fn no_more_than_four_requests_are_in_flight() {
        let mut images = images();
        let now = Instant::now();
        let urls: Vec<String> = (0..10).map(layout).collect();
        missed(&mut images, &urls, now);
        let plan = images.plan(urls.clone(), now);
        assert_eq!(fetches(&plan).len(), IN_FLIGHT);
        assert_eq!(plan.wake, None);
        assert!(fetches(&images.plan(urls.clone(), now)).is_empty());
        images.finish(urls[0].clone(), Done::Art(svg()), now);
        assert_eq!(fetches(&images.plan(urls, now)), [layout(4).as_str()]);
    }

    #[test]
    fn no_more_than_twenty_requests_start_in_a_minute() {
        let mut images = images();
        let start = Instant::now();
        let urls: Vec<String> = (0..30).map(layout).collect();
        missed(&mut images, &urls, start);
        let mut started = Vec::new();
        for step in 0..10 {
            let now = start + Duration::from_secs(step);
            let plan = images.plan(urls.clone(), now);
            for url in fetches(&plan) {
                started.push(url.to_owned());
            }
            for url in fetches(&plan).iter().map(|u| u.to_string()).collect::<Vec<_>>() {
                images.finish(url, Done::Art(svg()), now);
            }
        }
        assert_eq!(started.len(), PER_MINUTE);
        let now = start + Duration::from_secs(10);
        let plan = images.plan(urls.clone(), now);
        assert!(fetches(&plan).is_empty());
        assert_eq!(plan.wake, Some(Duration::from_secs(50)));
        let later = start + MINUTE;
        assert_eq!(fetches(&images.plan(urls, later)).len(), IN_FLIGHT);
    }

    #[test]
    fn a_429_pauses_every_request_for_its_retry_after() {
        let mut images = images();
        let now = Instant::now();
        let urls: Vec<String> = (0..6).map(layout).collect();
        missed(&mut images, &urls, now);
        let plan = images.plan(urls.clone(), now);
        assert_eq!(fetches(&plan).len(), IN_FLIGHT);
        images.finish(urls[0].clone(), Done::Refused(Duration::from_secs(30)), now);
        assert!(images.get(&urls[0]).is_none());
        let soon = now + Duration::from_secs(10);
        let plan = images.plan(urls.clone(), soon);
        assert!(plan.jobs.is_empty());
        assert_eq!(plan.wake, Some(Duration::from_secs(20)));
        assert!(!images.wants(&urls[0], soon));
        let after = now + Duration::from_secs(30);
        assert!(images.wants(&urls[0], after));
        let plan = images.plan(urls.clone(), after);
        assert_eq!(fetches(&plan), [urls[0].as_str()]);
    }

    #[test]
    fn a_failure_is_remembered_in_memory_for_a_while() {
        let mut images = images();
        let now = Instant::now();
        missed(&mut images, &[layout(1)], now);
        images.plan([layout(1)], now);
        images.finish(layout(1), Done::Failed, now);
        assert!(
            images
                .plan([layout(1)], now + Duration::from_secs(60))
                .jobs
                .is_empty()
        );
        let plan = images.plan([layout(1)], now + FAILED_FOR);
        assert_eq!(plan.jobs, [Job::Fetch(layout(1))]);
    }

    #[test]
    fn a_free_url_ignores_the_throttle() {
        let mut images = images();
        let now = Instant::now();
        images.paused = Some(now + MINUTE);
        let avatar = "https://cdn.discordapp.com/avatars/1/a.png".to_owned();
        assert_eq!(
            images.plan([avatar.clone()], now).jobs,
            [Job::Load(avatar.clone())]
        );
        images.finish(avatar.clone(), Done::Refused(MINUTE), now);
        assert_eq!(images.paused, Some(now + MINUTE));
    }

    #[test]
    fn retry_after_defaults_to_a_minute_and_is_capped() {
        assert_eq!(retry_after(Some("12")), Duration::from_secs(12));
        assert_eq!(retry_after(Some(" 0 ")), Duration::from_secs(1));
        assert_eq!(retry_after(None), RETRY_AFTER);
        assert_eq!(retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), RETRY_AFTER);
        assert_eq!(retry_after(Some("999999")), RETRY_AFTER_MAX);
    }

    fn serve(status: &str, headers: &str, body: &[u8]) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/v1/tt/boards/c/t/layout.svg",
            listener.local_addr().unwrap()
        );
        let reply = format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect::<Vec<u8>>();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            stream.write_all(&reply).unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, handle)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-tt-images-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn a_refused_request_is_not_cached_on_disk() {
        let dir = scratch("refused");
        let (url, server) = serve("429 Too Many Requests", "Retry-After: 7\r\n", b"{}");
        let done = fetch(&dir, &url, Some("tok"));
        let request = server.join().unwrap();
        assert!(matches!(done, Done::Refused(d) if d == Duration::from_secs(7)));
        assert!(request.to_ascii_lowercase().contains("authorization: bearer tok"));
        assert!(!dir.join(cache_name(&url)).exists());
        assert!(cached(&dir, &url).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_request_is_not_cached_on_disk() {
        let dir = scratch("failed");
        let (url, server) = serve("500 Internal Server Error", "", b"<svg></svg>");
        assert!(matches!(fetch(&dir, &url, None), Done::Failed));
        server.join().unwrap();
        assert!(!dir.join(cache_name(&url)).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_fetched_layout_is_cached_on_disk() {
        let dir = scratch("ok");
        let (url, server) = serve(
            "200 OK",
            "Content-Type: image/svg+xml\r\n",
            b"<svg xmlns=\"x\"></svg>",
        );
        assert!(matches!(fetch(&dir, &url, None), Done::Art(Art::Svg(_))));
        let request = server.join().unwrap();
        assert!(!request.to_ascii_lowercase().contains("authorization"));
        assert!(matches!(cached(&dir, &url), Some(Art::Svg(_))));
        assert!(matches!(Job::Disk(url.clone()).run(&dir, None), Done::Art(_)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
