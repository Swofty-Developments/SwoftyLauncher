//! Server-timed laps. At each timing line of a Time Trial lap (start, sector 1, sector 2, finish) the core
//! writes a mark into the mailbox's [`DIR`] folder. [`Pump`] sends each mark to the server as a tick on a thread
//! of its own, in the order they were made, and keeps the receipts the server answers per lap in
//! `timetrial/ticks/`. A lap collected into the outbox takes its receipts with it ([`for_lap`]), so a queued
//! upload carries them however late it goes. A mark that can't be sent while it is fresh is dropped: that lap is
//! untimed and uploads as before.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::queue::{read_json, write_json};
use crate::online::server::api::{Api, ApiError};

pub const DIR: &str = "ticks";
const STORE_DIR: &str = "timetrial/ticks";
const POLL_EVERY: Duration = Duration::from_millis(100);
/// How long a mark is tried for. The server times a lap only by ticks sent within a second of their mark; a later
/// one still proves the lap was driven in the session.
const FRESH_MS: u64 = 30_000;
const RETRY_MS: [u64; 4] = [250, 1_000, 2_000, 4_000];
/// The server's own limit is 12 ticks a minute per player.
const PER_MINUTE: usize = 12;
const MINUTE_MS: u64 = 60_000;
const STORE_KEEP_MS: u64 = 7 * 24 * 3_600_000;
const MARKS: [&str; 4] = ["start", "s1", "s2", "end"];
const MARK_MAX_BYTES: u64 = 4096;
const RECEIPT_MAX: usize = 1024;
const SENDS_PER_STEP: usize = 4;

/// What the pump needs of the server, so its logic can run against a fake.
pub trait Ticks {
    fn tick(&self, nonce: &str, body: &Value) -> Result<String, ApiError>;
}

impl Ticks for Api {
    fn tick(&self, nonce: &str, body: &Value) -> Result<String, ApiError> {
        Api::tick(self, nonce, body)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Mark {
    #[serde(default)]
    seq: u64,
    lap: u32,
    mark: String,
    game_ms: u64,
    at_ms: u64,
}

/// The receipts of one session, by the session's lap counter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    #[serde(default)]
    pub laps: BTreeMap<u32, Lap>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lap {
    #[serde(default)]
    pub receipts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

impl Store {
    fn path(data: &Path, nonce: &str) -> PathBuf {
        let digest = Sha256::digest(nonce.as_bytes());
        let name: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
        data.join(STORE_DIR).join(format!("{name}.json"))
    }

    fn read(data: &Path, nonce: &str) -> Option<Self> {
        read_json::<Self>(&Self::path(data, nonce)).filter(|s| s.nonce == nonce)
    }

    fn write(&self, data: &Path) -> std::io::Result<()> {
        let path = Self::path(data, &self.nonce);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_json(&path, self)
    }
}

/// The receipts the upload of a lap carries as `meta.timing`, in mark order: the lap of session `nonce` with the
/// session lap counter `lap_index`, or, from a lap file without one, the lap whose finish was at `time_ms`. Null when
/// the lap has none.
pub fn for_lap(data: &Path, nonce: Option<&str>, lap_index: Option<u32>, time_ms: u64) -> Value {
    let Some(store) = nonce.and_then(|n| Store::read(data, n)) else {
        return Value::Null;
    };
    let lap = match lap_index {
        Some(i) => store.laps.get(&i),
        None => store.laps.values().find(|l| l.end_ms == Some(time_ms)),
    };
    let receipts: Vec<Value> = lap
        .map(|l| {
            MARKS
                .iter()
                .filter_map(|m| l.receipts.get(*m))
                .map(|r| r.as_str().into())
                .collect()
        })
        .unwrap_or_default();
    if receipts.is_empty() {
        Value::Null
    } else {
        Value::Array(receipts)
    }
}

/// The folders the core writes marks into, one in each mailbox.
pub fn folders(data: &Path, game: &Path) -> Vec<PathBuf> {
    super::bridge::mailboxes(data, game)
        .iter()
        .map(|m| m.join(DIR))
        .collect()
}

/// Whether a mark is still waiting to be sent, so a lap just written waits a moment for its finish receipt.
pub fn pending(data: &Path, game: &Path) -> bool {
    folders(data, game).iter().any(|dir| !mark_files(dir).is_empty())
}

fn mark_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && p.is_file())
        .collect()
}

fn read_mark(path: &Path) -> Option<Mark> {
    if std::fs::metadata(path).ok()?.len() > MARK_MAX_BYTES {
        return None;
    }
    let mark: Mark = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    MARKS.contains(&mark.mark.as_str()).then_some(mark)
}

fn plausible(receipt: &str) -> bool {
    receipt.starts_with("tk1.")
        && receipt.len() <= RECEIPT_MAX
        && receipt
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.=".contains(&b))
}

fn retryable(e: &ApiError) -> bool {
    matches!(e.status, 0 | 408 | 429) || e.status >= 500
}

/// The pump's state between polls: the mark being tried, its attempts, the ticks sent in the last minute and the
/// session's receipts.
pub(crate) struct Sender {
    data: PathBuf,
    folders: Vec<PathBuf>,
    tries: BTreeMap<PathBuf, (u32, u64)>,
    sent: VecDeque<u64>,
    store: Option<Store>,
}

impl Sender {
    pub(crate) fn new(data: &Path, folders: Vec<PathBuf>) -> Self {
        Self {
            data: data.to_owned(),
            folders,
            tries: BTreeMap::new(),
            sent: VecDeque::new(),
            store: None,
        }
    }

    /// Empty the mark folders of an earlier session's leftovers and forget receipt stores past a week.
    pub(crate) fn clean(&self, now_ms: u64) {
        for dir in &self.folders {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                if entry.path().is_file() {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
        let old = |p: &Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|t| now_ms.saturating_sub(t.as_millis() as u64) > STORE_KEEP_MS)
        };
        for entry in std::fs::read_dir(self.data.join(STORE_DIR))
            .into_iter()
            .flatten()
            .flatten()
        {
            if old(&entry.path()) {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }

    fn marks(&self) -> Vec<(PathBuf, Option<Mark>)> {
        let mut all: Vec<(PathBuf, Option<Mark>)> = self
            .folders
            .iter()
            .flat_map(|d| mark_files(d))
            .map(|p| {
                let m = read_mark(&p);
                (p, m)
            })
            .collect();
        all.sort_by_key(|(p, m)| (m.as_ref().map_or((0, 0), |m| (m.at_ms, m.seq)), p.clone()));
        all
    }

    fn drop_mark(&mut self, path: &Path) {
        std::fs::remove_file(path).ok();
        self.tries.remove(path);
    }

    /// One poll: send the marks that are due, oldest first, while the session `nonce` is known. A mark waits
    /// behind the one before it, so the receipts chain in the order the marks were made.
    pub(crate) fn step(&mut self, server: &dyn Ticks, nonce: Option<&str>, now_ms: u64, say: &dyn Fn(&str)) {
        while self
            .sent
            .front()
            .is_some_and(|&at| now_ms.saturating_sub(at) >= MINUTE_MS)
        {
            self.sent.pop_front();
        }
        let mut sends = 0;
        for (path, mark) in self.marks() {
            let Some(mark) = mark else {
                self.drop_mark(&path);
                continue;
            };
            let age = now_ms.saturating_sub(mark.at_ms);
            if age > FRESH_MS {
                say(&format!(
                    "timing: lap {} {} not sent within {} s; that lap is untimed",
                    mark.lap,
                    mark.mark,
                    FRESH_MS / 1000
                ));
                self.drop_mark(&path);
                continue;
            }
            let Some(nonce) = nonce else {
                return;
            };
            let (attempt, next) = self.tries.get(&path).copied().unwrap_or((1, 0));
            if now_ms < next || self.sent.len() >= PER_MINUTE || sends >= SENDS_PER_STEP {
                return;
            }
            let mut store = match self.store.take() {
                Some(s) if s.nonce == nonce => s,
                _ => Store::read(&self.data, nonce).unwrap_or_else(|| Store {
                    nonce: nonce.to_owned(),
                    ..Store::default()
                }),
            };
            let mut body = json!({
                "lap": mark.lap,
                "mark": mark.mark,
                "game_ms": mark.game_ms,
                "since_ms": age,
                "attempt": attempt,
            });
            if let Some(prev) = &store.last {
                body["prev"] = prev.as_str().into();
            }
            self.sent.push_back(now_ms);
            sends += 1;
            let answer = server.tick(nonce, &body);
            let wait = match answer {
                Ok(receipt) if plausible(&receipt) => {
                    let lap = store.laps.entry(mark.lap).or_default();
                    lap.receipts.insert(mark.mark.clone(), receipt.clone());
                    if mark.mark == "end" {
                        lap.end_ms = Some(mark.game_ms);
                    }
                    store.last = Some(receipt);
                    if let Err(e) = store.write(&self.data) {
                        say(&format!("timing: the receipts could not be kept: {e}"));
                    }
                    None
                }
                Ok(_) => {
                    say(&format!(
                        "timing: lap {} {}: the server's receipt is not one",
                        mark.lap, mark.mark
                    ));
                    None
                }
                Err(e) if retryable(&e) => {
                    if attempt == 1 {
                        say(&format!(
                            "timing: lap {} {} not sent ({e}); trying again",
                            mark.lap, mark.mark
                        ));
                    }
                    Some(RETRY_MS[(attempt as usize - 1).min(RETRY_MS.len() - 1)])
                }
                Err(e) => {
                    say(&format!(
                        "timing: lap {} {} refused ({e}); that lap is untimed",
                        mark.lap, mark.mark
                    ));
                    None
                }
            };
            self.store = Some(store);
            match wait {
                Some(wait) => {
                    self.tries.insert(path, (attempt + 1, now_ms + wait));
                    return;
                }
                None => self.drop_mark(&path),
            }
        }
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Sends the marks of the session the game is running while it runs, reading the session nonce from the session
/// file the launcher wrote, which the nonce retry may fill in late.
pub struct Pump {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Pump {
    pub fn start(
        data: &Path,
        game: &Path,
        server: impl Ticks + Send + 'static,
        say: impl Fn(&str) + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let mut sender = Sender::new(data, folders(data, game));
        sender.clean(unix_ms());
        let session = game.join(super::SESSION_FILE);
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("tt-ticks".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    let nonce = read_json::<Value>(&session)
                        .and_then(|d| d.get("tt_nonce")?.as_str().map(str::to_owned));
                    sender.step(&server, nonce.as_deref(), unix_ms(), &say);
                    std::thread::sleep(POLL_EVERY);
                }
            })
            .ok();
        Self { stop, thread }
    }

    pub fn finish(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct Fake {
        replies: RefCell<VecDeque<Result<String, ApiError>>>,
        seen: RefCell<Vec<(String, Value)>>,
    }

    impl Fake {
        fn new(replies: Vec<Result<String, ApiError>>) -> Self {
            Self {
                replies: RefCell::new(replies.into()),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Ticks for Fake {
        fn tick(&self, nonce: &str, body: &Value) -> Result<String, ApiError> {
            self.seen.borrow_mut().push((nonce.to_owned(), body.clone()));
            self.replies
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Err(down()))
        }
    }

    fn down() -> ApiError {
        ApiError {
            status: 0,
            message: "couldn't reach the server".into(),
            retry_after: None,
        }
    }

    fn temp(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("swofty-ticks-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (data, game) = (root.join("data"), root.join("game"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        (data, game)
    }

    fn mark(data: &Path, seq: u64, lap: u32, which: &str, game_ms: u64, at_ms: u64) {
        let dir = data.join("tt").join(DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let body = json!({"seq": seq, "lap": lap, "mark": which, "game_ms": game_ms, "at_ms": at_ms});
        std::fs::write(dir.join(format!("{at_ms}-{seq}.json")), body.to_string()).unwrap();
    }

    fn lap_marks(data: &Path, lap: u32, at: u64) {
        let seq = u64::from(lap) * 4;
        mark(data, seq + 1, lap, "start", 0, at);
        mark(data, seq + 2, lap, "s1", 28_411, at + 1);
        mark(data, seq + 3, lap, "s2", 59_000, at + 2);
        mark(data, seq + 4, lap, "end", 91_234, at + 3);
    }

    fn receipt(n: u32) -> String {
        format!("tk1.cGF5bG9hZA{n}.bWFj")
    }

    #[test]
    fn the_marks_of_a_lap_go_out_in_order_chained_and_its_receipts_are_kept_for_the_upload() {
        let (data, game) = temp("order");
        let fake = Fake::new((1..=4).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        for (seq, (which, game_ms, at)) in [
            ("start", 0, 1_000_000),
            ("s1", 28_411, 1_028_400),
            ("s2", 59_000, 1_059_000),
            ("end", 91_234, 1_091_230),
        ]
        .into_iter()
        .enumerate()
        {
            mark(&data, seq as u64 + 1, 3, which, game_ms, at);
            sender.step(&fake, Some("nonce-a"), at + 12, &|_| {});
        }
        let seen = fake.seen.borrow();
        assert_eq!(seen.len(), 4);
        assert!(seen.iter().all(|(n, _)| n == "nonce-a"));
        let marks: Vec<&str> = seen.iter().map(|(_, b)| b["mark"].as_str().unwrap()).collect();
        assert_eq!(marks, ["start", "s1", "s2", "end"]);
        assert_eq!(
            seen[1].1,
            json!({"lap": 3, "mark": "s1", "game_ms": 28_411, "since_ms": 12, "attempt": 1, "prev": receipt(1)})
        );
        assert!(seen[0].1.get("prev").is_none());
        assert_eq!(seen[3].1["prev"], receipt(3).as_str());
        assert!(!pending(&data, &game));

        let all: Vec<Value> = (1..=4).map(|n| receipt(n).into()).collect();
        assert_eq!(
            for_lap(&data, Some("nonce-a"), Some(3), 91_234),
            Value::Array(all.clone())
        );
        assert_eq!(for_lap(&data, Some("nonce-a"), None, 91_234), Value::Array(all));
        assert_eq!(for_lap(&data, Some("nonce-a"), None, 91_235), Value::Null);
        assert_eq!(for_lap(&data, Some("nonce-a"), Some(4), 91_234), Value::Null);
        assert_eq!(for_lap(&data, Some("nonce-b"), Some(3), 91_234), Value::Null);
        assert_eq!(for_lap(&data, None, Some(3), 91_234), Value::Null);
    }

    #[test]
    fn a_lap_driven_offline_drops_its_marks_once_they_are_stale_and_is_untimed() {
        let (data, game) = temp("offline");
        lap_marks(&data, 1, 1_000_000);
        let fake = Fake::new(Vec::new());
        let mut sender = Sender::new(&data, folders(&data, &game));
        let lines = RefCell::new(Vec::new());
        let say = |l: &str| lines.borrow_mut().push(l.to_owned());
        let mut now = 1_000_010;
        while pending(&data, &game) && now < 1_200_000 {
            sender.step(&fake, Some("n"), now, &say);
            now += 100;
        }
        assert!(!pending(&data, &game));
        let seen = fake.seen.borrow();
        let firsts: Vec<u64> = seen
            .iter()
            .take_while(|(_, b)| b["mark"] == "start")
            .map(|(_, b)| b["attempt"].as_u64().unwrap())
            .collect();
        assert_eq!(
            &firsts[..4],
            [1, 2, 3, 4],
            "a mark is tried again with its attempt counted"
        );
        assert!(seen.len() <= PER_MINUTE, "the retries keep to the limit");
        assert_eq!(for_lap(&data, Some("n"), Some(1), 91_234), Value::Null);
        let lines = lines.borrow();
        assert_eq!(
            lines[0],
            "timing: lap 1 start not sent (couldn't reach the server); trying again"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "timing: lap 1 end not sent within 30 s; that lap is untimed")
        );
    }

    #[test]
    fn a_retried_tick_says_its_attempt_and_how_stale_it_is() {
        let (data, game) = temp("retry");
        mark(&data, 1, 2, "start", 0, 5_000);
        let busy = ApiError {
            status: 503,
            message: "busy".into(),
            retry_after: None,
        };
        let fake = Fake::new(vec![Err(busy), Ok(receipt(1))]);
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 5_020, &|_| {});
        sender.step(&fake, Some("n"), 5_100, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 1, "the retry waits its pause");
        sender.step(&fake, Some("n"), 5_300, &|_| {});
        let seen = fake.seen.borrow();
        assert_eq!(
            (seen[1].1["attempt"].as_u64(), seen[1].1["since_ms"].as_u64()),
            (Some(2), Some(300))
        );
        assert!(!pending(&data, &game));
    }

    #[test]
    fn a_server_without_ticks_refuses_them_and_the_marks_go_at_once() {
        let (data, game) = temp("old-server");
        lap_marks(&data, 1, 92_000);
        let gone = || ApiError {
            status: 404,
            message: "not found".into(),
            retry_after: None,
        };
        let fake = Fake::new((0..4).map(|_| Err(gone())).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 92_010, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 4);
        assert!(!pending(&data, &game));
        assert_eq!(for_lap(&data, Some("n"), Some(1), 91_234), Value::Null);
    }

    #[test]
    fn marks_wait_for_the_session_nonce_and_never_pass_twelve_ticks_a_minute() {
        let (data, game) = temp("throttle");
        for lap in 0..4 {
            lap_marks(&data, lap, 90_000 + u64::from(lap) * 10);
        }
        let fake = Fake::new((1..=16).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, None, 92_000, &|_| {});
        assert!(fake.seen.borrow().is_empty() && pending(&data, &game));
        let mut now = 92_000;
        while pending(&data, &game) && now < 200_000 {
            sender.step(&fake, Some("n"), now, &|_| {});
            now += 100;
        }
        assert_eq!(fake.seen.borrow().len(), PER_MINUTE);
        assert_eq!(
            for_lap(&data, Some("n"), Some(2), 91_234)
                .as_array()
                .map(Vec::len),
            Some(4)
        );
        assert_eq!(for_lap(&data, Some("n"), Some(3), 91_234), Value::Null);
    }

    #[test]
    fn junk_in_the_mark_folder_and_odd_receipts_are_dropped() {
        let (data, game) = temp("junk");
        let dir = data.join("tt").join(DIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1-1.json"), b"not json").unwrap();
        std::fs::write(
            dir.join("2-2.json"),
            json!({"lap": 1, "mark": "s9", "game_ms": 1, "at_ms": 2}).to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("3-3.json"), vec![b' '; 5000]).unwrap();
        std::fs::write(dir.join("4-4.json.tmp"), b"{}").unwrap();
        mark(&data, 5, 1, "start", 0, 10);
        mark(&data, 6, 1, "s1", 100, 20);
        let fake = Fake::new(vec![
            Ok("<script>".into()),
            Ok(format!("tk1.{}", "a".repeat(2000))),
        ]);
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 30, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 2);
        assert!(!pending(&data, &game));
        assert_eq!(for_lap(&data, Some("n"), Some(1), 0), Value::Null);
        sender.clean(30);
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
    }

    #[test]
    fn a_tick_is_posted_with_the_sign_in_and_the_receipt_read_back() {
        let (url, t) =
            crate::online::server::api::tests::serve("200 OK", r#"{"receipt":"tk1.eyJsIjozfQ.bWFj"}"#);
        let body = json!({"lap": 3, "mark": "s1", "game_ms": 28411, "since_ms": 12, "attempt": 1});
        let got = Api::with_server(&url, "tok").tick("abc", &body).unwrap();
        assert_eq!(got, "tk1.eyJsIjozfQ.bWFj");
        let request = String::from_utf8(t.join().unwrap()).unwrap();
        assert!(
            request.starts_with("POST /v1/tt/sessions/abc/ticks "),
            "{request}"
        );
        assert!(request.to_ascii_lowercase().contains("authorization: bearer tok"));
        let sent: Value = serde_json::from_str(&request[request.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(sent, body);
    }
}
