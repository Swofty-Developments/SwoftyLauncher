use std::time::{Duration, SystemTime};

use swofty_core::install::Game;
use swofty_core::saves::backup as save_backup;
use swofty_core::saves::launcher as launcher_saves;
use swofty_core::saves::vault::restore::Running;
use swofty_core::saves::vault::{self, Checked, Location, Meta, Notice, Reason, Vault, migrate, watch};
use swofty_core::store::Store;

use crate::console::output::say;

/// What a session's after-session snapshot needs from its start.
pub struct Before {
    pub(super) vault: Vault,
    pub(super) loc: Location,
    pub(super) meta: Meta,
    pub(super) steam: Option<std::path::PathBuf>,
}

fn map_moved_ids(store: &Store, profile: &str, g: &Game, loc: &Location, snapshot: Option<&str>) {
    let file = g.dir.join("swofty").join(swofty_core::row_ids::ALIASES_FILE);
    let Ok(bytes) = std::fs::read(&file) else {
        return;
    };
    let Ok(aliases) = serde_json::from_slice::<swofty_core::row_ids::IdAliases>(&bytes) else {
        return;
    };
    let on_launcher_saves = loc.layout.is_launcher();
    let path = loc.dir.join(swofty_core::saves::plain::rewrite::SAVE_FILE);
    let save = (loc.layout == swofty_core::saves::vault::Layout::Launcher).then_some(
        swofty_core::saves::plain::rewrite::SaveFile {
            path: &path,
            snapshot,
        },
    );
    let now = vault::now();
    for line in swofty_core::saves::plain::rewrite::prepare_play(
        &store.root,
        profile,
        on_launcher_saves,
        swofty_core::install::Source::detect(&g.dir),
        &aliases,
        now,
        save,
    ) {
        say("info", &line);
    }
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
    split_plain_stores(store);
    withdraw_unanswered_restores(store);
    if let Some(l) = game_loc.as_ref().filter(|l| !l.layout.is_launcher()) {
        swofty_core::saves::plain::remember_copy_source(&store.root, &l.source);
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
    if p.as_ref().is_ok_and(|p| p.is_vanilla()) && !absent.is_empty() {
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
    if p.as_ref().is_ok_and(|p| !p.is_vanilla()) {
        map_moved_ids(store, profile, g, &loc, snapshot.as_ref().map(|s| s.id.as_str()));
    }
    Some(Before {
        vault,
        loc,
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

/// Split each profile's shared plain store between the Steam and EA app copies, once per profile. Skipped while
/// F1 25 runs.
pub fn split_plain_stores(store: &Store) {
    if Running::now().game {
        return;
    }
    for line in swofty_core::saves::plain::split_all_held(&store.root) {
        say("info", &line);
    }
}

/// Withdraw the restore requests of a profile load that stopped before it
/// answered them (the game closed or crashed while it applied them), so the
/// next Play does not apply them again. Skipped while F1 25 runs: the game may
/// still answer them.
pub fn withdraw_unanswered_restores(store: &Store) {
    if Running::now().game {
        return;
    }
    for line in swofty_core::saves::plain::withdraw_all_unanswered(&store.root) {
        say("warning", &line);
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
    match b.vault.after_session(&b.loc, meta, vault::now()) {
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

/// Learn the saves a session ended stuck in a started race weekend on, once the session's end snapshot is in
/// the vault.
pub fn learn_stuck_weekends(store: &Store) {
    for line in swofty_core::saves::stuck::sweep(store, vault::now()) {
        say("warning", &line);
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
