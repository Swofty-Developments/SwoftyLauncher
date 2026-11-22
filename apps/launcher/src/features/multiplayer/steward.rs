//! League Stewarding's page for the host. While the host's game runs the addon, it names its page in
//! `steward.json` in its storage folder, and the room page offers to open it in the browser. The page's url
//! carries the page's token, so it is never logged.

use std::path::Path;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Deserialize;
use swofty_core::platform::process::GameProcess;
use swofty_core::session::SessionRecord;
use swofty_core::store::Store;

use super::state::due;

/// Where the addon names its page, under the game folder.
pub(super) const PAGE_FILE: &str = "swofty/addons/swofty.league-stewarding/steward.json";
/// How often the room page reads the file while it can offer the page.
pub(super) const EVERY: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
struct PageFile {
    schema: u32,
    url: String,
    started: u64,
}

/// The page's url from the text of `steward.json`: a schema 1 file whose page started with the running game or
/// after it (`game_started`, in Unix seconds), on this machine only.
pub(crate) fn page_url(text: &str, game_started: f64) -> Option<String> {
    let file: PageFile = serde_json::from_str(text).ok()?;
    let current = file.started as f64 >= game_started.floor();
    (file.schema == 1 && current && local_url(&file.url)).then_some(file.url)
}

/// Whether `url` is a plain http page on this machine: `127.0.0.1` or `localhost`, with or without a port.
pub(crate) fn local_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return false,
        None => authority,
    };
    host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost")
}

/// Whether the room page offers the page: to the host of a room that is on, while its game runs.
pub(crate) fn offered(in_room: bool, host: bool, over: bool, running: bool) -> bool {
    in_room && host && !over && running
}

/// When the game in `dir` started, in Unix seconds, as its process says; Windows doesn't say.
pub(super) fn process_started(processes: &[GameProcess], dir: &Path) -> Option<f64> {
    processes
        .iter()
        .filter(|g| g.is_in(dir))
        .filter_map(|g| g.started?.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .max_by(f64::total_cmp)
}

/// Reads the page the game in `dir` names; `None` without a current one. The game started at the later of the
/// newest session's start and `process_started`.
pub(super) fn read(store: &Store, dir: &Path, process_started: Option<f64>) -> Option<String> {
    let session = store
        .latest_session()
        .and_then(|d| SessionRecord::read(&d).started);
    let started = session
        .into_iter()
        .chain(process_started)
        .max_by(f64::total_cmp)?;
    let text = std::fs::read_to_string(dir.join(PAGE_FILE)).ok()?;
    page_url(&text, started)
}

/// The page the room page offers, read every [`EVERY`] while it can be offered.
#[derive(Default)]
pub(crate) struct Steward {
    pub(super) url: Option<String>,
    pub(super) at: Option<Instant>,
    pub(super) reading: bool,
}

impl Steward {
    pub(super) fn due(&self, now: Instant) -> bool {
        !self.reading && due(self.at, EVERY, now)
    }

    /// Drops the page once it can't be offered. True when one was shown.
    pub(super) fn forget(&mut self) -> bool {
        self.at = None;
        self.url.take().is_some()
    }

    /// Takes a read; `offered` is whether the page can still be offered. True when the button changed.
    pub(super) fn took(&mut self, url: Option<String>, offered: bool, now: Instant) -> bool {
        self.reading = false;
        self.at = Some(now);
        let url = url.filter(|_| offered);
        if self.url == url {
            return false;
        }
        self.url = url;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Steward, local_url, offered, page_url};

    const URL: &str = "http://127.0.0.1:51234/?t=0123456789abcdef0123456789abcdef";

    fn file(schema: u32, url: &str, started: u64) -> String {
        serde_json::json!({"schema": schema, "url": url, "port": 51234, "started": started}).to_string()
    }

    #[test]
    fn a_current_schema_one_page_on_this_machine_is_offered() {
        assert_eq!(
            page_url(&file(1, URL, 1_795_000_000), 1_795_000_000.6).as_deref(),
            Some(URL)
        );
        assert_eq!(
            page_url(&file(1, URL, 1_795_000_100), 1_795_000_000.0).as_deref(),
            Some(URL)
        );
        let more = r#"{"schema":1,"url":"http://localhost:8080/?t=ab","port":8080,"started":5,"name":"x"}"#;
        assert_eq!(
            page_url(more, 5.0).as_deref(),
            Some("http://localhost:8080/?t=ab")
        );
    }

    #[test]
    fn a_page_from_before_this_game_another_schema_or_a_broken_file_is_not() {
        assert_eq!(
            page_url(&file(1, URL, 1_794_999_999), 1_795_000_000.0),
            None,
            "the last game's page"
        );
        assert_eq!(page_url(&file(2, URL, 1_795_000_000), 1_795_000_000.0), None);
        assert_eq!(page_url(r#"{"schema":1,"url":"http://127.0.0.1:1/"}"#, 0.0), None);
        assert_eq!(page_url("{\"schema\":1,", 0.0), None);
        assert_eq!(page_url("", 0.0), None);
    }

    #[test]
    fn only_a_plain_http_page_on_this_machine_is_local() {
        for url in [
            URL,
            "http://127.0.0.1/",
            "http://127.0.0.1",
            "http://localhost:9000/?t=1",
            "http://LOCALHOST:9000",
        ] {
            assert!(local_url(url), "{url}");
        }
        for url in [
            "https://127.0.0.1:51234/",
            "http://127.0.0.1.example.com/",
            "http://127.0.0.1@example.com/",
            "http://localhost:80@example.com/",
            "http://example.com/?u=http://127.0.0.1/",
            "http://192.168.1.2:51234/",
            "http://[::1]:51234/",
            "http://127.0.0.1:port/",
            "file:///C:/steward.html",
            "javascript:alert(1)",
        ] {
            assert!(!local_url(url), "{url}");
        }
        assert_eq!(page_url(&file(1, "http://example.com/?t=1", 5), 5.0), None);
    }

    #[test]
    fn only_the_host_of_a_room_that_is_on_with_its_game_running_is_offered_the_page() {
        assert!(offered(true, true, false, true));
        assert!(!offered(false, true, false, true), "no room");
        assert!(!offered(true, false, false, true), "a player who joined");
        assert!(!offered(true, true, true, true), "the room is over");
        assert!(!offered(true, true, false, false), "the game isn't running");
    }

    #[test]
    fn the_button_follows_each_read_and_goes_once_the_page_cant_be_offered() {
        let now = Instant::now();
        let mut steward = Steward::default();
        assert!(steward.due(now));
        steward.reading = true;
        assert!(!steward.due(now), "a read is under way");
        assert!(steward.took(Some(URL.into()), true, now));
        assert!(!steward.due(now + Duration::from_secs(1)));
        assert!(steward.due(now + super::EVERY));
        assert!(!steward.took(Some(URL.into()), true, now), "the same page");
        assert!(
            steward.took(Some(URL.into()), false, now),
            "a read that ends after the game did"
        );
        assert_eq!(steward.url, None);
        steward.url = Some(URL.into());
        assert!(steward.forget());
        assert!(!steward.forget());
        assert!(steward.due(now));
    }
}
