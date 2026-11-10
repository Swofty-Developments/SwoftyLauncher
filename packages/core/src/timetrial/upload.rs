//! The loop that gets laps to the server: one pass over the laps folder and the outbox, and a watcher
//! that makes passes while the game runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::api::{Api, Boards, Nonce};
use super::bridge::Pump;
use super::queue::{Event, Folder, Limits, Queue, Report, unix_now};
use super::toast;
use super::{laps_dir, token};
use crate::session::Config;

const WATCH_EVERY: Duration = Duration::from_secs(5);
const FINAL_SWEEP: Duration = Duration::from_secs(6);

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
fn toast(game: &Path, queue: &Queue, event: &Event, say: &dyn Fn(&str)) {
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
    match toast::write(game, &doc) {
        Ok(()) => say(&format!(
            "toast: P{} on {}/{}",
            doc["position"], board.content, board.track
        )),
        Err(e) => say(&format!("toast: couldn't write the message: {e}")),
    }
}

/// Ask for a nonce for the game session about to start. A failure is not fatal: laps are uploaded
/// without one.
pub fn nonce(data: &Path, config: &Config) -> Option<Nonce> {
    Api::new(&token(data, config)?).sessions().ok()
}

/// One pass: collect the game's laps into the outbox, then upload what is due. With no token the laps
/// are collected and the old ones in the outbox expire.
pub fn sweep(
    data: &Path,
    game: Option<&Path>,
    api: Option<&dyn Boards>,
    fingerprint: Option<&Value>,
    deadline: Option<Instant>,
    on: &mut dyn FnMut(Event),
) -> Report {
    let queue = Queue::new(data);
    if let Some(game) = game {
        queue.collect(&laps_dir(game), fingerprint).ok();
    }
    let now = unix_now();
    queue.prune(now);
    match api {
        Some(api) => queue.drain(
            api,
            fingerprint.unwrap_or(&Value::Null),
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

/// The pass the launcher makes when it starts, for laps a session left queued.
pub fn flush(data: &Path, config: &Config) -> Report {
    let api = token(data, config).map(|t| Api::new(&t));
    sweep(
        data,
        None,
        api.as_ref().map(|a| a as &dyn Boards),
        None,
        None,
        &mut |_| {},
    )
}

/// Sweeps while the game runs. [`Watcher::finish`] makes the last sweep, with a few seconds at most.
pub struct Watcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pump: Option<Pump>,
    data: PathBuf,
    game: PathBuf,
    config: Config,
    fingerprint: Arc<dyn Fn() -> Option<Value> + Send + Sync>,
    say: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Watcher {
    /// With In-game leaderboards on and a sign-in, the bridge's mailbox pump runs beside it.
    /// With Time Trial boards off or nobody signed in, the laps are still collected into the outbox and
    /// nothing leaves the machine.
    pub fn start(
        data: &Path,
        game: &Path,
        config: &Config,
        fingerprint: impl Fn() -> Option<Value> + Send + Sync + 'static,
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
                    Pump::start(game, Api::new(&t), move |l| say(l))
                }),
            data: data.to_owned(),
            game: game.to_owned(),
            config: config.clone(),
            fingerprint: Arc::new(fingerprint),
            say,
        };
        let (stop, data, game, config) = (w.stop.clone(), w.data.clone(), w.game.clone(), w.config.clone());
        let (fingerprint, say) = (w.fingerprint.clone(), w.say.clone());
        w.thread = std::thread::Builder::new()
            .name("tt-laps".into())
            .spawn(move || {
                let mut cached: Option<Option<Value>> = None;
                while !stop.load(Ordering::Relaxed) {
                    if has_laps(&game) || has_outbox(&data) {
                        let print = cached.get_or_insert_with(|| fingerprint()).clone();
                        pass(&data, &game, &config, print.as_ref(), None, &*say);
                    }
                    let until = Instant::now() + WATCH_EVERY;
                    while Instant::now() < until && !stop.load(Ordering::Relaxed) {
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
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
        let print = has_laps(&self.game).then(|| (self.fingerprint)()).flatten();
        pass(
            &self.data,
            &self.game,
            &self.config,
            print.as_ref(),
            Some(Instant::now() + FINAL_SWEEP),
            &*self.say,
        );
    }
}

fn has_laps(game: &Path) -> bool {
    std::fs::read_dir(laps_dir(game)).is_ok_and(|d| {
        d.filter_map(Result::ok)
            .any(|e| e.path().extension().is_some_and(|x| x == "swlap"))
    })
}

fn has_outbox(data: &Path) -> bool {
    !Queue::new(data).list(Folder::Outbox).is_empty()
}

fn pass(
    data: &Path,
    game: &Path,
    config: &Config,
    fingerprint: Option<&Value>,
    deadline: Option<Instant>,
    say: &dyn Fn(&str),
) {
    let api = token(data, config).map(|t| match deadline {
        Some(d) => Api::new(&t).within(d.saturating_duration_since(Instant::now())),
        None => Api::new(&t),
    });
    let queue = Queue::new(data);
    sweep(
        data,
        Some(game),
        api.as_ref().map(|a| a as &dyn Boards),
        fingerprint,
        deadline,
        &mut |e| {
            say(&line(&e));
            if deadline.is_none() {
                toast(game, &queue, &e, say);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetrial::api::Uploaded;

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
        let laps = laps_dir(&root.join("game"));
        std::fs::create_dir_all(&laps).unwrap();
        std::fs::write(laps.join("a.swlap"), crate::timetrial::swlap::sample(1, true)).unwrap();
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
}
