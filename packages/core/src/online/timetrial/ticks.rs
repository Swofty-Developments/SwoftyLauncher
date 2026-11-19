//! Server-timed laps. At each timing line of a Time Trial lap (start, sector 1, sector 2, finish) the core
//! writes a mark into the mailbox's [`DIR`] folder. [`Pump`] sends each mark to the server as a tick on a thread
//! of its own, in the order they were made, each chained to the receipt of its lap's previous mark, and keeps the
//! receipts per lap in `timetrial/ticks/`. A lap collected into the outbox takes its receipts with it
//! ([`for_lap`]), so a queued upload carries them however late it goes. A mark that can't be sent in time is
//! dropped with the rest of its lap's timing: that lap is untimed and uploads as before. The marks of a pause in a
//! lap (`p1` as the game pauses, chained to the lap's start, and `r1` as it resumes, chained to `p1`; up to four)
//! let the server leave the pause out of the lap's real time; one that can't be sent is dropped alone, and the pause
//! stays in.

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
/// How long a mark waits to be sent while the server can't be reached.
const GIVE_UP_MS: u64 = 600_000;
/// How long after its lap's finish a mark the server answered with an error is tried again.
const AFTER_END_MS: u64 = 60_000;
/// The pauses before each try again; the last repeats. Ten minutes of them stay within the server's 50 attempts.
const RETRY_MS: [u64; 7] = [500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000];
const MAX_ATTEMPT: u32 = 50;
/// The server's own limit is 12 ticks a minute per player.
const PER_MINUTE: usize = 12;
const MINUTE_MS: u64 = 60_000;
const STORE_KEEP_MS: u64 = 7 * 24 * 3_600_000;
const MARKS: [&str; 4] = ["start", "s1", "s2", "end"];
const PAUSE_MARKS: [&str; 8] = ["p1", "r1", "p2", "r2", "p3", "r3", "p4", "r4"];
const MAX_LAP: u32 = 100_000;
const MAX_GAME_MS: u64 = 3_600_000;
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

/// What the upload of a lap carries as `meta.timing`: the session lap counter and the receipts in mark order, for
/// the lap of session `nonce` with the counter `lap_index`, or, from a lap file without one, the lap whose finish was
/// at `time_ms`. Null when the lap has none.
pub fn for_lap(data: &Path, nonce: Option<&str>, lap_index: Option<u32>, time_ms: u64) -> Value {
    let Some(store) = nonce.and_then(|n| Store::read(data, n)) else {
        return Value::Null;
    };
    let found = match lap_index {
        Some(i) => store.laps.get_key_value(&i),
        None => store.laps.iter().find(|(_, l)| l.end_ms == Some(time_ms)),
    };
    let Some((lap, receipts)) = found
        .map(|(n, l)| {
            (
                *n,
                MARKS
                    .iter()
                    .filter_map(|m| l.receipts.get(*m))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, r)| !r.is_empty())
    else {
        return Value::Null;
    };
    json!({"lap": lap, "receipts": receipts})
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
    ((MARKS.contains(&mark.mark.as_str()) || pause(&mark.mark))
        && mark.lap <= MAX_LAP
        && mark.game_ms <= MAX_GAME_MS)
        .then_some(mark)
}

fn plausible(receipt: &str) -> bool {
    !receipt.is_empty()
        && receipt.len() <= RECEIPT_MAX
        && receipt
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.=".contains(&b))
}

/// Whether `mark` is one end of a pause in a lap.
fn pause(mark: &str) -> bool {
    PAUSE_MARKS.contains(&mark)
}

/// The mark whose receipt a tick of `mark` carries as `prev`: the line before it in its lap, the lap's start for a
/// pause, the pause for its resume.
fn previous(mark: &str) -> Option<&'static str> {
    if let Some(at) = PAUSE_MARKS.iter().position(|m| *m == mark) {
        return Some(if at % 2 == 0 {
            MARKS[0]
        } else {
            PAUSE_MARKS[at - 1]
        });
    }
    let at = MARKS.iter().position(|m| *m == mark)?;
    at.checked_sub(1).map(|i| MARKS[i])
}

#[derive(Debug, Clone, Copy)]
struct Try {
    attempt: u32,
    next: u64,
    answered: bool,
}

enum Outcome {
    Kept,
    Again { wait: u64, answered: bool },
    DropMark,
    DropLap,
    StopSession,
}

/// The pump's state between polls: the tries of the marks waiting, the ticks sent in the last minute, the
/// session's receipts and the session the server will not time.
pub(crate) struct Sender {
    data: PathBuf,
    folders: Vec<PathBuf>,
    tries: BTreeMap<PathBuf, Try>,
    sent: VecDeque<u64>,
    store: Option<Store>,
    stopped: Option<String>,
}

impl Sender {
    pub(crate) fn new(data: &Path, folders: Vec<PathBuf>) -> Self {
        Self {
            data: data.to_owned(),
            folders,
            tries: BTreeMap::new(),
            sent: VecDeque::new(),
            store: None,
            stopped: None,
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

    fn session(&mut self, nonce: &str) -> &mut Store {
        if self.store.as_ref().is_none_or(|s| s.nonce != nonce) {
            self.store = Some(Store::read(&self.data, nonce).unwrap_or_else(|| Store {
                nonce: nonce.to_owned(),
                ..Store::default()
            }));
        }
        self.store.as_mut().expect("store")
    }

    fn keep(&mut self, say: &dyn Fn(&str)) {
        if let Some(store) = &self.store
            && let Err(e) = store.write(&self.data)
        {
            say(&format!("timing: the receipts could not be kept: {e}"));
        }
    }

    /// One poll: send the marks that are due, oldest first, while the session `nonce` is known. A mark waits
    /// behind the one before it, and goes only with the receipt of its lap's previous mark in hand.
    pub(crate) fn step(&mut self, server: &dyn Ticks, nonce: Option<&str>, now_ms: u64, say: &dyn Fn(&str)) {
        while self
            .sent
            .front()
            .is_some_and(|&at| now_ms.saturating_sub(at) >= MINUTE_MS)
        {
            self.sent.pop_front();
        }
        let marks = self.marks();
        let mut sends = 0;
        for (path, mark) in &marks {
            let Some(mark) = mark else {
                self.drop_mark(path);
                continue;
            };
            if nonce.is_some() && nonce == self.stopped.as_deref() {
                self.drop_mark(path);
                continue;
            }
            let age = now_ms.saturating_sub(mark.at_ms);
            let tried = self.tries.get(path).copied();
            let lap_end = marks
                .iter()
                .filter_map(|(_, m)| m.as_ref())
                .find(|m| m.lap == mark.lap && m.mark == "end")
                .map(|m| m.at_ms);
            let past_end =
                tried.is_some_and(|t| t.answered) && lap_end.is_some_and(|end| now_ms > end + AFTER_END_MS);
            if age > GIVE_UP_MS || past_end || tried.is_some_and(|t| t.attempt > MAX_ATTEMPT) {
                say(&format!(
                    "timing: lap {} {} not sent within {} s; {}",
                    mark.lap,
                    mark.mark,
                    if past_end {
                        (now_ms - mark.at_ms) / 1000
                    } else {
                        GIVE_UP_MS / 1000
                    },
                    untimed(&mark.mark)
                ));
                self.drop_mark(path);
                if pause(&mark.mark) {
                    continue;
                }
                if let Some(nonce) = nonce {
                    self.session(nonce).laps.remove(&mark.lap);
                    self.keep(say);
                }
                continue;
            }
            let Some(nonce) = nonce else {
                return;
            };
            let Try { attempt, next, .. } = tried.unwrap_or(Try {
                attempt: 1,
                next: 0,
                answered: false,
            });
            if now_ms < next || self.sent.len() >= PER_MINUTE || sends >= SENDS_PER_STEP {
                return;
            }
            let store = self.session(nonce);
            let prev = previous(&mark.mark).map(|before| {
                store
                    .laps
                    .get(&mark.lap)
                    .and_then(|l| l.receipts.get(before))
                    .cloned()
            });
            if prev == Some(None) {
                say(&format!(
                    "timing: lap {} {} has no {} receipt to chain to; {}",
                    mark.lap,
                    mark.mark,
                    previous(&mark.mark).unwrap_or("?"),
                    untimed(&mark.mark)
                ));
                self.drop_mark(path);
                continue;
            }
            let mut body = json!({
                "lap": mark.lap,
                "mark": mark.mark,
                "game_ms": mark.game_ms,
                "since_ms": age,
                "attempt": attempt,
            });
            if let Some(Some(prev)) = prev {
                body["prev"] = prev.into();
            }
            self.sent.push_back(now_ms);
            sends += 1;
            let what = format!("timing: lap {} {}", mark.lap, mark.mark);
            let outcome = match server.tick(nonce, &body) {
                Ok(receipt) if plausible(&receipt) => {
                    let lap = self.session(nonce).laps.entry(mark.lap).or_default();
                    lap.receipts.insert(mark.mark.clone(), receipt);
                    if mark.mark == "end" {
                        lap.end_ms = Some(mark.game_ms);
                    }
                    Outcome::Kept
                }
                Ok(_) => {
                    say(&format!(
                        "{what}: the server's receipt is not one; {}",
                        untimed(&mark.mark)
                    ));
                    if pause(&mark.mark) {
                        Outcome::DropMark
                    } else {
                        Outcome::DropLap
                    }
                }
                Err(e) if e.status == 404 || e.status == 401 => {
                    say(&format!(
                        "{what}: the server does not time this session ({e}); its laps are untimed"
                    ));
                    Outcome::StopSession
                }
                Err(e) if matches!(e.status, 0 | 408 | 429) || e.status >= 500 => {
                    if attempt == 1 {
                        say(&format!("{what} not sent ({e}); trying again"));
                    }
                    let backoff = RETRY_MS[(attempt as usize - 1).min(RETRY_MS.len() - 1)];
                    Outcome::Again {
                        wait: e
                            .retry_after
                            .map_or(backoff, |s| s.saturating_mul(1000).max(backoff)),
                        answered: e.status != 0,
                    }
                }
                Err(e) => {
                    say(&format!("{what} refused ({e}); {}", untimed(&mark.mark)));
                    if pause(&mark.mark) {
                        Outcome::DropMark
                    } else {
                        Outcome::DropLap
                    }
                }
            };
            match outcome {
                Outcome::Kept => {
                    self.keep(say);
                    self.drop_mark(path);
                }
                Outcome::Again { wait, answered } => {
                    self.tries.insert(
                        path.clone(),
                        Try {
                            attempt: attempt + 1,
                            next: now_ms + wait,
                            answered,
                        },
                    );
                    return;
                }
                Outcome::DropMark => self.drop_mark(path),
                Outcome::DropLap => {
                    self.drop_mark(path);
                    self.session(nonce).laps.remove(&mark.lap);
                    self.keep(say);
                }
                Outcome::StopSession => {
                    self.drop_mark(path);
                    self.stopped = Some(nonce.to_owned());
                }
            }
        }
    }
}

/// What a mark that did not go out costs: its lap's timing, or for a pause only the pause's leaving out.
fn untimed(mark: &str) -> &'static str {
    if pause(mark) {
        "that pause stays in the lap's time"
    } else {
        "that lap is untimed"
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

    fn status(code: u16, retry_after: Option<u64>) -> ApiError {
        ApiError {
            status: code,
            message: format!("status {code}"),
            retry_after,
        }
    }

    fn timing(lap: u32, receipts: &[String]) -> Value {
        json!({"lap": lap, "receipts": receipts})
    }

    fn all(n: u32) -> Vec<String> {
        (1..=n).map(receipt).collect()
    }

    #[test]
    fn a_discarded_lap_leaves_a_gap_in_the_numbers_and_the_laps_around_it_keep_their_own_receipts() {
        let (data, game) = temp("gap-discarded");
        let fake = Fake::new((1..=10).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        for (seq, (lap, which, game_ms, at)) in [
            (1, "start", 0, 2_000_000),
            (1, "s1", 28_411, 2_028_400),
            (1, "s2", 59_000, 2_059_000),
            (1, "end", 91_234, 2_091_230),
            (2, "start", 0, 2_091_231),
            (2, "s1", 29_100, 2_120_330),
            (3, "start", 0, 2_300_000),
            (3, "s1", 28_200, 2_328_200),
            (3, "s2", 58_700, 2_358_700),
            (3, "end", 90_512, 2_390_512),
        ]
        .into_iter()
        .enumerate()
        {
            mark(&data, seq as u64 + 1, lap, which, game_ms, at);
            sender.step(&fake, Some("nonce-g"), at + 10, &|_| {});
        }
        let seen = fake.seen.borrow();
        let sent: Vec<(u64, &str)> = seen
            .iter()
            .map(|(_, b)| (b["lap"].as_u64().unwrap(), b["mark"].as_str().unwrap()))
            .collect();
        assert_eq!(sent.len(), 10, "{sent:?}");
        assert!(
            seen[6].1.get("prev").is_none(),
            "lap 3 starts its own chain: {}",
            seen[6].1
        );
        assert_eq!(seen[7].1["prev"], receipt(7).as_str());
        assert!(!pending(&data, &game));

        let r = |n: std::ops::RangeInclusive<u32>| n.map(receipt).collect::<Vec<_>>();
        assert_eq!(
            for_lap(&data, Some("nonce-g"), Some(1), 91_234),
            timing(1, &r(1..=4))
        );
        assert_eq!(
            for_lap(&data, Some("nonce-g"), Some(3), 90_512),
            timing(3, &r(7..=10))
        );
        assert_eq!(
            for_lap(&data, Some("nonce-g"), None, 90_512),
            timing(3, &r(7..=10)),
            "a lap file without a number finds its lap by the finish"
        );
    }

    #[test]
    fn the_marks_of_a_lap_go_out_in_order_chained_and_its_receipts_are_kept_for_the_upload() {
        let (data, game) = temp("order");
        let fake = Fake::new((1..=5).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        for (seq, (lap, which, game_ms, at)) in [
            (3, "start", 0, 1_000_000),
            (3, "s1", 28_411, 1_028_400),
            (3, "s2", 59_000, 1_059_000),
            (3, "end", 91_234, 1_091_230),
            (4, "start", 0, 1_091_231),
        ]
        .into_iter()
        .enumerate()
        {
            mark(&data, seq as u64 + 1, lap, which, game_ms, at);
            sender.step(&fake, Some("nonce-a"), at + 12, &|_| {});
        }
        let seen = fake.seen.borrow();
        assert_eq!(seen.len(), 5);
        assert!(seen.iter().all(|(n, _)| n == "nonce-a"));
        let marks: Vec<&str> = seen.iter().map(|(_, b)| b["mark"].as_str().unwrap()).collect();
        assert_eq!(marks, ["start", "s1", "s2", "end", "start"]);
        assert_eq!(
            seen[1].1,
            json!({"lap": 3, "mark": "s1", "game_ms": 28_411, "since_ms": 12, "attempt": 1, "prev": receipt(1)})
        );
        assert!(seen[0].1.get("prev").is_none());
        assert_eq!(seen[3].1["prev"], receipt(3).as_str());
        assert!(
            seen[4].1.get("prev").is_none(),
            "a lap's start chains to nothing: {}",
            seen[4].1
        );
        assert!(!pending(&data, &game));

        assert_eq!(
            for_lap(&data, Some("nonce-a"), Some(3), 91_234),
            timing(3, &all(4))
        );
        assert_eq!(for_lap(&data, Some("nonce-a"), None, 91_234), timing(3, &all(4)));
        assert_eq!(for_lap(&data, Some("nonce-a"), None, 91_235), Value::Null);
        assert_eq!(
            for_lap(&data, Some("nonce-a"), Some(4), 91_234),
            timing(4, &[receipt(5)])
        );
        assert_eq!(for_lap(&data, Some("nonce-a"), Some(5), 91_234), Value::Null);
        assert_eq!(for_lap(&data, Some("nonce-b"), Some(3), 91_234), Value::Null);
        assert_eq!(for_lap(&data, None, Some(3), 91_234), Value::Null);
    }

    #[test]
    fn a_lap_driven_offline_keeps_its_marks_for_ten_minutes_and_sends_them_when_back_online() {
        let (data, game) = temp("offline");
        lap_marks(&data, 1, 1_000_000);
        let fake = Fake::new(Vec::new());
        let mut sender = Sender::new(&data, folders(&data, &game));
        let lines = RefCell::new(Vec::new());
        let say = |l: &str| lines.borrow_mut().push(l.to_owned());
        let mut now = 1_000_010;
        let mut sent_at = Vec::new();
        while now < 1_300_000 {
            let before = fake.seen.borrow().len();
            sender.step(&fake, Some("n"), now, &say);
            if fake.seen.borrow().len() > before {
                sent_at.push(now);
            }
            now += 100;
        }
        assert!(
            pending(&data, &game),
            "five minutes offline: the marks still wait"
        );
        {
            let seen = fake.seen.borrow();
            assert!(
                seen.iter().all(|(_, b)| b["mark"] == "start"),
                "a mark waits behind the one before it"
            );
            let attempts: Vec<u64> = seen.iter().map(|(_, b)| b["attempt"].as_u64().unwrap()).collect();
            assert_eq!(&attempts[..5], [1, 2, 3, 4, 5]);
        }
        let gaps: Vec<u64> = sent_at.windows(2).map(|w| w[1] - w[0]).collect();
        assert_eq!(
            &gaps[..4],
            [500, 1_000, 2_000, 4_000],
            "backing off 0.5, 1, 2 and 4 s"
        );
        *fake.replies.borrow_mut() = (1..=4).map(|n| Ok(receipt(n))).collect();
        while pending(&data, &game) && now < 1_400_000 {
            sender.step(&fake, Some("n"), now, &say);
            now += 100;
        }
        {
            let seen = fake.seen.borrow();
            let back = &seen[seen.len() - 4..];
            assert_eq!(back[3].1["mark"], "end");
            assert!(
                back[3].1["since_ms"].as_u64().unwrap() > 200_000,
                "the real lateness is sent"
            );
        }
        assert_eq!(for_lap(&data, Some("n"), Some(1), 91_234), timing(1, &all(4)));
        assert_eq!(
            lines.borrow()[0],
            "timing: lap 1 start not sent (couldn't reach the server); trying again"
        );

        lap_marks(&data, 2, 2_000_000);
        let mut now = 2_000_010;
        while pending(&data, &game) && now < 2_700_000 {
            sender.step(&fake, Some("n"), now, &say);
            now += 1_000;
        }
        assert!(
            !pending(&data, &game) && now > 2_600_000,
            "given up past ten minutes"
        );
        assert_eq!(for_lap(&data, Some("n"), Some(2), 91_234), Value::Null);
        assert!(
            lines
                .borrow()
                .iter()
                .any(|l| l == "timing: lap 2 start not sent within 600 s; that lap is untimed")
        );
    }

    #[test]
    fn a_retried_tick_says_its_attempt_and_how_stale_it_is() {
        let (data, game) = temp("retry");
        mark(&data, 1, 2, "start", 0, 5_000);
        let fake = Fake::new(vec![Err(status(503, None)), Ok(receipt(1))]);
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 5_020, &|_| {});
        sender.step(&fake, Some("n"), 5_300, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 1, "the retry waits its pause");
        sender.step(&fake, Some("n"), 5_520, &|_| {});
        let seen = fake.seen.borrow();
        assert_eq!(
            (seen[1].1["attempt"].as_u64(), seen[1].1["since_ms"].as_u64()),
            (Some(2), Some(520))
        );
        assert!(!pending(&data, &game));
    }

    #[test]
    fn a_throttled_tick_waits_as_long_as_the_server_asks() {
        let (data, game) = temp("retry-after");
        mark(&data, 1, 2, "start", 0, 5_000);
        let fake = Fake::new(vec![Err(status(429, Some(7))), Ok(receipt(1))]);
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 5_000, &|_| {});
        sender.step(&fake, Some("n"), 11_900, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 1);
        sender.step(&fake, Some("n"), 12_000, &|_| {});
        assert_eq!(fake.seen.borrow().len(), 2);
        assert!(!pending(&data, &game));
    }

    #[test]
    fn a_server_error_is_tried_again_until_a_minute_after_the_lap_ends() {
        let (data, game) = temp("lap-end");
        lap_marks(&data, 1, 1_000);
        let fake = Fake::new((0..100).map(|_| Err(status(502, None))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        let mut now = 1_010;
        while pending(&data, &game) && now < 200_000 {
            sender.step(&fake, Some("n"), now, &|_| {});
            now += 100;
        }
        assert!(!pending(&data, &game));
        assert!((61_000..62_200).contains(&now), "{now}");
    }

    #[test]
    fn an_out_of_order_or_bad_tick_drops_that_laps_timing_and_the_next_lap_still_goes() {
        for code in [400, 409] {
            let (data, game) = temp(&format!("drop-{code}"));
            lap_marks(&data, 1, 1_000);
            lap_marks(&data, 2, 1_010);
            let mut replies = vec![Ok(receipt(1)), Err(status(code, None))];
            replies.extend((2..=5).map(|n| Ok(receipt(n))));
            let fake = Fake::new(replies);
            let mut sender = Sender::new(&data, folders(&data, &game));
            sender.step(&fake, Some("n"), 1_020, &|_| {});
            sender.step(&fake, Some("n"), 1_120, &|_| {});
            let seen = fake.seen.borrow();
            let sent: Vec<(u64, &str)> = seen
                .iter()
                .map(|(_, b)| (b["lap"].as_u64().unwrap(), b["mark"].as_str().unwrap()))
                .collect();
            assert_eq!(
                sent,
                [
                    (1, "start"),
                    (1, "s1"),
                    (2, "start"),
                    (2, "s1"),
                    (2, "s2"),
                    (2, "end")
                ],
                "{code}"
            );
            assert!(!pending(&data, &game));
            assert_eq!(for_lap(&data, Some("n"), Some(1), 91_234), Value::Null, "{code}");
            assert_eq!(
                for_lap(&data, Some("n"), Some(2), 91_234)["receipts"]
                    .as_array()
                    .map(Vec::len),
                Some(4)
            );
        }
    }

    #[test]
    fn a_lap_whose_sector_marks_never_came_says_its_finish_is_untimed() {
        let (data, game) = temp("gap");
        mark(&data, 1, 2, "start", 0, 1_000);
        mark(&data, 2, 2, "end", 90_360, 91_000);
        let fake = Fake::new((1..=2).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        let said = RefCell::new(Vec::new());
        sender.step(&fake, Some("n"), 91_010, &|s| {
            said.borrow_mut().push(s.to_owned())
        });
        let sent: Vec<String> = fake
            .seen
            .borrow()
            .iter()
            .map(|(_, b)| b["mark"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(sent, ["start"]);
        assert_eq!(
            *said.borrow(),
            ["timing: lap 2 end has no s2 receipt to chain to; that lap is untimed"]
        );
        assert!(!pending(&data, &game));
    }

    #[test]
    fn a_pause_goes_out_chained_to_the_laps_start_and_the_upload_keeps_the_line_receipts() {
        let (data, game) = temp("pause");
        for (seq, which, game_ms, at) in [
            (1, "start", 0, 1_000_000),
            (2, "s1", 33_024, 1_033_040),
            (3, "p1", 49_830, 1_049_830),
            (4, "r1", 49_830, 1_053_554),
            (5, "s2", 73_220, 1_076_945),
            (6, "end", 91_488, 1_111_584),
        ] {
            mark(&data, seq, 1, which, game_ms, at);
        }
        let fake = Fake::new((1..=6).map(|n| Ok(receipt(n))).collect());
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 1_111_600, &|_| {});
        sender.step(&fake, Some("n"), 1_111_700, &|_| {});
        let seen = fake.seen.borrow();
        let sent: Vec<(&str, Option<&str>)> = seen
            .iter()
            .map(|(_, b)| (b["mark"].as_str().unwrap(), b["prev"].as_str()))
            .collect();
        let (start, s1, p1, s2) = (receipt(1), receipt(2), receipt(3), receipt(5));
        assert_eq!(
            sent,
            [
                ("start", None),
                ("s1", Some(start.as_str())),
                ("p1", Some(start.as_str())),
                ("r1", Some(p1.as_str())),
                ("s2", Some(s1.as_str())),
                ("end", Some(s2.as_str())),
            ]
        );
        assert_eq!(seen[3].1["game_ms"], 49_830);
        assert!(!pending(&data, &game));
        assert_eq!(
            for_lap(&data, Some("n"), Some(1), 91_488),
            timing(1, &[receipt(1), receipt(2), receipt(5), receipt(6)])
        );
    }

    #[test]
    fn a_refused_pause_drops_only_itself_and_its_resume() {
        let (data, game) = temp("pause-refused");
        for (seq, which, game_ms, at) in [
            (1, "start", 0, 1_000),
            (2, "p1", 10_000, 11_000),
            (3, "r1", 10_000, 14_000),
            (4, "s1", 30_000, 34_000),
            (5, "s2", 61_000, 65_000),
            (6, "end", 90_000, 94_000),
        ] {
            mark(&data, seq, 1, which, game_ms, at);
        }
        let mut replies = vec![Ok(receipt(1)), Err(status(400, None))];
        replies.extend((2..=4).map(|n| Ok(receipt(n))));
        let fake = Fake::new(replies);
        let mut sender = Sender::new(&data, folders(&data, &game));
        let said = RefCell::new(Vec::new());
        sender.step(&fake, Some("n"), 94_010, &|s| {
            said.borrow_mut().push(s.to_owned())
        });
        sender.step(&fake, Some("n"), 94_110, &|s| {
            said.borrow_mut().push(s.to_owned())
        });
        let sent: Vec<String> = fake
            .seen
            .borrow()
            .iter()
            .map(|(_, b)| b["mark"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(sent, ["start", "p1", "s1", "s2", "end"]);
        assert_eq!(
            *said.borrow(),
            [
                "timing: lap 1 p1 refused (status 400); that pause stays in the lap's time",
                "timing: lap 1 r1 has no p1 receipt to chain to; that pause stays in the lap's time",
            ]
        );
        assert!(!pending(&data, &game));
        assert_eq!(for_lap(&data, Some("n"), Some(1), 90_000), timing(1, &all(4)));
    }

    #[test]
    fn a_session_the_server_does_not_time_stops_ticking() {
        for code in [404, 401] {
            let (data, game) = temp(&format!("stop-{code}"));
            lap_marks(&data, 1, 92_000);
            let fake = Fake::new(vec![Err(status(code, None)), Ok(receipt(1))]);
            let mut sender = Sender::new(&data, folders(&data, &game));
            sender.step(&fake, Some("n"), 92_010, &|_| {});
            lap_marks(&data, 2, 92_100);
            sender.step(&fake, Some("n"), 92_110, &|_| {});
            assert_eq!(fake.seen.borrow().len(), 1, "{code}");
            assert!(!pending(&data, &game));
            assert_eq!(for_lap(&data, Some("n"), Some(1), 91_234), Value::Null);
            lap_marks(&data, 3, 92_200);
            sender.step(&fake, Some("next"), 92_210, &|_| {});
            assert_eq!(fake.seen.borrow()[1].0, "next", "a new session ticks again");
        }
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
        let mut sent = Vec::new();
        let mut now = 92_000;
        while pending(&data, &game) && now < 300_000 {
            let before = fake.seen.borrow().len();
            sender.step(&fake, Some("n"), now, &|_| {});
            sent.extend(std::iter::repeat_n(now, fake.seen.borrow().len() - before));
            now += 100;
        }
        assert_eq!(sent.len(), 16);
        assert!(
            sent.iter()
                .all(|t| sent.iter().filter(|u| (*t..*t + MINUTE_MS).contains(u)).count() <= PER_MINUTE)
        );
        assert_eq!(
            for_lap(&data, Some("n"), Some(3), 91_234)["receipts"]
                .as_array()
                .map(Vec::len),
            Some(4)
        );
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
        mark(&data, 7, 2, "start", 0, 21);
        mark(&data, 8, 3, "start", 0, 22);
        let fake = Fake::new(vec![
            Ok("<script>".into()),
            Ok(format!("tk1.{}", "a".repeat(2000))),
            Ok("tk2.opaque.mac".into()),
        ]);
        let mut sender = Sender::new(&data, folders(&data, &game));
        sender.step(&fake, Some("n"), 30, &|_| {});
        let seen = fake.seen.borrow();
        let sent: Vec<u64> = seen.iter().map(|(_, b)| b["lap"].as_u64().unwrap()).collect();
        assert_eq!(
            sent,
            [1, 2, 3],
            "a lap whose start got no receipt sends nothing more"
        );
        assert!(!pending(&data, &game));
        assert_eq!(for_lap(&data, Some("n"), Some(1), 0), Value::Null);
        assert_eq!(for_lap(&data, Some("n"), Some(2), 0), Value::Null);
        assert_eq!(
            for_lap(&data, Some("n"), Some(3), 0),
            timing(3, &["tk2.opaque.mac".to_owned()])
        );
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
