use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use anyhow::{Result, bail};
use clap::Subcommand;
use swofty_core::install::Game;
use swofty_core::save_backup;
use swofty_core::store::Store;
use swofty_core::vault::restore::{self, Running};
use swofty_core::vault::{
    self, Checked, Location, Meta, Notice, Reason, Snapshot, Taken, Vault, migrate, watch,
};

use crate::run::say;

#[derive(Subcommand)]
pub enum SavesCommand {
    #[command(
        about = "List the career vault's snapshots of the profile save, newest first, with the cloud status"
    )]
    List {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "Show one snapshot: its files with their hashes, Steam's record of them, and its events"
    )]
    Show {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Snapshot the profile save now and pin it, for example before playing from Steam")]
    Backup,
    #[command(
        about = "Copy a snapshot back over the profile save (F1 25 and Steam, or the EA app, must be closed); snapshots the current save first"
    )]
    Restore {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "List the versions of the profile save with the careers each holds, its cloud status, and what restoring it gets back and loses"
    )]
    Versions {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "List the careers the vault has seen, with their status and the addons they need")]
    Careers {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Keep a snapshot: retention never removes it")]
    Pin {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Undo `pin`; reasons other than your own pin keep the snapshot")]
    Unpin {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Show or set the career vault's size cap in MB (500 by default)")]
    Cap { mb: Option<u64> },
}

const PINNED: &str = "pinned";

const SOURCE_HELP: &str =
    "The vault to use, steam-<id> or ea-<user> (default: the save of the copy Play uses)";

/// The snapshot a session's after-session check compares with.
pub struct Before {
    vault: Vault,
    loc: Location,
    snapshot: Option<Snapshot>,
    meta: Meta,
}

fn report(lines: Vec<String>) {
    for l in lines {
        say("info", &l);
    }
}

/// Snapshot the profile save before a session, after moving in any old
/// `save-backups/`.
pub fn before_session(store: &Store, profile: &str, g: &Game, session: &str) -> Option<Before> {
    match migrate::migrate(&store.root) {
        Ok(notes) => report(notes),
        Err(e) => say(
            "warning",
            &format!("saves: could not move save-backups into the career vault: {e}"),
        ),
    }
    let Some(loc) = Location::for_game(g) else {
        say(
            "info",
            "saves: no profile save folder found for this copy of F1 25; nothing to snapshot",
        );
        return None;
    };
    let p = store.profile(profile);
    let absent = match p
        .as_ref()
        .map_err(|e| e.to_string())
        .and_then(|p| save_backup::absent_in(store, p).map_err(|e| e.to_string()))
    {
        Ok(a) => a,
        Err(e) => {
            say(
                "warning",
                &format!("saves: could not check the profile's addons: {e}"),
            );
            Vec::new()
        }
    };
    let mut meta = Meta::new(Reason::BeforeSession).profile(profile);
    meta.session_id = (!session.is_empty()).then(|| session.to_owned());
    meta.absent = absent.clone();
    meta.exe_sha256 = Some(g.identity.exe_sha256.clone());
    if p.is_ok_and(|p| p.is_vanilla()) && !absent.is_empty() {
        meta = meta.pin("vanilla-session");
    }
    let vault = match Vault::of(&store.root, &loc) {
        Ok(v) => v,
        Err(e) => {
            say("warning", &format!("saves: {e}"));
            return None;
        }
    };
    let snapshot = match vault.before_session(&loc, meta.clone(), vault::now()) {
        Ok(c) => {
            if let Some(n) = &c.notice {
                say("warning", &format!("saves: {}", n.text()));
            }
            let snapshot = c.taken.snapshot().cloned();
            report(watch::describe(&Checked { notice: None, ..c }));
            snapshot
        }
        Err(e) => {
            say(
                "warning",
                &format!("saves: could not snapshot the profile save: {e}"),
            );
            None
        }
    };
    if let (Some(s), false) = (&snapshot, absent.is_empty()) {
        say(
            "info",
            &format!(
                "saves: this session runs without {}; snapshot {} holds the save from before it",
                absent.join(", "),
                s.id
            ),
        );
    }
    Some(Before {
        vault,
        loc,
        snapshot,
        meta,
    })
}

/// Snapshot the profile save once the game exited and the session's game
/// files are rolled back.
pub fn after_session(before: Option<Before>) {
    let Some(b) = before else {
        return;
    };
    let mut meta = b.meta;
    meta.reason = Reason::AfterSession;
    meta.pin = None;
    match b
        .vault
        .after_session(&b.loc, meta, b.snapshot.as_ref(), vault::now())
    {
        Ok(c) => {
            if let Some(n) = &c.notice {
                say("warning", &format!("saves: {}", n.text()));
            }
            report(watch::describe(&Checked { notice: None, ..c }));
        }
        Err(e) => say(
            "warning",
            &format!("saves: could not snapshot the profile save after the session: {e}"),
        ),
    }
}

/// The vault notice a session that started at `since` raised.
pub fn notice_since(store: &Store, since: f64) -> Option<String> {
    let at = std::fs::metadata(Notice::path(&store.root))
        .and_then(|m| m.modified())
        .ok()?;
    let since = SystemTime::UNIX_EPOCH + Duration::from_secs_f64(since.max(0.0));
    if at < since {
        return None;
    }
    Notice::read(&store.root).map(|n| n.text())
}

fn open(store: &Store, source: Option<&str>) -> Result<(Vault, Option<Location>)> {
    let active = Location::active(&store.root);
    match source {
        Some(s) => Ok((Vault::open(&store.root, s)?, active.filter(|l| l.source == s))),
        None => match active {
            Some(l) => Ok((Vault::of(&store.root, &l)?, Some(l))),
            None => match Vault::sources(&store.root).as_slice() {
                [only] => Ok((Vault::open(&store.root, only)?, None)),
                [] => bail!(
                    "no F1 25 profile save found and the career vault in {} is empty",
                    Vault::root(&store.root).display()
                ),
                many => bail!(
                    "no F1 25 profile save found; pass --source with one of {}",
                    many.join(", ")
                ),
            },
        },
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn row(s: &Snapshot) -> String {
    let mut out = format!("{}  {}", s.id, s.reason.text());
    if let Some(p) = &s.profile {
        out.push_str(&format!(", profile {p}"));
    }
    if !s.absent_addons.is_empty() {
        out.push_str(&format!(", without {}", s.absent_addons.join(", ")));
    }
    out.push_str(&format!(", {} file(s), {} bytes", s.files.len(), s.total()));
    if !s.pinned.is_empty() {
        out.push_str(&format!(", pinned ({})", s.pinned.join(", ")));
    }
    out
}

fn list(store: &Store, source: Option<&str>) -> Result<()> {
    if let Err(e) = migrate::migrate(&store.root).map(report) {
        say("warning", &format!("saves: {e}"));
    }
    let (v, loc) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no snapshots: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let snaps = v.list()?;
    println!(
        "vault {}: {} snapshot(s), {} of {}",
        v.dir.display(),
        snaps.len(),
        mb(v.usage()),
        mb(v.policy.cap_bytes)
    );
    if let Some(l) = &loc {
        match vault::save_status(l) {
            Ok(st) if st.files > 0 => println!(
                "save {}: {} file(s), {} bytes, Steam Cloud: {}",
                l.dir.display(),
                st.files,
                st.total,
                st.cloud.text()
            ),
            Ok(_) => println!("save {}: no profile save yet", l.dir.display()),
            Err(e) => println!("save {}: {e}", l.dir.display()),
        }
    }
    for s in snaps {
        println!("{}", row(&s));
    }
    Ok(())
}

fn show(store: &Store, id: &str, source: Option<&str>) -> Result<()> {
    let (v, _) = open(store, source)?;
    let s = v.find(id)?;
    println!("{}", row(&s));
    println!("taken {} by launcher {}", vault::human(s.at), s.launcher);
    println!("save folder {}", s.save_dir.display());
    if let Some(n) = &s.note {
        println!("note: {n}");
    }
    if let Some(sid) = &s.session_id {
        println!("session {sid}");
    }
    if let Some(exe) = &s.exe_sha256 {
        println!("F1_25.exe sha256 {exe}");
    }
    println!("Steam Cloud when taken: {}", s.cloud().text());
    for f in &s.files {
        println!(
            "  {} ({}): {} bytes, sha256 {}, sha1 {}",
            f.name, f.file, f.size, f.sha256, f.sha1
        );
        if let Some(r) = &f.remotecache {
            println!(
                "    remotecache: size {}, sha {}, syncstate {}",
                r.size, r.sha, r.syncstate
            );
        }
    }
    for e in v
        .events()
        .iter()
        .filter(|e| e.snapshot.as_deref() == Some(id) || e.from.as_deref() == Some(id))
    {
        println!(
            "  event {} {}{}",
            vault::human(e.at),
            serde_json::to_value(e.kind)
                .ok()
                .and_then(|k| k.as_str().map(str::to_owned))
                .unwrap_or_default(),
            e.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default()
        );
    }
    Ok(())
}

fn names(list: &[vault::careers::Brief]) -> String {
    list.iter()
        .map(|b| match &b.place {
            Some(p) => format!("{} ({p})", b.name),
            None => b.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn versions(store: &Store, source: Option<&str>) -> Result<()> {
    if let Err(e) = migrate::migrate(&store.root).map(report) {
        say("warning", &format!("saves: {e}"));
    }
    let (v, loc) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no versions: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let snaps = v.list()?;
    println!(
        "{} version(s) in {}, {} of {}",
        snaps.len(),
        v.dir.display(),
        mb(v.usage()),
        mb(v.policy.cap_bytes)
    );
    if let Some(st) = loc.as_ref().and_then(|l| vault::save_status(l).ok()) {
        println!("Steam Cloud now: {}", st.cloud.text());
    }
    for s in snaps {
        let held = v.index_of(&s).map(|i| {
            let held: Vec<String> = vault::careers::in_save(&i)
                .map(|c| match c.label.is_empty() {
                    true => c.career.clone(),
                    false => c.label.clone(),
                })
                .collect();
            match held.len() {
                0 => "no careers".to_owned(),
                n => format!("{n} career(s): {}", held.join(", ")),
            }
        });
        println!(
            "{}  {}, {}, {}, Steam Cloud: {}, {}{}",
            s.id,
            s.reason.text(),
            match &s.profile {
                Some(p) => format!("profile {p}"),
                None => "no profile".to_owned(),
            },
            mb(s.total()),
            s.cloud().text(),
            held.as_deref().unwrap_or("careers unknown"),
            match s.pinned.is_empty() {
                true => String::new(),
                false => format!(", pinned ({})", s.pinned.join(", ")),
            }
        );
        if let Some(d) = v.preview(&s).filter(|d| !d.is_empty()) {
            if !d.get_back.is_empty() {
                println!("    restoring it gets back: {}", names(&d.get_back));
            }
            if !d.lose.is_empty() {
                println!("    restoring it loses: {}", names(&d.lose));
            }
            for (now, then) in &d.progress {
                println!(
                    "    restoring it loses progress in {}: {} back to {}",
                    now.name,
                    now.place.as_deref().unwrap_or("now"),
                    then.place.as_deref().unwrap_or("then")
                );
            }
        }
    }
    Ok(())
}

fn careers(store: &Store, source: Option<&str>) -> Result<()> {
    let (v, _) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no careers: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let table = v.careers();
    if table.careers.is_empty() {
        println!(
            "no careers yet: the vault learns them from the career index F1 25 writes in a launcher session"
        );
        return Ok(());
    }
    for r in &table.careers {
        let mut line = format!("{}  {}", r.career, r.name());
        if let Some(m) = &r.mode {
            line.push_str(&format!(", {m}"));
        }
        if let Some(d) = &r.driver {
            line.push_str(&format!(", {d}"));
        }
        if let Some(t) = &r.team {
            line.push_str(&format!(", {t}"));
        }
        if let Some(p) = r.place() {
            line.push_str(&format!(", {p}"));
        }
        if let Some(at) = r.last_played {
            line.push_str(&format!(", last played {}", vault::human(at)));
        }
        let needs: Vec<&str> = r.needs().collect();
        if !needs.is_empty() {
            line.push_str(&format!(", needs {}", needs.join(" ")));
        }
        line.push_str(&format!(", {}", r.status.slug()));
        println!("{line}");
        if r.status.is_missing() {
            println!("    last in the save at snapshot {}", r.last_seen);
        }
    }
    Ok(())
}

fn pin_cmd(store: &Store, id: &str, source: Option<&str>, pin: bool) -> Result<()> {
    let (v, _) = open(store, source)?;
    let s = match pin {
        true => v.pin(id, PINNED, vault::now())?,
        false => v.unpin_why(id, PINNED, vault::now())?,
    };
    match (pin, s.pinned.is_empty()) {
        (true, _) => println!("pinned snapshot {}", s.id),
        (false, true) => println!("unpinned snapshot {}", s.id),
        (false, false) => println!(
            "unpinned snapshot {}; it stays kept for {}",
            s.id,
            s.pinned.join(", ")
        ),
    }
    Ok(())
}

fn backup(store: &Store) -> Result<()> {
    migrate::migrate(&store.root).map(report).ok();
    let Some(loc) = Location::active(&store.root) else {
        bail!("no F1 25 profile save found to back up");
    };
    let v = Vault::of(&store.root, &loc)?;
    let profile = store.active_profile();
    let meta = Meta::new(Reason::Manual).profile(profile).pin("backup");
    match v.check(&loc, meta, vault::now())?.taken {
        Taken::New(s) => println!("saved and pinned snapshot {} ({} bytes)", s.id, s.total()),
        Taken::Unchanged(s) => println!("the profile save is unchanged since snapshot {}; pinned it", s.id),
        Taken::NoSave => bail!("no profile save in {} yet", loc.dir.display()),
    }
    Ok(())
}

fn restore_cmd(store: &Store, id: &str, source: Option<&str>) -> Result<()> {
    let (v, loc) = open(store, source)?;
    let snap = v.find(id)?;
    let loc = match loc {
        Some(l) => l,
        None => Location {
            source: snap.source.clone(),
            layout: snap.layout,
            dir: snap.save_dir.clone(),
        },
    };
    let r = restore::restore(&v, id, &loc, Running::now(), vault::now())?;
    say("info", &r.log_line());
    if let Some(b) = &r.before {
        println!("the save it replaced is snapshot {b}");
    }
    println!("restored {id} into {}", r.dir.display());
    if !r.kept.is_empty() {
        println!("left as they are (not in the snapshot): {}", r.kept.join(", "));
    }
    if loc.layout == vault::Layout::Steam {
        println!(
            "start Steam to upload the restored save; if it asks which copy to keep, keep the local files (this computer)"
        );
    }
    Ok(())
}

pub fn command(store: &Store, cmd: SavesCommand) -> Result<ExitCode> {
    match cmd {
        SavesCommand::List { source } => list(store, source.as_deref())?,
        SavesCommand::Show { snapshot, source } => show(store, &snapshot, source.as_deref())?,
        SavesCommand::Versions { source } => versions(store, source.as_deref())?,
        SavesCommand::Careers { source } => careers(store, source.as_deref())?,
        SavesCommand::Pin { snapshot, source } => pin_cmd(store, &snapshot, source.as_deref(), true)?,
        SavesCommand::Unpin { snapshot, source } => pin_cmd(store, &snapshot, source.as_deref(), false)?,
        SavesCommand::Backup => backup(store)?,
        SavesCommand::Restore { snapshot, source } => restore_cmd(store, &snapshot, source.as_deref())?,
        SavesCommand::Cap { mb: None } => println!(
            "{} MB",
            store.config().vault_cap_mb.unwrap_or(vault::DEFAULT_CAP_MB)
        ),
        SavesCommand::Cap { mb: Some(n) } => {
            if n == 0 {
                bail!("the cap must be at least 1 MB");
            }
            let mut c = store.config();
            c.vault_cap_mb = Some(n);
            store.save_config(&c)?;
            println!("the career vault keeps at most {n} MB; the next snapshot prunes to it");
        }
    }
    Ok(ExitCode::SUCCESS)
}
