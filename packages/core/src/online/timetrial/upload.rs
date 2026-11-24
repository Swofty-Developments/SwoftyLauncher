//! The loop that gets laps to the server: one pass over the laps folder and the outbox, and a watcher
//! that makes passes while the game runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::bridge::Pump;
use super::fingerprint::Session;
use super::game_replay;
use super::queue::{Event, Folder, Limits, Queue, Report, Stamp, unix_now};
use super::ticks;
use super::toast;
use super::{laps_dirs, token, write_session_keys};
use crate::online::server::api::{Api, Boards, Nonce};
use crate::session::Config;

const WATCH_EVERY: Duration = Duration::from_secs(5);
const FINAL_SWEEP: Duration = Duration::from_secs(6);
/// How long a new lap waits for the replay the core saves of a personal best, so the two upload together.
const REPLAY_WAIT: Duration = Duration::from_secs(10);
const HOLD_POLL: Duration = Duration::from_millis(300);
/// How long a new lap waits for the timing marks still being sent, so it uploads with its finish receipt.
const MARKS_WAIT: Duration = Duration::from_secs(5);
const NONCE_RETRIES: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(8),
];
const NONCE_LATER: Duration = Duration::from_secs(60);

/// The run log line for an event, tagged `laps` by the caller.
pub fn line(event: &Event) -> String {
    match event {
        Event::Uploaded { time_ms, answer, .. } => {
            let rank = answer.rank.map_or(String::new(), |r| format!(" -> P{r}"));
            format!("uploaded {time_ms} ms{rank} ({})", answer.status)
        }
        Event::Retry { error, .. } => format!("upload failed, will retry: {error}"),
        Event::Rejected { name, error } => format!("{name} not uploaded: {error}"),
    }
}

/// Tell the core a lap is the player's new best. Only while the game runs: the final sweep after it
/// exits has nobody to show it to.
fn toast(data: &Path, queue: &Queue, event: &Event, say: &dyn Fn(&str)) {
    let Event::Uploaded {
        board,
        time_ms,
        answer,
        ..
    } = event
    else {
        return;
    };
    let previous = toast::previous_best(queue, board, &answer.id);
    let Some(doc) = toast::message(board, *time_ms, answer, previous, unix_now()) else {
        return;
    };
    match toast::write(data, &doc) {
        Ok(()) => say(&format!(
            "toast: P{} on {}/{}",
            doc["position"], board.content, board.track
        )),
        Err(e) => say(&format!("toast: couldn't write the message: {e}")),
    }
}

/// Ask for a nonce for the game session about to start, for the game build `game_build` (the exe's sha256)
/// when it is known. A failure is not fatal here: the watcher keeps asking while the game runs, and the core
/// picks the nonce up for the laps written after it arrives.
pub fn nonce(data: &Path, config: &Config, game_build: Option<&str>) -> Result<Nonce, String> {
    let token = token(data, config).ok_or_else(|| "nobody is signed in".to_owned())?;
    Api::new(&token).sessions(game_build).map_err(|e| e.to_string())
}

/// How long the watcher waits before asking for the nonce again: three quick tries over about 15 s,
/// then once a minute while the game runs.
pub fn nonce_retry_pause(attempt: usize) -> Duration {
    NONCE_RETRIES.get(attempt).copied().unwrap_or(NONCE_LATER)
}

/// Keeps asking for the session nonce until the server issues one or the game ends, and puts it in the
/// session file for the core. Without it every lap of the session is refused by the server.
pub(crate) fn nonce_until_issued(
    data: &Path,
    game: &Path,
    config: &Config,
    game_build: Option<&str>,
    stop: &dyn Fn() -> bool,
    say: &dyn Fn(&str),
) {
    for attempt in 0.. {
        let until = Instant::now() + nonce_retry_pause(attempt);
        while Instant::now() < until {
            if stop() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if stop() || token(data, config).is_none() {
            return;
        }
        match nonce(data, config, game_build) {
            Ok(n) => {
                match write_session_keys(game, Some(&n), config.time_trial_ingame) {
                    Ok(()) => say(&format!(
                        "session nonce issued on retry {}: the laps written from here carry it",
                        attempt + 1
                    )),
                    Err(e) => say(&format!(
                        "session nonce issued but the session file is not writable: {e}"
                    )),
                }
                return;
            }
            Err(e) => say(&format!(
                "session nonce: {e}; laps without one are refused by the server, asking again in {} s",
                nonce_retry_pause(attempt + 1).as_secs()
            )),
        }
    }
}

/// One pass: collect the game's laps into the outbox, then upload what is due. With no token the laps
/// are collected and the old ones in the outbox expire.
pub fn sweep(
    data: &Path,
    game: Option<&Path>,
    api: Option<&dyn Boards>,
    stamp: Option<&Stamp>,
    deadline: Option<Instant>,
    on: &mut dyn FnMut(Event),
) -> Report {
    let queue = Queue::new(data);
    if let Some(game) = game {
        for laps in laps_dirs(data, game) {
            queue.collect(&laps, stamp).ok();
        }
    }
    let now = unix_now();
    queue.prune(now);
    match api {
        Some(api) => queue.drain(
            api,
            stamp.map_or(&Value::Null, |s| &s.fingerprint),
            Limits {
                now,
                deadline,
                ignore_backoff: false,
            },
            on,
        ),
        None => Report {
            waiting: queue.list(Folder::Outbox).len(),
            ..Report::default()
        },
    }
}

/// The pass the launcher makes when it starts, for laps and game replays a session left queued.
pub fn flush(data: &Path, game: Option<&Path>, config: &Config) -> Report {
    let api = token(data, config).map(|t| Api::new(&t));
    let report = sweep(
        data,
        None,
        api.as_ref().map(|a| a as &dyn Boards),
        None,
        None,
        &mut |_| {},
    );
    if let (Some(game), Some(api)) = (game, &api) {
        game_replay::send(
            &Queue::new(data),
            &game_replay::dirs(data, game),
            api,
            unix_now(),
            &|| false,
            &mut |_| {},
        );
    }
    report
}

/// Upload the game replays the core saved in `game` whose laps the server has, if someone is signed in.
pub fn replays(data: &Path, game: &Path, config: &Config, stop: &dyn Fn() -> bool, say: &dyn Fn(&str)) {
    static BUSY: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Ok(_busy) = BUSY.try_lock() else {
        return;
    };
    let dirs = game_replay::dirs(data, game);
    if !game_replay::any(&dirs) {
        return;
    }
    let Some(api) = token(data, config).map(|t| Api::new(&t)) else {
        return;
    };
    game_replay::send(&Queue::new(data), &dirs, &api, unix_now(), stop, &mut |l| say(l));
}

/// Sweeps while the game runs. [`Watcher::finish`] makes the last sweep, with a few seconds at most.
/// Game replays upload on a thread of their own that the finish does not wait for.
pub struct Watcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pump: Option<Pump>,
    ticks: Option<ticks::Pump>,
    data: PathBuf,
    game: PathBuf,
    config: Config,
    fingerprint: Session,
    integrity: Value,
    say: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Watcher {
    /// With In-game leaderboards on and a sign-in, the bridge's mailbox pump runs beside it, and with a sign-in
    /// the timing marks of the session's laps go to the server as they are made.
    /// With Time Trial boards off or nobody signed in, the laps are still collected into the outbox and
    /// nothing leaves the machine. With `retry_nonce` the session nonce the start could not get is asked
    /// for again on a thread of its own, for `game_build`. Each lap is kept with `integrity`, what the session did about the
    /// integrity module, and the addon set `fingerprint` holds for the session, for its upload meta.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        data: &Path,
        game: &Path,
        config: &Config,
        retry_nonce: bool,
        game_build: Option<String>,
        integrity: Value,
        fingerprint: Session,
        say: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        let say: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(say);
        let mut w = Self {
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            pump: config
                .time_trial_ingame
                .then(|| token(data, config))
                .flatten()
                .map(|t| {
                    let say = say.clone();
                    Pump::start(&super::bridge::mailboxes(data, game), Api::new(&t), move |l| {
                        say(l)
                    })
                }),
            ticks: token(data, config).map(|t| {
                let say = say.clone();
                ticks::Pump::start(data, game, Api::new(&t), move |l| say(l))
            }),
            data: data.to_owned(),
            game: game.to_owned(),
            config: config.clone(),
            fingerprint,
            integrity,
            say,
        };
        let (stop, data, game, config) = (w.stop.clone(), w.data.clone(), w.game.clone(), w.config.clone());
        let (fingerprint, integrity, say) = (w.fingerprint.clone(), w.integrity.clone(), w.say.clone());
        w.thread = std::thread::Builder::new()
            .name("tt-laps".into())
            .spawn(move || {
                let mut first = FirstLap::default();
                while !stop.load(Ordering::Relaxed) {
                    let laps = has_laps(&data, &game);
                    if laps && (waiting_for_replay(&data, &game) || waiting_for_marks(&data, &game)) {
                        std::thread::sleep(HOLD_POLL);
                        continue;
                    }
                    if laps || has_outbox(&data) {
                        let asked = Instant::now();
                        let print = fingerprint.get();
                        if laps {
                            first.seen(newest_lap(&data, &game), asked.elapsed());
                        }
                        let stamp = Stamp {
                            fingerprint: print.unwrap_or(Value::Null),
                            integrity: integrity.clone(),
                        };
                        let report = pass(&data, &game, &config, Some(&stamp), None, &*say);
                        if let Some(line) = first.uploaded(&report) {
                            say(&line);
                        }
                        if laps {
                            replays(&data, &game, &config, &|| stop.load(Ordering::Relaxed), &*say);
                        }
                    }
                    let until = Instant::now() + WATCH_EVERY;
                    while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            })
            .ok();
        if retry_nonce {
            let (stop, data, game, config) =
                (w.stop.clone(), w.data.clone(), w.game.clone(), w.config.clone());
            let say = w.say.clone();
            std::thread::Builder::new()
                .name("tt-nonce".into())
                .spawn(move || {
                    let stopped = || stop.load(Ordering::Relaxed);
                    nonce_until_issued(&data, &game, &config, game_build.as_deref(), &stopped, &*say);
                })
                .ok();
        }
        let (stop, data, game, config) = (w.stop.clone(), w.data.clone(), w.game.clone(), w.config.clone());
        let say = w.say.clone();
        std::thread::Builder::new()
            .name("tt-replays".into())
            .spawn(move || {
                let stopped = || stop.load(Ordering::Relaxed);
                while !stopped() {
                    replays(&data, &game, &config, &stopped, &*say);
                    let until = Instant::now() + WATCH_EVERY;
                    while Instant::now() < until && !stopped() {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            })
            .ok();
        w
    }

    /// Stop the watcher and the bridge and sweep once more, giving up after a few seconds.
    pub fn finish(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(pump) = self.pump.take() {
            pump.finish();
        }
        if let Some(ticks) = self.ticks.take() {
            ticks.finish();
        }
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
        let stamp = has_laps(&self.data, &self.game).then(|| Stamp {
            fingerprint: self.fingerprint.get().unwrap_or(Value::Null),
            integrity: self.integrity.clone(),
        });
        pass(
            &self.data,
            &self.game,
            &self.config,
            stamp.as_ref(),
            Some(Instant::now() + FINAL_SWEEP),
            &*self.say,
        );
    }
}

/// When the session's first lap was written and how long its pass waited for the fingerprint, until the run log
/// says how long after it was written it reached the server.
#[derive(Default)]
struct FirstLap {
    written: Option<std::time::SystemTime>,
    waited: Duration,
    told: bool,
}

impl FirstLap {
    fn seen(&mut self, written: Option<std::time::SystemTime>, waited: Duration) {
        if self.written.is_none() {
            self.written = written;
            self.waited = waited;
        }
    }

    fn uploaded(&mut self, report: &Report) -> Option<String> {
        let written = self.written.filter(|_| !self.told && report.uploaded > 0)?;
        self.told = true;
        Some(format!(
            "first lap of the session uploaded {:.1} s after it was written, {:.1} s of it waiting for the addon set fingerprint",
            written.elapsed().unwrap_or_default().as_secs_f64(),
            self.waited.as_secs_f64()
        ))
    }
}

/// Whether a lap is held back for its replay: the newest lap is younger than [`REPLAY_WAIT`] and no
/// replay saved since it was written is ready yet.
fn hold_for_replay(lap_age: Duration, replay_ready: bool) -> bool {
    !replay_ready && lap_age < REPLAY_WAIT
}

fn newest_lap(data: &Path, game: &Path) -> Option<std::time::SystemTime> {
    laps_dirs(data, game)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|d| d.filter_map(Result::ok))
        .filter(|e| e.path().extension().is_some_and(|x| x == "swlap"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

fn waiting_for_replay(data: &Path, game: &Path) -> bool {
    let Some(written) = newest_lap(data, game) else {
        return false;
    };
    let age = written.elapsed().unwrap_or_default();
    let since = written
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .saturating_sub(2);
    let ready = game_replay::ready(&game_replay::dirs(data, game), unix_now())
        .iter()
        .any(|p| p.secs >= since);
    hold_for_replay(age, ready)
}

fn waiting_for_marks(data: &Path, game: &Path) -> bool {
    newest_lap(data, game).is_some_and(|t| t.elapsed().unwrap_or_default() < MARKS_WAIT)
        && ticks::pending(data, game)
}

fn has_laps(data: &Path, game: &Path) -> bool {
    laps_dirs(data, game).iter().any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|d| {
            d.filter_map(Result::ok)
                .any(|e| e.path().extension().is_some_and(|x| x == "swlap"))
        })
    })
}

fn has_outbox(data: &Path) -> bool {
    !Queue::new(data).list(Folder::Outbox).is_empty()
}

fn pass(
    data: &Path,
    game: &Path,
    config: &Config,
    stamp: Option<&Stamp>,
    deadline: Option<Instant>,
    say: &dyn Fn(&str),
) -> Report {
    let api = token(data, config).map(|t| match deadline {
        Some(d) => Api::new(&t).within(d.saturating_duration_since(Instant::now())),
        None => Api::new(&t),
    });
    let queue = Queue::new(data);
    sweep(
        data,
        Some(game),
        api.as_ref().map(|a| a as &dyn Boards),
        stamp,
        deadline,
        &mut |e| {
            say(&line(&e));
            if deadline.is_none() {
                toast(data, &queue, &e, say);
            }
        },
    )
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_new_lap_waits_for_its_replay_but_never_long() {
        assert!(super::hold_for_replay(Duration::from_secs(2), false));
        assert!(!super::hold_for_replay(Duration::from_secs(2), true));
        assert!(!super::hold_for_replay(super::REPLAY_WAIT, false));
    }

    use super::*;
    use crate::online::server::api::Uploaded;

    #[test]
    fn the_nonce_is_asked_for_three_times_quickly_and_then_once_a_minute() {
        let quick: Duration = (0..3).map(nonce_retry_pause).sum();
        assert_eq!(quick, Duration::from_secs(15));
        assert_eq!(nonce_retry_pause(3), Duration::from_secs(60));
        assert_eq!(nonce_retry_pause(40), Duration::from_secs(60));
    }

    #[test]
    fn the_run_log_line_names_the_time_rank_and_status() {
        let answer = Uploaded {
            id: "x".into(),
            status: "ranked".into(),
            rank: Some(12),
            best: Some(true),
            total: None,
            previous_best_ms: None,
            reasons: vec![],
            url: None,
        };
        let e = Event::Uploaded {
            name: "a".into(),
            board: Box::default(),
            time_ms: 91234,
            answer,
        };
        assert_eq!(line(&e), "uploaded 91234 ms -> P12 (ranked)");
    }

    #[test]
    fn a_sweep_with_no_sign_in_collects_and_uploads_nothing() {
        let root = std::env::temp_dir().join(format!("swofty-ttu-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let [_, laps] = laps_dirs(&root.join("data"), &root.join("game"));
        std::fs::create_dir_all(&laps).unwrap();
        std::fs::write(
            laps.join("a.swlap"),
            crate::online::timetrial::swlap::sample(1, true),
        )
        .unwrap();
        let r = sweep(
            &root.join("data"),
            Some(&root.join("game")),
            None,
            None,
            None,
            &mut |_| {},
        );
        assert_eq!((r.uploaded, r.waiting), (0, 1));
        assert!(!laps.join("a.swlap").exists());
        assert_eq!(Queue::new(&root.join("data")).list(Folder::Outbox).len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn laps_in_the_data_folder_and_the_game_folder_are_both_collected() {
        let root = std::env::temp_dir().join(format!("swofty-ttu-both-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (data, game) = (root.join("data"), root.join("game"));
        assert!(!has_laps(&data, &game));
        let [new, old] = laps_dirs(&data, &game);
        assert_eq!(new, data.join("laps"));
        assert_eq!(old, game.join("swofty").join("laps"));
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(
            new.join("a.swlap"),
            crate::online::timetrial::swlap::sample(1, true),
        )
        .unwrap();
        assert!(has_laps(&data, &game));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("b.swlap"),
            crate::online::timetrial::swlap::sample(2, true),
        )
        .unwrap();
        let r = sweep(&data, Some(&game), None, None, None, &mut |_| {});
        assert_eq!((r.uploaded, r.waiting), (0, 2));
        assert!(!new.join("a.swlap").exists() && !old.join("b.swlap").exists());
        assert!(!has_laps(&data, &game));
        std::fs::remove_dir_all(&root).ok();
    }

    fn lap_with_nonce(time_ms: u64, nonce: &str) -> Vec<u8> {
        let header = serde_json::json!({
            "format": 1,
            "nonce": nonce,
            "board": {"content": "game/f1_2025", "track": "bahrain", "weather": "dry"},
            "lap": {"time_ms": time_ms, "valid": true, "sectors_ms": [1, 2, 3]},
        })
        .to_string();
        let mut out = crate::online::timetrial::swlap::MAGIC.to_vec();
        out.extend((header.len() as u32).to_le_bytes());
        out.extend(header.as_bytes());
        out.extend(b"END\0\0\0\0\0");
        out
    }

    fn stub(replies: Vec<(&'static str, &'static str)>) -> (String, JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let t = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut c, _) = listener.accept().unwrap();
                let mut got = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = c.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&buf[..n]);
                    let Some(h) = got.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&got[..h]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: ")?.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if got.len() >= h + 4 + len {
                        break;
                    }
                }
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                c.write_all(reply.as_bytes()).unwrap();
                seen.push(String::from_utf8_lossy(&got).into_owned());
            }
            seen
        });
        (url, t)
    }

    fn temp(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("swofty-ttu-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        (root.clone(), root.join("data"), root.join("game"))
    }

    #[test]
    fn a_sweep_sends_the_laps_of_both_folders_to_the_server_with_their_nonces() {
        let (root, data, game) = temp("wire");
        let [new, old] = laps_dirs(&data, &game);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("20261118T000001-1.swlap"),
            lap_with_nonce(91000, "n-game"),
        )
        .unwrap();
        std::fs::write(
            new.join("20261118T000002-1.swlap"),
            lap_with_nonce(92000, "n-data"),
        )
        .unwrap();
        let (url, t) = stub(vec![
            ("200 OK", r#"{"id":"l1","status":"ranked","rank":3}"#),
            (
                "200 OK",
                r#"{"id":"l2","status":"pending","reasons":["needs review: attest"]}"#,
            ),
        ]);
        let api = Api::with_server(&url, "tok");
        let mut events = Vec::new();
        let r = sweep(
            &data,
            Some(&game),
            Some(&api as &dyn Boards),
            Some(&Stamp {
                fingerprint: serde_json::json!({"digest": "d"}),
                integrity: serde_json::json!({"state": "fetch_failed", "version": null, "source": null, "detail": "timeout"}),
            }),
            None,
            &mut |e| events.push(line(&e)),
        );
        assert_eq!((r.uploaded, r.waiting, r.rejected), (2, 0, 0), "{events:?}");
        assert!(!has_laps(&data, &game) && !has_outbox(&data));
        let seen = t.join().unwrap();
        let sessions: Vec<&str> = seen
            .iter()
            .map(|req| {
                assert!(req.starts_with("POST /v1/tt/laps HTTP/1.1"), "{req}");
                assert!(req.to_ascii_lowercase().contains("authorization: bearer tok"));
                assert!(req.contains(&format!(r#""launcher":"{}""#, env!("CARGO_PKG_VERSION"))));
                assert!(req.contains(r#""fingerprint":{"digest":"d"}"#), "{req}");
                let integrity = req.split(r#""integrity":"#).nth(1).unwrap_or_default();
                let integrity = &integrity[..integrity.find('}').map_or(0, |end| end + 1)];
                for part in [
                    r#""state":"fetch_failed""#,
                    r#""version":null"#,
                    r#""source":null"#,
                    r#""detail":"timeout""#,
                ] {
                    assert!(integrity.contains(part), "{req}");
                }
                assert!(req.contains(r#""gzip":true"#));
                if req.contains(r#""session":"n-game""#) {
                    "n-game"
                } else if req.contains(r#""session":"n-data""#) {
                    "n-data"
                } else {
                    ""
                }
            })
            .collect();
        assert_eq!(sessions.len(), 2);
        assert!(
            sessions.contains(&"n-game") && sessions.contains(&"n-data"),
            "{sessions:?}"
        );
        let sent = Queue::new(&data).list(Folder::Sent);
        assert_eq!(sent.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    fn lap_numbered(time_ms: u64, nonce: &str, lap_index: u32) -> Vec<u8> {
        let header = serde_json::json!({
            "format": 1,
            "nonce": nonce,
            "board": {"content": "game/f1_2025", "track": "bahrain", "weather": "dry"},
            "lap": {"time_ms": time_ms, "valid": true, "sectors_ms": [1, 2, 3], "lap_index": lap_index},
        })
        .to_string();
        let mut out = crate::online::timetrial::swlap::MAGIC.to_vec();
        out.extend((header.len() as u32).to_le_bytes());
        out.extend(header.as_bytes());
        out.extend(b"END\0\0\0\0\0");
        out
    }

    #[test]
    fn a_timed_lap_keeps_its_receipts_through_the_queue_and_an_offline_lap_uploads_without_any() {
        let (root, data, game) = temp("timed");
        let [laps, _] = laps_dirs(&data, &game);
        std::fs::create_dir_all(&laps).unwrap();
        let marks = ticks::folders(&data, &game)[0].clone();
        std::fs::create_dir_all(&marks).unwrap();
        let now = unix_now() * 1000;
        for (seq, (which, game_ms)) in [("start", 0), ("s1", 28_411), ("s2", 59_000), ("end", 91_234)]
            .into_iter()
            .enumerate()
        {
            let at = now + seq as u64;
            let body =
                serde_json::json!({"seq": seq + 1, "lap": 4, "mark": which, "game_ms": game_ms, "at_ms": at});
            std::fs::write(marks.join(format!("{at}-{}.json", seq + 1)), body.to_string()).unwrap();
        }
        let (url, t) = stub(vec![
            ("200 OK", r#"{"receipt":"tk1.cw.bQ"}"#),
            ("200 OK", r#"{"receipt":"tk1.czE.bQ"}"#),
            ("200 OK", r#"{"receipt":"tk1.czI.bQ"}"#),
            ("200 OK", r#"{"receipt":"tk1.ZQ.bQ"}"#),
            ("503 Service Unavailable", ""),
            ("200 OK", r#"{"id":"l1","status":"ranked","rank":3}"#),
            ("200 OK", r#"{"id":"l2","status":"ranked","rank":9}"#),
        ]);
        let api = Api::with_server(&url, "tok");
        let mut sender = ticks::Sender::new(&data, ticks::folders(&data, &game));
        sender.step(&api, Some("n-online"), now + 20, &|_| {});
        assert!(!ticks::pending(&data, &game));
        std::fs::write(
            laps.join("20261118T000001-1.swlap"),
            lap_numbered(91_234, "n-online", 4),
        )
        .unwrap();
        std::fs::write(
            laps.join("20261118T000002-1.swlap"),
            lap_numbered(90_100, "n-online", 5),
        )
        .unwrap();

        let r = sweep(
            &data,
            Some(&game),
            Some(&api as &dyn Boards),
            None,
            None,
            &mut |_| {},
        );
        assert_eq!(
            (r.uploaded, r.waiting),
            (0, 2),
            "the server is busy: both laps wait in the outbox"
        );
        let queued = Queue::new(&data).list(Folder::Outbox);
        assert_eq!(
            queued[0].waiting.as_ref().map(|w| w.timing.clone()),
            Some(serde_json::json!({"lap": 4, "receipts": [
                "tk1.cw.bQ",
                "tk1.czE.bQ",
                "tk1.czI.bQ",
                "tk1.ZQ.bQ"
            ]}))
        );
        assert_eq!(
            queued[1].waiting.as_ref().map(|w| w.timing.clone()),
            Some(Value::Null)
        );

        let queue = Queue::new(&data);
        for entry in queue.list(Folder::Outbox) {
            let side = entry.path.with_extension("json");
            let mut w: serde_json::Value = serde_json::from_slice(&std::fs::read(&side).unwrap()).unwrap();
            w["next_try"] = 0.into();
            std::fs::write(&side, w.to_string()).unwrap();
        }
        let r = sweep(&data, None, Some(&api as &dyn Boards), None, None, &mut |_| {});
        assert_eq!(r.uploaded, 2);
        let seen = t.join().unwrap();
        assert_eq!(seen.len(), 7);
        assert!(
            seen[1].starts_with("POST /v1/tt/sessions/n-online/ticks "),
            "{}",
            seen[1]
        );
        assert!(seen[1].contains(r#""prev":"tk1.cw.bQ""#), "{}", seen[1]);
        let uploads = &seen[5..];
        assert!(uploads.iter().all(|u| u.starts_with("POST /v1/tt/laps ")));
        assert!(
            uploads[0].contains(
                r#""timing":{"lap":4,"receipts":["tk1.cw.bQ","tk1.czE.bQ","tk1.czI.bQ","tk1.ZQ.bQ"]}"#
            ),
            "{}",
            uploads[0]
        );
        assert!(!uploads[1].contains(r#""timing""#), "{}", uploads[1]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_refused_lap_is_set_aside_and_a_server_error_keeps_the_rest_in_the_outbox() {
        let (root, data, game) = temp("fail");
        let [_, laps] = laps_dirs(&data, &game);
        std::fs::create_dir_all(&laps).unwrap();
        std::fs::write(laps.join("20261118T000001-1.swlap"), lap_with_nonce(91000, "n")).unwrap();
        std::fs::write(laps.join("20261118T000002-1.swlap"), lap_with_nonce(92000, "n")).unwrap();
        std::fs::write(laps.join("20261118T000003-1.swlap"), lap_with_nonce(93000, "n")).unwrap();
        let (url, t) = stub(vec![
            ("400 Bad Request", r#"{"error":"the lap file is not valid"}"#),
            ("503 Service Unavailable", ""),
        ]);
        let api = Api::with_server(&url, "tok");
        let mut events = Vec::new();
        let r = sweep(
            &data,
            Some(&game),
            Some(&api as &dyn Boards),
            None,
            None,
            &mut |e| events.push(line(&e)),
        );
        assert_eq!((r.uploaded, r.waiting, r.rejected), (0, 2, 1), "{events:?}");
        assert_eq!(
            events[0],
            "20261118T000001-1.swlap not uploaded: the lap file is not valid"
        );
        assert!(events[1].starts_with("upload failed, will retry: "), "{events:?}");
        assert_eq!(t.join().unwrap().len(), 2);
        let q = Queue::new(&data);
        assert_eq!(q.list(Folder::Rejected).len(), 1);
        let waiting = q.list(Folder::Outbox);
        assert_eq!(waiting.len(), 2);
        assert!(
            waiting
                .iter()
                .all(|e| e.waiting.as_ref().is_some_and(|w| w.next_try > unix_now()))
        );
        let r = sweep(
            &data,
            Some(&game),
            Some(&api as &dyn Boards),
            None,
            None,
            &mut |_| {},
        );
        assert_eq!((r.uploaded, r.waiting), (0, 2));
        std::fs::remove_dir_all(&root).ok();
    }

    struct Counted(Arc<std::sync::atomic::AtomicUsize>);

    fn counted_print() -> crate::online::multiplayer::fingerprint::Fingerprint {
        crate::online::multiplayer::fingerprint::Fingerprint::new(
            crate::game_folder::baseline::Identity {
                build_id: "1".into(),
                exe_sha256: "e".repeat(64),
            },
            None,
            Vec::new(),
            Default::default(),
            None,
        )
    }

    impl super::super::fingerprint::Setup for Counted {
        fn inputs(&self) -> super::super::fingerprint::Inputs {
            super::super::fingerprint::Inputs::default()
        }

        fn fingerprint(&self) -> Option<crate::online::multiplayer::fingerprint::Fingerprint> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Some(counted_print())
        }

        fn registry_ids(&self) -> Value {
            Value::Null
        }
    }

    fn written_long_ago(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - Duration::from_secs(60))
            .unwrap();
    }

    #[test]
    fn the_session_laps_carry_the_fingerprint_its_start_worked_out_and_the_finish_does_not_redo_it() {
        let (root, data, game) = temp("session-print");
        let [laps, _] = laps_dirs(&data, &game);
        std::fs::create_dir_all(&laps).unwrap();
        let made = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let session = Session::start(Counted(made.clone()), |_| {});
        let end = Instant::now() + Duration::from_secs(20);
        while made.load(Ordering::SeqCst) == 0 {
            assert!(
                Instant::now() < end,
                "the session start never worked the fingerprint out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        written_long_ago(&laps.join("20261122T000001-1.swlap"), &lap_with_nonce(91000, "n"));
        let w = Watcher::start(
            &data,
            &game,
            &Config::default(),
            false,
            None,
            Value::Null,
            session,
            |_| {},
        );
        let queue = Queue::new(&data);
        while queue.list(Folder::Outbox).is_empty() {
            assert!(Instant::now() < end, "the watcher never collected the lap");
            std::thread::sleep(Duration::from_millis(20));
        }
        written_long_ago(&laps.join("20261122T000002-1.swlap"), &lap_with_nonce(92000, "n"));
        w.finish();
        let outbox = queue.list(Folder::Outbox);
        assert_eq!(outbox.len(), 2);
        let wire = crate::online::timetrial::fingerprint::wire(&counted_print());
        for e in &outbox {
            assert_eq!(e.waiting.as_ref().map(|w| &w.fingerprint), Some(&wire));
        }
        assert_eq!(made.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_run_log_says_once_how_long_the_first_lap_took_to_reach_the_server() {
        let mut first = FirstLap::default();
        let written = std::time::SystemTime::now() - Duration::from_secs(12);
        first.seen(Some(written), Duration::from_millis(2500));
        first.seen(Some(std::time::SystemTime::now()), Duration::ZERO);
        assert_eq!(first.uploaded(&Report::default()), None);
        let sent = Report {
            uploaded: 1,
            ..Report::default()
        };
        let line = first.uploaded(&sent).unwrap();
        assert!(
            line.starts_with("first lap of the session uploaded 12."),
            "{line}"
        );
        assert!(
            line.ends_with(", 2.5 s of it waiting for the addon set fingerprint"),
            "{line}"
        );
        assert_eq!(first.uploaded(&sent), None);
    }

    #[test]
    fn the_startup_flush_sends_the_outbox_but_leaves_the_game_folder_alone() {
        let (root, data, game) = temp("flush");
        let [_, laps] = laps_dirs(&data, &game);
        std::fs::create_dir_all(&laps).unwrap();
        std::fs::write(laps.join("20261118T000001-1.swlap"), lap_with_nonce(91000, "n")).unwrap();
        sweep(&data, Some(&game), None, None, None, &mut |_| {});
        std::fs::write(laps.join("20261118T000002-1.swlap"), lap_with_nonce(92000, "n")).unwrap();
        let (url, t) = stub(vec![("200 OK", r#"{"id":"l1","status":"ranked"}"#)]);
        let api = Api::with_server(&url, "tok");
        let r = sweep(&data, None, Some(&api as &dyn Boards), None, None, &mut |_| {});
        assert_eq!((r.uploaded, r.waiting), (1, 0));
        assert_eq!(t.join().unwrap().len(), 1);
        assert!(laps.join("20261118T000002-1.swlap").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
