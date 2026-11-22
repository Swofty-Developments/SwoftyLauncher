mod copies;
pub(super) mod loader;
mod platform;
pub(super) mod play;

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use swofty_core::GAME_EXE;
use swofty_core::diagnostics::launch;
use swofty_core::diagnostics::quarantine;
use swofty_core::game_folder::recovery::{Scope, SessionLock};
use swofty_core::install::{Game, Installer};
use swofty_core::platform::process::{game_processes, games_in};
use swofty_core::session::{Config, LaunchRequest, parse_cpu_list};
use swofty_core::store::Store;

use crate::commands::run::RunArgs;
use crate::commands::run::failure::already_running;
use crate::console::output::{info, say};
use crate::signals::{end_on_signals, ending};
use platform::{become_subreaper, set_affinity};
use play::play;

pub(crate) fn run(store: &Store, profile: &str, args: &RunArgs, g: &Game) -> Result<ExitCode> {
    if let Some(running) = game_processes().first() {
        return Err(already_running(
            store,
            running.pid,
            "close it before launching it again",
        ));
    }
    let lock = crate::session_lock::lock(store, &info)?;
    crate::session_lock::stale(store, &lock, g, Scope::All, &info)?;
    crate::commands::offline::gate(store, false)?;
    swofty_core::game_folder::entitlement::check_store_start(&g.dir, args.via_ea)?;
    crate::commands::build::check_entitled(store, g)?;
    let version = swofty_core::game_folder::exe_version::read(&g.dir.join(GAME_EXE));
    if let Some(d) = swofty_core::diagnostics::known_problems::outdated_exe(version.as_deref()) {
        say("warning", &format!("{}: {}", d.title, d.text()));
    }
    // An EA app copy's session was checked before the EA app started: the EA
    // app, not this process, starts its game.
    if !args.via_ea
        && let Some(reason) = swofty_core::platform::run_as_admin::elevation_blocker(&g.dir)
    {
        bail!("{reason}");
    }
    if swofty_core::platform::run_as_admin::write_blocker(&g.dir).is_some() {
        bail!(
            "{}",
            swofty_core::platform::run_as_admin::write_blocked_text(&g.dir)
        );
    }
    end_on_signals();
    let config: Config = std::fs::read(store.config_path())
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();
    let launch_path = store.root.join("state/launch.json");
    let mut request: LaunchRequest = std::fs::read(&launch_path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();
    age_out(&mut request, written_within(&launch_path, NETPLAY_REQUEST_LIFE));
    if let Some(file) = &request.row_ids {
        match swofty_core::row_ids::Overrides::read(file) {
            Ok(ids) => {
                say(
                    "info",
                    &format!(
                        "room launch: building with the host's stat ids for {} addon(s), load order for {} addon(s) and {} option value(s)",
                        ids.rows.len(),
                        ids.order.len(),
                        ids.option_values()
                    ),
                );
                swofty_core::row_ids::lend(Some(ids));
            }
            Err(e) => bail!(
                "the host's ids and settings for the room couldn't be read ({}): {e}",
                file.display()
            ),
        }
    }
    let profile = request.profile.clone().unwrap_or_else(|| profile.to_owned());
    let build_blocked = swofty_core::build_support::blocked(&g.dir.join(GAME_EXE));
    let stock = build_blocked.is_some();
    let (profile, request) = gate_unsupported_build(build_blocked.clone(), profile, request);
    if let (Ok(p), Ok(installed)) = (store.profile(&profile), store.addons()) {
        let catalog = swofty_core::catalog::cached(&store.root).unwrap_or_default();
        for (base, family) in swofty_core::profile::resolver::lonely_bases(&p, &installed, &catalog) {
            if installed
                .values()
                .any(|m| m.name == base && store.is_builtin(&m.id))
            {
                continue;
            }
            say(
                "warning",
                &format!(
                    "{base} is on without any of its addons, so it adds nothing; install at least one, such as {}",
                    family.join(", ")
                ),
            );
        }
    }
    if let Some(cpus) = config.launch.cpu_affinity.as_deref().and_then(parse_cpu_list) {
        set_affinity(&cpus);
    }
    let game = g.dir.clone();
    let apply = (request.apply || args.apply) && !stock;
    quarantine::Report::clear(&store.root).ok();
    launch::Failure::clear(&store.root).ok();
    crate::commands::laps::note_integrity(store, None, false, &info);
    let session = Session {
        store,
        game: g,
        request_id: request.id.clone(),
        _lock: lock,
    };
    crate::commands::build::with_session_env(request.netplay.is_some());
    if apply {
        say("info", &format!("installing profile {profile}"));
        crate::commands::build::install(store, &profile, g, true, &info)?;
    }
    let installed = apply.then(Instant::now);
    let _home_menu = swofty_core::ui::home_menu::publish(
        &store.root,
        &game,
        env!("CARGO_PKG_VERSION"),
        store.config().home_video,
    );
    become_subreaper();
    let vanilla = store.profile(&profile).is_ok_and(|p| p.is_vanilla());
    if crate::stores::ea::stages_offline(args.via_ea, vanilla) {
        stage_offline(store, &game)?;
    } else {
        say(
            "info",
            "offline play: not staged; the vanilla profile starts F1 25 through the EA app as installed, \
             with EA AntiCheat",
        );
    }
    let before = crate::commands::saves::before_session(
        store,
        &profile,
        g,
        &request.id,
        build_blocked.as_deref(),
        request.steam_saves_once,
    );
    let result = run_loop(store, args, &config, &request, &profile, g, installed);
    let out = finish(session, result);
    crate::commands::saves::withdraw_unanswered_restores(store);
    crate::commands::saves::after_session(before);
    crate::commands::saves::learn_stuck_weekends(store);
    out
}

const UNSUPPORTED_LAUNCH: &str = "launching it stock";

/// A game build the launcher has no table entry for launches stock: the vanilla profile, nothing applied,
/// no launcher saves, no add-on core.
pub(crate) fn gate_unsupported_build(
    blocked: Option<String>,
    profile: String,
    request: LaunchRequest,
) -> (String, LaunchRequest) {
    match blocked {
        Some(version) => {
            say(
                "warning",
                &format!(
                    "this game version ({version}) isn't supported by Swofty Launcher yet; add-ons are off until a \
                     launcher update, so F1 25 runs without them ({UNSUPPORTED_LAUNCH}; set {} to override)",
                    swofty_core::build_support::ALLOW_ENV
                ),
            );
            (
                swofty_core::profile::VANILLA.to_owned(),
                LaunchRequest {
                    apply: false,
                    row_ids: None,
                    steam_saves_once: false,
                    ..request
                },
            )
        }
        None => (profile, request),
    }
}

fn stage_offline(store: &Store, game: &Path) -> Result<()> {
    let artifacts = crate::commands::offline::ensure_extension(store)?;
    if let Some(note) = crate::commands::offline::loader_note(&artifacts) {
        say("info", &format!("offline play: {note}"));
    }
    let probe = swofty_core::offline::probe_cached(&store.root, game, &artifacts)?;
    if !probe.supported {
        bail!(
            "this F1 25 build is not supported by the offline play extension yet ({}); \
             the modded session was refused",
            probe.report
        );
    }
    say("info", &format!("game build supported: {}", probe.report));
    let journal = swofty_core::offline::journal_path(&store.root);
    let staged = swofty_core::offline::stage(game, &artifacts, &journal);
    if let Err(swofty_core::Error::MissingStock { files, version }) = &staged {
        swofty_core::game_folder::stock::Report::missing(files.clone(), game, version.clone())
            .write(&store.root)
            .ok();
    }
    for note in staged? {
        say("info", &format!("offline play: {note}"));
    }
    say(
        "info",
        "staged offline play: EA AntiCheat is not started this session; multiplayer is unavailable",
    );
    Ok(())
}

/// A launch the loader check refused before the game started.
#[derive(Debug)]
pub(crate) struct Refused(quarantine::Report);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.text())
    }
}

impl std::error::Error for Refused {}

/// End the session, which rolls the game folder back; a refused launch then
/// exits with its own code, so the launcher and `swofty run` can tell it apart.
pub(crate) fn finish(session: Session, result: Result<ExitCode>) -> Result<ExitCode> {
    drop(session);
    match result.map_err(|e| e.downcast::<Refused>()) {
        Err(Ok(refused)) => {
            eprintln!("swofty: {refused}");
            Ok(ExitCode::from(quarantine::REFUSED_EXIT))
        }
        Err(Err(e)) => Err(e),
        Ok(code) => Ok(code),
    }
}

/// Why a rollback could not touch the game files, when the copy's game is
/// still running and holds them.
fn still_running(game: &Path) -> String {
    match games_in(game).first() {
        Some(g) => format!(
            " because F1 25 ({GAME_EXE}, pid {}) is still running and holds the game files",
            g.pid
        ),
        None => String::new(),
    }
}

/// A wrapper session: it holds the session lock, and however it ends (the
/// game exits or crashes, an error, a panic, a stop signal) dropping it puts
/// the offline play staging and the profile's game files back.
pub(crate) struct Session<'a> {
    pub(crate) store: &'a Store,
    pub(crate) game: &'a Game,
    pub(crate) request_id: String,
    pub(crate) _lock: SessionLock,
}

/// What the wrapper says once the profile's files are back, which is not the
/// whole game folder while offline play's files stayed locked.
pub(super) fn rolled_back(offline_left: bool) -> &'static str {
    if offline_left {
        "rolled the profile's game files back, but not the offline play files above"
    } else {
        "rolled the profile's game files back"
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let offline_left = match swofty_core::offline::restore_pending(&self.store.root, &self.game.dir) {
            Ok(Some(notes)) => {
                for note in notes {
                    info(&format!("offline play: {note}"));
                }
                info("offline play: the game folder is back to vanilla");
                false
            }
            Ok(None) => false,
            Err(e) => {
                info(&format!(
                    "offline play restore failed{}; the next swofty run or launcher start retries it, or run swofty restore: {e:#}",
                    still_running(&self.game.dir)
                ));
                true
            }
        };
        let inst = Installer::new(&self.store.root, self.game);
        let rolled_back = swofty_core::diagnostics::phases::time("rollback.total", || inst.roll_back(false));
        crate::commands::build::print_phases();
        match rolled_back {
            Ok(done) => {
                if !done.is_empty() {
                    info(self::rolled_back(offline_left));
                }
                for d in done.iter().flat_map(|r| &r.drift) {
                    info(&format!("rollback: {d}"));
                }
                let kept = done.iter().map(|r| r.kept.len()).sum::<usize>();
                if kept > 0 {
                    info(&format!(
                        "kept {kept} file(s) addons wrote in {}",
                        swofty_core::addons::storage::dir(&self.store.root).display()
                    ));
                }
            }
            Err(e) => info(&format!(
                "rollback failed{}; the next swofty run or launcher start retries it, or run swofty restore: {e:#}",
                still_running(&self.game.dir)
            )),
        }
        let logs = self
            .store
            .latest_session()
            .filter(|d| {
                std::fs::read_to_string(d.join("request_id")).is_ok_and(|s| s.trim() == self.request_id)
            })
            .map(|d| d.join("game-logs"))
            .unwrap_or_else(|| self.store.sessions_dir().join("orphaned-logs"));
        match inst.tidy(&logs) {
            Ok(0) => {}
            Ok(n) => info(&format!(
                "moved {n} log file(s) out of the game folder to {}",
                logs.display()
            )),
            Err(e) => info(&format!("the loader folder was left in the game folder: {e:#}")),
        }
    }
}

pub(crate) fn run_loop(
    store: &Store,
    args: &RunArgs,
    config: &Config,
    request: &LaunchRequest,
    profile: &str,
    g: &Game,
    mut installed: Option<Instant>,
) -> Result<ExitCode> {
    let profile = profile.to_owned();
    let game = g.dir.clone();
    let mut restarted = false;
    loop {
        if ending() {
            return Ok(ExitCode::FAILURE);
        }
        rebuild_if_needed(store, &profile, &game);
        let code = play(
            store,
            args,
            config,
            request,
            g,
            installed,
            Round {
                profile: &profile,
                restarted,
            },
        )?;
        let relaunch = match store.merge_requests(&profile, true) {
            Ok(Some(m)) => {
                for c in &m.changed {
                    say("info", &format!("in game: {c}"));
                }
                for c in &m.skipped {
                    say("info", &format!("in game, skipped: {c}"));
                }
                m.relaunch
            }
            Ok(None) => false,
            Err(e) => {
                say("info", &format!("in-game changes not merged: {e}"));
                false
            }
        };
        if !relaunch || ending() {
            return Ok(ExitCode::from(code.clamp(0, 255) as u8));
        }
        say("info", "Apply & restart: starting F1 25 again");
        restarted = true;
        if installed.is_some() {
            say("info", &format!("reinstalling profile {profile}"));
            crate::commands::build::install(store, &profile, g, true, &info)?;
            installed = Some(Instant::now());
        }
    }
}

fn rebuild_if_needed(store: &Store, profile: &str, game: &Path) {
    let pending = match (store.profile(profile), store.addons()) {
        (Ok(p), Ok(addons)) => p.rebuild_pending(&addons),
        _ => return,
    };
    if pending.is_empty() {
        return;
    }
    say(
        "info",
        &format!("rebuilding {profile} for {}", pending.join(", ")),
    );
    if let Err(e) = crate::commands::build::build(store, profile, Some(game.to_path_buf()), false) {
        say("info", &format!("rebuild failed, launching anyway: {e:#}"));
    }
}

pub(crate) const NETPLAY_REQUEST_LIFE: Duration = Duration::from_secs(600);

/// A wrapper Steam starts later on its own must not inherit an earlier launch's one-session asks: its
/// arming, the host's row ids or Steam's saves. `fresh` is whether the request was written just now.
pub(crate) fn age_out(request: &mut LaunchRequest, fresh: bool) {
    if !fresh {
        request.netplay = None;
        request.row_ids = None;
        request.steam_saves_once = false;
    }
}

pub(crate) fn written_within(path: &Path, max: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age <= max)
}

pub(crate) struct Round<'a> {
    profile: &'a str,
    restarted: bool,
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::*;

    #[test]
    fn an_unsupported_build_launches_stock_with_nothing_applied() {
        let request = LaunchRequest {
            id: "x".into(),
            apply: true,
            steam_saves_once: true,
            row_ids: Some(PathBuf::from("ids.json")),
            ..LaunchRequest::default()
        };
        let (profile, out) = gate_unsupported_build(Some("1.0.145.2472".into()), "grid".into(), request);
        assert_eq!(profile, swofty_core::profile::VANILLA);
        assert!(!out.apply && out.row_ids.is_none() && !out.steam_saves_once);
        assert_eq!(out.id, "x");
    }

    #[test]
    fn a_supported_build_keeps_its_profile_and_request() {
        let request = LaunchRequest {
            apply: true,
            ..LaunchRequest::default()
        };
        let (profile, out) = gate_unsupported_build(None, "grid".into(), request.clone());
        assert_eq!(profile, "grid");
        assert_eq!(out, request);
    }

    #[test]
    fn an_old_launch_request_loses_its_arming() {
        let path = std::env::temp_dir().join(format!("swofty-launch-age-{}", std::process::id()));
        std::fs::write(&path, "{}").unwrap();
        assert!(written_within(&path, NETPLAY_REQUEST_LIFE));
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        assert!(!written_within(&path, NETPLAY_REQUEST_LIFE));
        assert!(!written_within(
            &path.with_extension("missing"),
            NETPLAY_REQUEST_LIFE
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_old_launch_request_plays_on_launcher_saves_again() {
        let asked = || LaunchRequest {
            netplay: Some("armed".into()),
            row_ids: Some(PathBuf::from("row_ids.json")),
            steam_saves_once: true,
            ..LaunchRequest::default()
        };
        let mut fresh = asked();
        age_out(&mut fresh, true);
        assert_eq!(fresh, asked());
        let mut old = asked();
        age_out(&mut old, false);
        assert!(!old.steam_saves_once);
        assert_eq!(old.netplay, None);
        assert_eq!(old.row_ids, None);
    }
}
