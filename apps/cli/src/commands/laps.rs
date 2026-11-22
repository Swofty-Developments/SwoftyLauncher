use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::Value;
use swofty_core::install::Game;
use swofty_core::online::server::api::{Api, Boards};
use swofty_core::online::server::integrity::{self, Module};
use swofty_core::online::timetrial::barred::{self, Barred};
use swofty_core::online::timetrial::fingerprint::{self, Played, Session, Setup};
use swofty_core::online::timetrial::queue::{Entry, Folder, Limits, Queue, unix_now};
use swofty_core::online::timetrial::upload::{self, Watcher};
use swofty_core::online::timetrial::{self, token};
use swofty_core::store::Store;

use crate::commands::{Execute, Invocation};
use crate::console::output::say;

impl Execute for LapsCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, &invocation.profile, self)
    }
}

#[derive(Subcommand)]
pub enum LapsCommand {
    #[command(about = "List the Time Trial laps waiting to upload and the ones sent, with their status")]
    List,
    #[command(about = "Upload the waiting laps now, or one lap file")]
    Upload {
        #[arg(help = "A .swlap file; without it every waiting lap is sent")]
        file: Option<PathBuf>,
    },
    #[command(about = "Print the address of the Time Trial boards")]
    Open,
    #[command(about = "Send one in-game bridge request file to the server and print the answer")]
    BridgeTest {
        #[arg(help = "A request in the format the game writes to swofty/tt/outbox")]
        file: PathBuf,
    },
    #[command(about = "Show a made-up lap position toast in the running Time Trial session")]
    ToastTest {
        #[arg(long, help = "Show the NEW RECORD form (position 1)")]
        record: bool,
    },
}

pub fn command(store: &Store, profile: &str, cmd: LapsCommand) -> Result<ExitCode> {
    match cmd {
        LapsCommand::List => list(store, profile),
        LapsCommand::Upload { file } => send(store, profile, file)?,
        LapsCommand::Open => println!("{}", timetrial::boards_url()),
        LapsCommand::BridgeTest { file } => bridge_test(store, &file)?,
        LapsCommand::ToastTest { record } => toast_test(store, record)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn toast_test(store: &Store, record: bool) -> Result<()> {
    let doc = timetrial::toast::sample(unix_now(), record);
    timetrial::toast::write(&store.root, &doc)?;
    println!(
        "wrote {}; it shows within a second in a running Time Trial session",
        timetrial::toast::dir(&store.root).display()
    );
    Ok(())
}

fn bridge_test(store: &Store, file: &std::path::Path) -> Result<()> {
    let Some(token) = token(&store.root, &store.config()) else {
        bail!("Time Trial boards is off or you are not signed in");
    };
    let mut request: Value = serde_json::from_slice(&std::fs::read(file)?)?;
    let Some(object) = request.as_object_mut() else {
        bail!("{} is not a JSON object", file.display());
    };
    object.remove("seq");
    let answer = Api::new(&token).game_f1os(&request)?;
    println!("status {}", answer.get("status").unwrap_or(&Value::Null));
    match answer.get("body") {
        Some(Value::String(body)) => println!("{body}"),
        Some(other) => println!("{other}"),
        None => println!("(no body)"),
    }
    Ok(())
}

fn clock(ms: u64) -> String {
    format!("{}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

fn describe(e: &Entry) -> String {
    let (track, weather, ms, valid) = match (&e.done, &e.header) {
        (Some(d), _) => (d.track.clone(), d.weather.clone(), d.time_ms, d.valid),
        (None, Some(h)) => (
            h.board.track.clone(),
            h.board.weather.clone(),
            h.lap.time_ms,
            h.lap.valid,
        ),
        (None, None) => ("?".into(), String::new(), 0, false),
    };
    let state = match e.folder {
        Folder::Outbox => match e.waiting.as_ref().filter(|w| w.attempts > 0) {
            Some(w) => format!(
                "waiting (tried {}x: {})",
                w.attempts,
                w.last_error.as_deref().unwrap_or("")
            ),
            None => "waiting".into(),
        },
        Folder::Sent => match e.done.as_ref().and_then(|d| d.answer.as_ref()) {
            Some(a) => match a.rank {
                Some(r) => format!("{} P{r}", a.status),
                None => a.status.clone(),
            },
            None => "sent".into(),
        },
        Folder::Rejected => format!(
            "not sent: {}",
            e.done
                .as_ref()
                .and_then(|d| d.error.as_deref())
                .unwrap_or("refused")
        ),
    };
    let lap = if valid { "" } else { " (invalid lap)" };
    format!("{track} {weather} {}{lap}  {state}", clock(ms))
}

fn list(store: &Store, profile: &str) {
    let queue = Queue::new(&store.root);
    let mut any = false;
    for folder in [Folder::Outbox, Folder::Rejected, Folder::Sent] {
        let mut entries = queue.list(folder);
        entries.reverse();
        for e in entries.iter().take(50) {
            any = true;
            println!("{}", describe(e));
        }
    }
    if !any {
        println!("no Time Trial laps yet");
    }
    if let Ok(game) = crate::commands::build::game(store, None) {
        let dirs = timetrial::game_replay::dirs(&store.root, &game.dir);
        for pair in timetrial::game_replay::ready(&dirs, unix_now()) {
            println!("game replay of a {} lap  waiting", clock(pair.lap_ms));
        }
    }
    if token(&store.root, &store.config()).is_none() {
        println!(
            "Time Trial boards uploads are off: sign in with Discord and leave Time Trial boards on in Settings"
        );
    }
    let listed = barred::current(&store.root, unix_now()).unwrap_or_default();
    let enabled = barred::for_store(store, profile, &listed);
    if let Some(line) = barred::warning(&enabled.iter().collect::<Vec<_>>()) {
        println!("{line}");
        for b in &enabled {
            match b.reason.trim() {
                "" => println!("  {} ({})", b.title(), b.id),
                reason => println!("  {} ({}): {reason}", b.title(), b.id),
            }
        }
    }
}

/// Hand the core the barred addons the profile has enabled, from the cached list, and ask the server for a
/// newer list on a thread of its own when the cached one is old or missing. The launch never waits for it.
fn barred_for_launch(store: &Store, profile: &str, game: &std::path::Path) {
    let kept = barred::cached(&store.root);
    let shown = kept
        .as_ref()
        .map(|c| barred::for_store(store, profile, &c.addons))
        .unwrap_or_default();
    hand_over(game, &shown);
    if kept.is_some_and(|c| c.fresh(unix_now())) {
        return;
    }
    let (s, p, g) = (store.clone(), profile.to_owned(), game.to_owned());
    std::thread::spawn(move || {
        let list = match barred::refresh(&s.root, &swofty_core::catalog::base_url(), unix_now()) {
            Ok(list) => list,
            Err(e) => {
                say("laps", &format!("barred addons list: {e}"));
                return;
            }
        };
        let next = barred::for_store(&s, &p, &list);
        if next != shown {
            hand_over(&g, &next);
        }
    });
}

fn hand_over(game: &std::path::Path, addons: &[Barred]) {
    if let Err(e) = barred::write_session(game, addons) {
        say(
            "laps",
            &format!("barred addons: the session file is not writable: {e}"),
        );
        return;
    }
    if let Some(line) = barred::warning(&addons.iter().collect::<Vec<_>>()) {
        say("laps", &line);
    }
}

fn send(store: &Store, profile: &str, file: Option<PathBuf>) -> Result<()> {
    let Some(token) = token(&store.root, &store.config()) else {
        bail!(
            "Time Trial boards is off or you are not signed in: sign in with Discord in the launcher and turn Time Trial boards on in Settings"
        );
    };
    let api = Api::new(&token);
    let print = crate::commands::build::game(store, None)
        .ok()
        .and_then(|g| {
            let played = Played {
                store: store.clone(),
                profile: profile.to_owned(),
                game_build: g.identity,
            };
            played.fingerprint().map(|f| fingerprint::wire(&f))
        })
        .unwrap_or(Value::Null);
    let queue = Queue::new(&store.root);
    match file {
        Some(file) => {
            let a = queue.send_one(&file, &api as &dyn Boards, &print, unix_now())?;
            let rank = a.rank.map_or(String::new(), |r| format!(" -> P{r}"));
            println!("uploaded{rank} ({})", a.status);
            if let Some(url) = a.url {
                println!("{url}");
            }
            for r in a.reasons {
                println!("  {r}");
            }
        }
        None => {
            let report = queue.drain(
                &api,
                &print,
                Limits {
                    ignore_backoff: true,
                    ..Limits::at(unix_now())
                },
                &mut |e| println!("{}", upload::line(&e)),
            );
            println!(
                "{} uploaded, {} still waiting, {} not sent",
                report.uploaded, report.waiting, report.rejected
            );
            if let Ok(game) = crate::commands::build::game(store, None) {
                let dirs = timetrial::game_replay::dirs(&store.root, &game.dir);
                timetrial::game_replay::send(&queue, &dirs, &api, unix_now(), &|| false, &mut |l| {
                    println!("{l}")
                });
            }
        }
    }
    Ok(())
}

pub fn begin(
    store: &Store,
    config: &swofty_core::session::Config,
    profile: &str,
    g: &Game,
) -> Option<Watcher> {
    barred_for_launch(store, profile, &g.dir);
    let Some(_) = token(&store.root, config) else {
        timetrial::clear_session_keys(&g.dir);
        return None;
    };
    let played = Played {
        store: store.clone(),
        profile: profile.to_owned(),
        game_build: g.identity.clone(),
    };
    let prints = Session::start(played, |line| say("laps", line));
    let build = Some(g.identity.exe_sha256.to_ascii_lowercase()).filter(|b| !b.is_empty());
    let nonce = match upload::nonce(&store.root, config, build.as_deref()) {
        Ok(n) => Some(n),
        Err(e) => {
            say(
                "laps",
                &format!("session nonce: {e}; the watcher asks again while the game runs"),
            );
            None
        }
    };
    if let Err(e) = timetrial::write_session_keys(&g.dir, nonce.as_ref(), config.time_trial_ingame) {
        say("laps", &format!("lap recording is off: {e}"));
        return None;
    }
    let integrity = session_integrity()
        .and_then(|state| serde_json::to_value(state).ok())
        .unwrap_or(Value::Null);
    Some(Watcher::start(
        &store.root,
        &g.dir,
        config,
        nonce.is_none(),
        build,
        integrity,
        prints,
        |line| say("laps", line),
    ))
}

/// What a session asked for of the integrity module: what the fetch came to, and whether it was the local build.
pub type Asked = (Result<Module, integrity::Unavailable>, bool);

/// The verified integrity module for a session when Time Trial boards is on and the player is signed in, or
/// `None` when the session does not ask for it. Any trouble is one log line and the session goes on without it.
pub fn integrity(store: &Store, out: &dyn Fn(&str)) -> Option<Asked> {
    let token = token(&store.root, &store.config())?;
    let asked = match integrity::local() {
        Some(local) => {
            out(&format!(
                "integrity module: {} is set; the local build is placed unsigned and the server's module is not used",
                integrity::LOCAL_ENV
            ));
            (local, true)
        }
        None => (integrity::ensure(&store.root, &Api::new(&token), out), false),
    };
    if let Err(why) = &asked.0 {
        out(&format!("integrity module: not loaded for this session: {why}"));
    }
    Some(asked)
}

static SESSION_INTEGRITY: std::sync::Mutex<Option<integrity::State>> = std::sync::Mutex::new(None);

/// Keep what this session did about the integrity module: as the note the launcher shows at Play when none was
/// placed, and for the laps the session records. `core` is whether the build runs the core; one without it
/// records no laps, so it asked for nothing.
pub fn note_integrity(store: &Store, asked: Option<&Asked>, core: bool, out: &dyn Fn(&str)) {
    let asked = asked.filter(|_| core);
    let state = asked.map(|(found, local)| integrity::State::of(found, *local));
    let missing = asked.and_then(|(found, _)| found.as_ref().err());
    if let Err(e) = integrity::Notice::keep(&store.root, missing) {
        out(&format!(
            "integrity module: could not keep the note for the launcher: {e}"
        ));
    }
    *SESSION_INTEGRITY.lock().unwrap_or_else(|e| e.into_inner()) = state;
}

fn session_integrity() -> Option<integrity::State> {
    SESSION_INTEGRITY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn end(watcher: Option<Watcher>, game: &std::path::Path) {
    if let Some(w) = watcher {
        w.finish();
        timetrial::clear_session_keys(game);
    }
    barred::write_session(game, &[]).ok();
}
