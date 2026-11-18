use std::time::{Duration, SystemTime};

use swofty_core::install::Game;
use swofty_core::saves::backup as save_backup;
use swofty_core::saves::launcher as launcher_saves;
use swofty_core::saves::vault::restore::Running;
use swofty_core::saves::vault::{
    self, Checked, Location, Meta, Notice, Reason, Snapshot, Vault, migrate, watch,
};
use swofty_core::store::Store;

use crate::console::output::say;

/// The snapshot a session's after-session check compares with.
pub struct Before {
    pub(super) vault: Vault,
    pub(super) loc: Location,
    pub(super) snapshot: Option<Snapshot>,
    pub(super) meta: Meta,
    pub(super) steam: Option<std::path::PathBuf>,
}

pub(super) fn report(lines: Vec<String>) {
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
    build_blocked: Option<&str>,
    steam_saves_once: bool,
) -> Option<Before> {
    before_session_at(
        store,
        profile,
        g,
        session,
        build_blocked,
        steam_saves_once,
        Location::for_game(g),
    )
}

pub(super) fn before_session_at(
    store: &Store,
    profile: &str,
    g: &Game,
    session: &str,
    build_blocked: Option<&str>,
    steam_saves_once: bool,
    game_loc: Option<Location>,
) -> Option<Before> {
    match migrate::migrate(&store.root) {
        Ok(notes) => report(notes),
        Err(e) => say(
            "warning",
            &format!("saves: could not move save-backups into the career vault: {e}"),
        ),
    }
    for line in swofty_core::saves::stall::sweep(store, game_loc.as_ref(), Running::now(), vault::now()) {
        say("warning", &line);
    }
    let p = store.profile(profile);
    let launcher = game_loc.as_ref().and_then(|l| {
        let area = launcher_saves::area(launcher_saves::Choice {
            build_blocked,
            vanilla: p.as_ref().map_or(true, |p| p.is_vanilla()),
            profile_uses_launcher_saves: p.as_ref().is_ok_and(|p| p.launcher_saves),
            steam_saves_once,
            layout: l.layout,
        });
        match area {
            launcher_saves::Area::Launcher => launcher_saves::store_for(&store.root, profile, l),
            launcher_saves::Area::Own(launcher_saves::Why::EaCopy) => {
                say("info", &format!("saves: {}", launcher_saves::EA_NOTE));
                None
            }
            launcher_saves::Area::Own(_) => None,
        }
    });
    if let (Some(store_loc), Some(from)) = (&launcher, &game_loc) {
        seed_if_empty(store, profile, from, store_loc);
    }
    let steam = game_loc
        .as_ref()
        .filter(|l| l.layout.named_like_steam() && !l.layout.is_launcher())
        .map(|l| l.dir.clone());
    if let Some(dir) = &steam {
        say(
            "info",
            &format!(
                "saves: Steam remote folder at session start, {}",
                vault::folder_listing(dir)
            ),
        );
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
        steam,
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
    if let Some(dir) = &b.steam {
        say(
            "info",
            &format!(
                "saves: Steam remote folder at session end, {}",
                vault::folder_listing(dir)
            ),
        );
    }
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
