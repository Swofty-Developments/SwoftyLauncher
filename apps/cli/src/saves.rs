use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use anyhow::{Result, bail};
use clap::Subcommand;
use swofty_core::install::Game;
use swofty_core::launcher_saves;
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
    #[command(about = "Show or set the career vault's size cap in MB (500 by default)")]
    Cap { mb: Option<u64> },
    #[command(
        about = "Copy a Steam save, or another profile's launcher saves, into a profile's launcher saves (F1 25 must be closed); keeps what they held in the vault first"
    )]
    Import {
        #[arg(
            long,
            value_name = "steam",
            conflicts_with = "from_profile",
            help = "Import the Steam save (the default)"
        )]
        from: Option<String>,
        #[arg(
            long,
            value_name = "PROFILE",
            help = "Import another profile's launcher saves"
        )]
        from_profile: Option<String>,
        #[arg(long, help = "The profile to import into (default: the active profile)")]
        profile: Option<String>,
    },
    #[command(
        about = "Copy a profile's launcher saves over the Steam save (F1 25 and Steam must be closed); keeps the Steam save in the vault first"
    )]
    Export {
        #[arg(long, value_name = "steam", help = "Where to copy to; only steam")]
        to: String,
        #[arg(long, help = "The profile to export (default: the active profile)")]
        profile: Option<String>,
    },
}

const SOURCE_HELP: &str =
    "The vault to use, steam-<id>, ea-<user> or launcher-<profile> (default: the save of the copy Play uses)";

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
pub fn before_session(
    store: &Store,
    profile: &str,
    g: &Game,
    session: &str,
    launcher_saves_allowed: bool,
) -> Option<Before> {
    match migrate::migrate(&store.root) {
        Ok(notes) => report(notes),
        Err(e) => say(
            "warning",
            &format!("saves: could not move save-backups into the career vault: {e}"),
        ),
    }
    let game_loc = Location::for_game(g);
    let p = store.profile(profile);
    let launcher = match (&game_loc, &p) {
        (Some(l), Ok(p)) if !p.is_vanilla() && p.launcher_saves => {
            match launcher_saves::supported(l.layout) {
                true => launcher_saves::store_for(&store.root, profile, l).filter(|_| launcher_saves_allowed),
                false => {
                    say("info", &format!("saves: {}", launcher_saves::EA_NOTE));
                    None
                }
            }
        }
        _ => None,
    };
    if let (Some(store_loc), Some(from)) = (&launcher, &game_loc) {
        seed_if_empty(store, profile, from, store_loc);
    }
    let Some(loc) = launcher.or(game_loc) else {
        say(
            "info",
            "saves: no profile save folder found for this copy of F1 25; nothing to snapshot",
        );
        return None;
    };
    if loc.layout.is_launcher() {
        say(
            "info",
            &format!(
                "saves: this session plays on the launcher saves of profile {profile} ({}); Steam's saves are not touched",
                loc.dir.display()
            ),
        );
    }
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

fn seed_if_empty(store: &Store, profile: &str, from: &Location, to: &Location) {
    if launcher_saves::has_files(to) {
        return;
    }
    match launcher_saves::seed(&store.root, profile, from, Reason::Imported, vault::now()) {
        Ok(s) => say("info", &s.line(profile, from, to)),
        Err(e) => say(
            "warning",
            &format!(
                "saves: could not copy the Steam save into the launcher saves of profile {profile}: {e}"
            ),
        ),
    }
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
        Some(s) => Ok((
            Vault::open(&store.root, s)?,
            active
                .filter(|l| l.source == s)
                .or_else(|| Location::of_source(&store.root, s)),
        )),
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
    if loc.layout.is_launcher() {
        println!("the launcher saves are restored; Steam's saves are not touched");
    }
    if loc.layout == vault::Layout::Steam {
        println!(
            "start Steam to upload the restored save; if it asks which copy to keep, keep the local files (this computer)"
        );
    }
    Ok(())
}

fn profile_arg(store: &Store, profile: Option<String>) -> Result<String> {
    let p = profile.unwrap_or_else(|| store.active_profile());
    let loaded = store.profile(&p)?;
    if loaded.is_vanilla() {
        bail!("{p} plays without the Swofty core and always uses Steam's saves");
    }
    Ok(p)
}

fn import_cmd(
    store: &Store,
    from: Option<String>,
    from_profile: Option<String>,
    profile: Option<String>,
) -> Result<()> {
    let profile = profile_arg(store, profile)?;
    let (src, reason) = match (from.as_deref(), from_profile) {
        (Some(other), _) if other != "steam" => {
            bail!("--from takes steam; use --from-profile for another profile")
        }
        (_, Some(p)) => {
            if p == profile {
                bail!("{p} is the profile to import into");
            }
            store.profile(&p)?;
            (launcher_saves::location(&store.root, &p), Reason::ImportedProfile)
        }
        _ => match Location::active(&store.root) {
            Some(l) if l.layout == vault::Layout::Steam => (l, Reason::Imported),
            Some(_) => bail!("{}", launcher_saves::EA_NOTE),
            None => bail!("no Steam save of F1 25 found to import"),
        },
    };
    let (files, bytes, snapshot) =
        launcher_saves::import(&store.root, &profile, &src, reason, Running::now(), vault::now())?;
    say(
        "info",
        &format!(
            "saves: imported {files} file(s) from {} into the launcher saves of profile {profile}",
            src.dir.display()
        ),
    );
    println!(
        "imported {files} file(s), {bytes} bytes, from {} into {}",
        src.dir.display(),
        launcher_saves::remote_dir(&store.root, &profile).display()
    );
    if let Some(s) = snapshot {
        println!("the vault holds it as snapshot {s}");
    }
    Ok(())
}

fn export_cmd(store: &Store, to: &str, profile: Option<String>) -> Result<()> {
    if to != "steam" {
        bail!("--to takes steam");
    }
    let profile = profile_arg(store, profile)?;
    let steam = match Location::active(&store.root) {
        Some(l) if l.layout == vault::Layout::Steam => l,
        Some(_) => bail!("{}", launcher_saves::EA_NOTE),
        None => bail!("no Steam save folder of F1 25 found to export to"),
    };
    let r = launcher_saves::export(&store.root, &profile, &steam, Running::now(), vault::now())?;
    say("info", &r.log_line());
    if let Some(b) = &r.before {
        println!("the Steam save it replaced is snapshot {b}");
    }
    println!(
        "exported the launcher saves of profile {profile} into {}",
        r.dir.display()
    );
    println!(
        "start Steam to upload the exported save; if it asks which copy to keep, keep the local files (this computer)"
    );
    Ok(())
}

pub fn command(store: &Store, cmd: SavesCommand) -> Result<ExitCode> {
    match cmd {
        SavesCommand::Import {
            from,
            from_profile,
            profile,
        } => import_cmd(store, from, from_profile, profile)?,
        SavesCommand::Export { to, profile } => export_cmd(store, &to, profile)?,
        SavesCommand::List { source } => list(store, source.as_deref())?,
        SavesCommand::Show { snapshot, source } => show(store, &snapshot, source.as_deref())?,
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
