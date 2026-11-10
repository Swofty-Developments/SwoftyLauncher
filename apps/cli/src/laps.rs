use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::Value;
use swofty_core::baseline::Identity;
use swofty_core::install::Game;
use swofty_core::store::Store;
use swofty_core::timetrial::api::{Api, Boards};
use swofty_core::timetrial::queue::{Entry, Folder, Limits, Queue, unix_now};
use swofty_core::timetrial::upload::{self, Watcher};
use swofty_core::timetrial::{self, token};

use crate::run::say;

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
}

pub fn command(store: &Store, profile: &str, cmd: LapsCommand) -> Result<ExitCode> {
    match cmd {
        LapsCommand::List => list(store),
        LapsCommand::Upload { file } => send(store, profile, file)?,
        LapsCommand::Open => println!("{}", timetrial::boards_url()),
    }
    Ok(ExitCode::SUCCESS)
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

fn list(store: &Store) {
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
    if token(&store.root, &store.config()).is_none() {
        println!(
            "Time Trial boards uploads are off: sign in with Discord and leave Time Trial boards on in Settings"
        );
    }
}

fn fingerprint(store: &Store, profile: &str, identity: &Identity) -> Option<Value> {
    let p = store.profile(profile).ok()?;
    let fp = swofty_core::multiplayer::fingerprint::fingerprint(store, &p, identity).ok()?;
    serde_json::to_value(fp).ok()
}

fn send(store: &Store, profile: &str, file: Option<PathBuf>) -> Result<()> {
    let Some(token) = token(&store.root, &store.config()) else {
        bail!(
            "Time Trial boards is off or you are not signed in: sign in with Discord in the launcher and turn Time Trial boards on in Settings"
        );
    };
    let api = Api::new(&token);
    let print = crate::build::game(store, None)
        .ok()
        .and_then(|g| fingerprint(store, profile, &g.identity))
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
    let Some(_) = token(&store.root, config) else {
        timetrial::clear_session_keys(&g.dir);
        return None;
    };
    let nonce = upload::nonce(&store.root, config).map(|n| n.nonce);
    if let Err(e) = timetrial::write_session_keys(&g.dir, nonce.as_deref()) {
        say("laps", &format!("lap recording is off: {e}"));
        return None;
    }
    let (s, p, identity) = (store.clone(), profile.to_owned(), g.identity.clone());
    Some(Watcher::start(
        &store.root,
        &g.dir,
        config,
        move || fingerprint(&s, &p, &identity),
        |line| say("laps", line),
    ))
}

pub fn end(watcher: Option<Watcher>, game: &std::path::Path) {
    if let Some(w) = watcher {
        w.finish();
        timetrial::clear_session_keys(game);
    }
}
