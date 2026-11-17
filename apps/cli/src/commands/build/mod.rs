use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::commands::{Execute, Invocation};
use anyhow::{Context, Result, bail};
use clap::Args;
use swofty_core::baseline::DepotHashes;
use swofty_core::build::{ADDONS_DIR, GVDB};
use swofty_core::busy::{self, Busy, Step};
use swofty_core::install::{EA_APP_EXPERIMENTAL, Game, Installer, LOADER_DIR, Source};
use swofty_core::managed;
use swofty_core::modules::{self, Plugin};
use swofty_core::recovery::{Blocked, Scope, SessionLock};
use swofty_core::settings::SETTINGS_FILE;
use swofty_core::stock::{self, Depots};
use swofty_core::store::Store;
use swofty_core::timetrial::integrity::{self, Module};
use swofty_core::{GAME_EXE, steam};
use swofty_formats::erp::Erp;
use swofty_formats::gvdb::Gvdb;

#[derive(Args)]
pub(crate) struct BuildArgs {
    #[arg(long, help = crate::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
    #[arg(
        long,
        help = "Also list every side-data finding of add-row copies: known lookups, matches to review, matches in other game areas and acknowledged ones"
    )]
    pub(crate) side_data: bool,
}
#[derive(Args)]
pub(crate) struct ConflictsArgs {
    #[arg(long, help = "Build the profile without installing it and show its conflicts")]
    pub(crate) check: bool,
    #[arg(long, help = crate::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
}
#[derive(Args)]
pub(crate) struct ApplyArgs {
    #[arg(long, help = crate::GAME_HELP)]
    pub(crate) game: Option<PathBuf>,
}
#[derive(Args)]
pub(crate) struct VerifyArgs {
    #[arg(long)]
    pub(crate) game: Option<PathBuf>,
}
#[derive(Args)]
pub(crate) struct RestoreArgs {
    #[arg(long)]
    pub(crate) game: Option<PathBuf>,
}

impl Execute for BuildArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        build(&invocation.store, &invocation.profile, self.game, self.side_data)
    }
}

impl Execute for ConflictsArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        conflicts(&invocation.store, &invocation.profile, self.check, self.game)
    }
}

impl Execute for ApplyArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        apply(&invocation.store, &invocation.profile, self.game)
    }
}

impl Execute for VerifyArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        verify(&invocation.store, self.game)
    }
}

impl Execute for RestoreArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        restore(&invocation.store, self.game)
    }
}
const REPAIR_TIMEOUT: Duration = Duration::from_secs(20);
pub const UI_ERPS: &[&str] = &[
    "2025_asset_groups/ui_package/flow_frontend.erp",
    "2025_asset_groups/ui_package/persistent.erp",
    "2025_asset_groups/ui_package/databases.erp",
    "2025_asset_groups/ui_package/screens/fe_home.erp",
];
pub fn game(store: &Store, explicit: Option<PathBuf>) -> Result<Game> {
    Ok(Game::locate(&store.root, game_dir(store, explicit)?)?)
}

pub fn supported(g: &Game) -> Result<()> {
    if Source::detect(&g.dir) == Source::EaApp {
        eprintln!("swofty: {}: {EA_APP_EXPERIMENTAL}", g.dir.display());
    }
    Ok(())
}

/// `--game`, else the active copy (`swofty game use`, the launcher's game
/// switcher), else Steam's.
fn game_dir(store: &Store, explicit: Option<PathBuf>) -> Result<PathBuf> {
    explicit
        .or_else(|| swofty_core::copies::find_active(&store.config()).map(|c| c.dir))
        .context("F1 25 not found; pass --game, add the folder with swofty game add, or run swofty doctor")
}

pub fn build(store: &Store, profile: &str, game: Option<PathBuf>, side_data: bool) -> Result<ExitCode> {
    let g = self::game(store, game)?;
    let built = timed_build(store, profile, &g, None, &[])?;
    report(profile, &built);
    print_phases();
    if side_data {
        println!(
            "side data of add-row copies ({} finding(s)):",
            built.side_data.len()
        );
        for f in &built.side_data {
            println!("  {f}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Prints the Play path's phase timings and findings recorded since the last call.
pub fn print_phases() {
    for line in swofty_core::phases::take() {
        crate::console::output::say(swofty_core::phases::TAG, &line);
    }
}

fn timed_build(
    store: &Store,
    profile: &str,
    g: &Game,
    integrity: Option<&Module>,
    plugins: &[(Plugin, modules::Module)],
) -> Result<swofty_core::build::Built> {
    swofty_core::phases::time("play.build", || {
        let built = build_profile(store, profile, g, integrity, plugins);
        swofty_core::phases::end_step();
        built
    })
}

fn build_profile(
    store: &Store,
    profile: &str,
    g: &Game,
    integrity: Option<&Module>,
    plugins: &[(Plugin, modules::Module)],
) -> Result<swofty_core::build::Built> {
    let addons = store.addons()?;
    let mut p = store.profile(profile)?;
    let mut built = swofty_core::build::build(store, profile, g)?;
    if p.is_vanilla() {
        return Ok(built);
    }
    let settings = built.overlay.join(SETTINGS_FILE);
    let dir = settings.parent().expect("has parent");
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let session = match built.settings.take() {
        Some(s) => s,
        None => store.session_settings(profile)?,
    };
    let mut session = session;
    if let Some(m) = integrity
        && !built.plan.loader.is_empty()
    {
        integrity::place(m, &mut built.plan.loader, &mut session.plugins);
        built.lines.push(format!(
            "integrity: {} {} under {LOADER_DIR}/{ADDONS_DIR}/{}",
            integrity::DLL,
            m.version,
            integrity::PLUGIN_ID
        ));
    }
    if !built.plan.loader.is_empty() {
        for (plugin, m) in plugins {
            modules::place(m, plugin.id, &mut built.plan.loader, &mut session.plugins);
            built.lines.push(format!(
                "module {}: {} {} under {LOADER_DIR}/{ADDONS_DIR}/{}",
                plugin.name,
                m.file_name(),
                m.version,
                plugin.id
            ));
        }
    }
    std::fs::write(&settings, serde_json::to_vec_pretty(&session)?)
        .with_context(|| format!("writing {}", settings.display()))?;
    if !built.plan.loader.is_empty() {
        built.plan.loader.insert(SETTINGS_FILE.into(), settings);
    }
    p.mark_built(&addons);
    store.update_profile(profile, |fresh| {
        fresh.applied = p.applied;
        Ok(())
    })?;
    Ok(built)
}

static SESSION_ENV: AtomicBool = AtomicBool::new(false);

/// Whether the game this process starts gets the session module's environment; its in-game plugin is
/// placed only then.
pub fn with_session_env(on: bool) {
    SESSION_ENV.store(on, Ordering::Relaxed);
}

/// The in-game plugins the server publishes for a signed-in session, verified. A plugin that is not
/// there is one log line with the reason and the session goes on without it.
fn server_plugins(store: &Store, out: &dyn Fn(&str)) -> Vec<(Plugin, modules::Module)> {
    let Some(session) = swofty_core::mpauth::load(&store.root) else {
        return Vec::new();
    };
    let api = swofty_core::timetrial::api::Api::new(&session.token);
    let mut found = Vec::new();
    let wanted = [modules::TT_GAME]
        .into_iter()
        .chain(SESSION_ENV.load(Ordering::Relaxed).then_some(modules::MP_GAME));
    for plugin in wanted {
        match modules::ensure(&store.root, &api, plugin.name, modules::GAME_PLATFORM, plugin.abi) {
            Ok(m) => {
                if m.is_local() {
                    out(&format!(
                        "module {}: warning: {} is set; the local build {} is placed unsigned and the server's is not used",
                        plugin.name,
                        modules::local_env(plugin.name),
                        m.path.display()
                    ));
                }
                found.push((plugin, m));
            }
            Err(why) => out(&format!(
                "module {}: not loaded for this session: {why}",
                plugin.name
            )),
        }
    }
    found
}

fn report(profile: &str, built: &swofty_core::build::Built) {
    if built.plan.is_vanilla() {
        println!("{profile} plays without addons; nothing to build");
        return;
    }
    println!("profile {profile}: {}", built.order.join(" -> "));
    for l in &built.lines {
        println!("{l}");
    }
    for w in &built.warnings {
        println!("warning: {w}");
    }
    println!("overlay {}: {}", built.buildhash, built.overlay.display());
    for rel in built.plan.data.keys() {
        println!("  replace {rel}");
    }
    for rel in built.plan.loader.keys() {
        println!("  place {rel}");
    }
    let added = &built.plan.added;
    if !added.is_empty() {
        println!(
            "  add {} file(s) under {}",
            added.len(),
            common_root(added.keys())
        );
    }
}

pub fn conflicts(store: &Store, profile: &str, check: bool, game: Option<PathBuf>) -> Result<ExitCode> {
    let report = match check {
        true => swofty_core::conflicts::check(store, profile, &self::game(store, game)?)?,
        false => match swofty_core::conflicts::last(store, profile) {
            Some(report) => report,
            None => {
                println!("{profile} has not been built yet; run swofty conflicts --check");
                return Ok(ExitCode::SUCCESS);
            }
        },
    };
    let names: std::collections::BTreeMap<String, String> = store
        .registry()?
        .manifests()
        .into_iter()
        .map(|(id, m)| (id, m.name))
        .collect();
    print!("{}", conflicts_text(&report, &names));
    Ok(ExitCode::SUCCESS)
}

fn conflicts_text(
    report: &swofty_core::conflicts::Report,
    names: &std::collections::BTreeMap<String, String>,
) -> String {
    if report.pairs.is_empty() {
        return format!("{}: no addon overrides another\n", report.profile);
    }
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let mut out = String::new();
    for pair in &report.pairs {
        out.push_str(&pair.line(&name));
        out.push('\n');
        for item in &pair.items {
            out.push_str(&format!("  {item}\n"));
        }
        let more =
            (pair.database + pair.resources + pair.files + pair.strings).saturating_sub(pair.items.len());
        if more > 0 && pair.items.len() >= swofty_core::conflicts::MAX_ITEMS {
            out.push_str(&format!("  and {more} more\n"));
        }
    }
    out
}

fn common_root<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let mut roots: Vec<&str> = paths.filter_map(|p| p.split('/').next()).collect();
    roots.sort();
    roots.dedup();
    roots.join(", ")
}

/// Build the profile and install it. Files an unfinished session left in the
/// way that are provably ours are cleared and the install is tried once
/// more; anything else stops it untouched, with what a restore would do.
pub fn install(store: &Store, profile: &str, g: &Game, session: bool, out: &dyn Fn(&str)) -> Result<()> {
    if swofty_core::compat::write_blocker(&g.dir).is_some() {
        bail!("{}", swofty_core::compat::write_blocked_text(&g.dir));
    }
    repair_importer(store, profile, out);
    let done = AtomicBool::new(false);
    let copied = Copied::default();
    let progress = |n: usize, total: usize| {
        copied.0.store(n as u64, Ordering::Relaxed);
        copied.1.store(total as u64, Ordering::Relaxed);
    };
    let busy = session.then(|| {
        busy::clear_cancel(&store.root);
        let addons = store
            .profile(profile)
            .map(|p| p.enabled().map(str::to_owned).collect())
            .unwrap_or_default();
        Busy::new(profile, addons)
    });
    let r = std::thread::scope(|s| {
        s.spawn(|| heartbeat(&done, &store.root, busy, &copied));
        let r = install_checked(store, profile, g, session, out, &progress)
            .map_err(|e| named(store, g, e))
            .map_err(|e| denied(e, &store.root, &g.dir))
            .map_err(|e| outdated(store, g, e));
        done.store(true, Ordering::Relaxed);
        r
    });
    print_phases();
    if session {
        Busy::clear(&store.root);
        busy::clear_cancel(&store.root);
    }
    r
}

/// Downloads Simplified for Swofty again when its program for this OS is gone,
/// which an antivirus does, before the build starts it.
fn repair_importer(store: &Store, profile: &str, out: &dyn Fn(&str)) {
    let on = store
        .profile(profile)
        .is_ok_and(|p| p.enabled().any(|id| id == managed::ID));
    let Some(version) = managed::broken(store).filter(|_| on) else {
        return;
    };
    out(&format!(
        "[repair] {} {version} is missing its program; downloading it again",
        managed::NAME
    ));
    let repaired = managed::release_of(&version, REPAIR_TIMEOUT)
        .map_err(|e| e.to_string())
        .and_then(|r| managed::repair(store, &r).map_err(|e| e.to_string()));
    match repaired {
        Ok(v) => out(&format!("[repair] installed {} {v} again", managed::NAME)),
        Err(e) => out(&format!(
            "[repair] could not install {} again: {e}",
            managed::NAME
        )),
    }
}

#[derive(Default)]
struct Copied(AtomicU64, AtomicU64);

/// While a build and install run, a line every 30 s that they moved, and one
/// a minute while a single step runs long, so the launcher's ten-minute limit
/// on a silent wrapper stops only one that is stuck. A session's build also
/// keeps `state/build.json` current.
fn heartbeat(done: &AtomicBool, data: &Path, mut busy: Option<Busy>, copied: &Copied) {
    const NOTE: Duration = Duration::from_secs(2);
    let first = swofty_core::work();
    let mut beat = Beat::new(first, swofty_core::doing(), Instant::now());
    let mut noted: Option<Instant> = None;
    while !done.load(Ordering::Relaxed) {
        if let Some(b) = busy.as_mut()
            && noted.is_none_or(|n| n.elapsed() >= NOTE)
        {
            noted = Some(Instant::now());
            track(b, swofty_core::work() - first, copied);
            b.write(data).ok();
        }
        std::thread::sleep(Duration::from_millis(250));
        if let Some(line) = beat.line(swofty_core::work(), swofty_core::doing(), Instant::now()) {
            crate::console::output::say("info", &line);
        }
    }
}

struct Beat {
    start: Instant,
    first: u64,
    last: u64,
    doing: String,
    tick: Instant,
    said: Instant,
}

impl Beat {
    const EVERY: Duration = Duration::from_secs(30);
    const QUIET: Duration = Duration::from_secs(60);

    fn new(work: u64, doing: String, at: Instant) -> Self {
        Self {
            start: at,
            first: work,
            last: work,
            doing,
            tick: at,
            said: at,
        }
    }

    fn line(&mut self, work: u64, step: String, at: Instant) -> Option<String> {
        if at.saturating_duration_since(self.tick) < Self::EVERY {
            return None;
        }
        self.tick = at;
        let moved = work != self.last || step != self.doing;
        if !moved && at.saturating_duration_since(self.said) < Self::QUIET {
            return None;
        }
        self.said = at;
        self.last = work;
        self.doing = step;
        Some(format!(
            "build: still working after {} s ({} file(s) and fragment(s) so far{})",
            at.saturating_duration_since(self.start).as_secs(),
            work - self.first,
            match self.doing.is_empty() {
                true => String::new(),
                false => format!("; {}", self.doing),
            }
        ))
    }
}

/// Names the addon or game file a build stopped on when it is missing or cut
/// short, and what to do about it.
fn named(store: &Store, g: &Game, e: anyhow::Error) -> anyhow::Error {
    match e.downcast::<swofty_core::Error>() {
        Ok(e) => swofty_core::build::named(store, &g.dir, e).into(),
        Err(e) => e,
    }
}

/// Names the folder and the fix when Windows refused a write in the game
/// folder or the data folder.
fn denied(e: anyhow::Error, data: &Path, game: &Path) -> anyhow::Error {
    let fix = e.chain().find_map(|c| {
        c.downcast_ref::<swofty_core::Error>()
            .and_then(|c| swofty_core::build::refused_fix(data, game, c))
    });
    match fix {
        Some(fix) => anyhow::anyhow!("{e}. {fix}"),
        None => e,
    }
}

/// Leads with the update to install when an addon's changes failed to apply
/// to an F1 25 older than the newest update the launcher knows. The build's
/// own error follows on the next line, for the logs and Send Logs.
fn outdated(store: &Store, g: &Game, e: anyhow::Error) -> anyhow::Error {
    let version = swofty_core::exe_version::read(&g.dir.join(GAME_EXE));
    let names = store
        .addons()
        .map(|a| a.into_iter().map(|(id, m)| (id, m.name)).collect())
        .unwrap_or_default();
    outdated_with(e, version.as_deref(), &names)
}

fn outdated_with(
    e: anyhow::Error,
    version: Option<&str>,
    names: &std::collections::BTreeMap<String, String>,
) -> anyhow::Error {
    let raw = format!("{e:#}");
    match swofty_core::known_problems::outdated_game(&raw, version, names) {
        Some(d) => anyhow::anyhow!("{}\n({raw})", d.text()),
        None => e,
    }
}

fn track(b: &mut Busy, work: u64, copied: &Copied) {
    let total = copied.1.load(Ordering::Relaxed);
    if total == 0 {
        b.step = Step::Building;
        b.done = work;
    } else {
        b.step = Step::Copying;
        b.done = copied.0.load(Ordering::Relaxed);
        b.total = total;
    }
}

fn install_checked(
    store: &Store,
    profile: &str,
    g: &Game,
    session: bool,
    out: &dyn Fn(&str),
    progress: &dyn Fn(usize, usize),
) -> Result<()> {
    crate::commands::setup::gate(store)?;
    Blocked::clear(&store.root).ok();
    stock::Report::clear(&store.root).ok();
    // A session's wrapper checked the copy before it got here.
    if !session {
        check_entitled(store, g)?;
    }
    let depots = swofty_core::phases::time("play.stock_check", || check_stock(store, g, out))?;
    let inst = Installer {
        session,
        depot: depots.as_ref().map_or(&() as &dyn DepotHashes, |d| d),
        out,
        progress,
        ..Installer::new(&store.root, g)
    };
    let integrity = session
        .then(|| {
            swofty_core::phases::time("play.fetch.integrity", || {
                crate::commands::laps::integrity(store, out)
            })
        })
        .flatten();
    let plugins = if session {
        swofty_core::phases::time("play.fetch.modules", || server_plugins(store, out))
    } else {
        Vec::new()
    };
    let built = timed_build(store, profile, g, integrity.as_ref(), &plugins)?;
    report(profile, &built);
    let found = match swofty_core::phases::time("play.install", || inst.apply(&built.plan)) {
        Err(swofty_core::Error::Leftovers(found)) => found,
        r => return Ok(not_stock(store, g, r)?),
    };
    if found.iter().any(|c| !c.owned) {
        return Err(crate::recover::blocked(store, g, &found));
    }
    let cleared = inst.clear(&found)?;
    crate::recover::announce(store, g, &cleared, out);
    crate::recover::record_cleared(store, &cleared);
    let built = timed_build(store, profile, g, integrity.as_ref(), &plugins)?;
    report(profile, &built);
    match swofty_core::phases::time("play.install", || inst.apply(&built.plan)) {
        Err(swofty_core::Error::Leftovers(found)) => Err(crate::recover::blocked(store, g, &found)),
        r => Ok(not_stock(store, g, r)?),
    }
}

/// Refuse a modded build or session on a copy no store checks the
/// ownership of, and on a Steam copy whose game or Steam API files are not
/// the ones Steam installed. Steam's list is only read here; a copy whose
/// list can't be read is checked without it.
pub fn check_entitled(store: &Store, g: &Game) -> Result<()> {
    let depots = steam::find_game()
        .filter(|i| i.dir == g.dir)
        .and_then(|i| Depots::load(&i).ok());
    swofty_core::entitlement::check(&g.dir, depots.as_ref())?;
    let Some(depots) = depots else {
        return Ok(());
    };
    let changed = swofty_core::entitlement::changed_files(&store.root, &g.dir, &depots)?;
    if !changed.is_empty() {
        stock::Report::new(changed.clone(), &g.dir).write(&store.root)?;
        return Err(swofty_core::Error::NotStock(changed).into());
    }
    Ok(())
}

/// Check the game against Steam's manifests before anything is built from
/// it. Steam's list stays with the install, which checks every original it
/// backs up for the first time. A game folder outside Steam is not checked;
/// a Steam install whose list can't be read says why.
fn check_stock(store: &Store, g: &Game, out: &dyn Fn(&str)) -> Result<Option<Depots>> {
    let Some(found) = steam::find_game().filter(|i| i.dir == g.dir) else {
        return Ok(None);
    };
    let depots = match Depots::load(&found) {
        Ok(d) => d,
        Err(why) => {
            out(&format!(
                "[stock] {why}; the game files are not checked against Steam"
            ));
            heal_launcher(store, &g.dir, None, out)?;
            return Ok(None);
        }
    };
    heal_launcher(store, &g.dir, Some(&depots as &dyn DepotHashes), out)?;
    let checked = stock::check(&store.root, g, &depots)?;
    if !checked.missing.is_empty() {
        let version = swofty_core::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
        stock::Report::missing(checked.missing.clone(), &g.dir, version.clone())
            .write(&store.root)
            .ok();
        return Err(swofty_core::Error::MissingStock {
            files: checked.missing,
            version,
        }
        .into());
    }
    if checked.proven > 0 {
        out(&format!(
            "[stock] {} original(s) on record match Steam's copy",
            checked.proven
        ));
    }
    for rel in &checked.rerecorded {
        out(&format!(
            "[stock] {rel} matches Steam's copy again; backed up as the original"
        ));
    }
    for d in &checked.eac {
        out(&format!(
            "[stock] {} differs from Steam's copy (size {}, Steam {}); not used by modded sessions, continuing",
            d.rel,
            d.size.map_or("missing".to_owned(), |n| n.to_string()),
            d.steam
        ));
    }
    for d in &checked.vr {
        out(&format!(
            "[stock] {} differs from Steam's copy (size {}, Steam {}); a VR runtime replaces it, continuing",
            d.rel,
            d.size.map_or("missing".to_owned(), |n| n.to_string()),
            d.steam
        ));
    }
    for d in &checked.upscaler {
        out(&format!(
            "[stock] {} differs from Steam's copy (size {}, Steam {}); an upscaler drop-in replaces it, continuing",
            d.rel,
            d.size.map_or("missing".to_owned(), |n| n.to_string()),
            d.steam
        ));
    }
    if let Some(n) = checked.swept {
        out(&format!("[stock] {n} game file(s) checked against Steam's list"));
    }
    if !checked.foreign.is_empty() {
        for f in &checked.detail {
            out(&format!(
                "[stock] {} is not Steam's copy (measured {}, size {}, Steam {}, modified {})",
                f.rel,
                f.measured,
                f.size.map_or("missing".to_owned(), |n| n.to_string()),
                f.steam,
                f.modified.map_or("unknown".to_owned(), |t| format!("{t:.0}"))
            ));
        }
        let (changed, missing) = stock::split_absent(&checked.detail);
        let version = swofty_core::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
        if changed.is_empty() && !missing.is_empty() {
            stock::Report::missing(missing.clone(), &g.dir, version.clone())
                .with_detail(checked.detail.clone())
                .write(&store.root)?;
            return Err(swofty_core::Error::MissingStock {
                files: missing,
                version,
            }
            .into());
        }
        stock::Report::new(checked.foreign.clone(), &g.dir)
            .with_detail(checked.detail.clone())
            .write(&store.root)?;
        if missing.is_empty() {
            return Err(swofty_core::Error::NotStock(checked.foreign).into());
        }
        return Err(swofty_core::Error::NotStockAndMissing {
            changed,
            missing,
            version,
        }
        .into());
    }
    Ok(Some(depots))
}

/// Clear a stale EA AntiCheat backup an interrupted session left before the
/// stock check runs, logging what it did. Does nothing while a session is
/// pending.
fn heal_launcher(
    store: &Store,
    game: &Path,
    depot: Option<&dyn DepotHashes>,
    out: &dyn Fn(&str),
) -> Result<()> {
    for note in swofty_core::offline::heal_before_check(&store.root, game, depot)? {
        out(&format!("[stock] {note}"));
    }
    Ok(())
}

/// An original that turned out not to be Steam's while the install backed it
/// up, or one missing from the game folder with no backup, stops the launch
/// the same way as the check before the build.
fn not_stock(store: &Store, g: &Game, r: swofty_core::Result<()>) -> swofty_core::Result<()> {
    match &r {
        Err(swofty_core::Error::NotStock(files)) => {
            stock::Report::new(files.clone(), &g.dir).write(&store.root).ok();
        }
        Err(swofty_core::Error::MissingStock { files, version }) => {
            stock::Report::missing(files.clone(), &g.dir, version.clone())
                .write(&store.root)
                .ok();
        }
        _ => {}
    }
    r
}

pub fn apply(store: &Store, profile: &str, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = self::game(store, game)?;
    supported(&g)?;
    let out = |l: &str| println!("{l}");
    let Some(lock) = SessionLock::try_acquire(&store.root)? else {
        if let Some(b) = Busy::live(&store.root) {
            bail!("{}", b.text());
        }
        bail!(
            "{}",
            crate::recover::held(store, swofty_core::process::game_running())
        );
    };
    crate::recover::stale(store, &lock, &g, Scope::All, &out)?;
    install(store, profile, &g, false, &out)?;
    println!(
        "applied {profile} to {}; swofty restore puts the game back",
        g.dir.display()
    );
    Ok(ExitCode::SUCCESS)
}

pub fn verify(store: &Store, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = self::game(store, game)?;
    let drift = Installer::new(&store.root, &g).verify()?;
    let game = g.dir;
    let mut ok = drift.is_empty();
    for d in &drift {
        println!("DRIFT {d}");
    }
    let mut check = |name: &str, result: Result<bool>| match result {
        Ok(true) => println!("ok    {name}"),
        Ok(false) => {
            ok = false;
            println!("FAIL  {name}: round trip differs");
        }
        Err(e) => {
            ok = false;
            println!("FAIL  {name}: {e:#}");
        }
    };
    check(
        GVDB,
        read(&game, GVDB).and_then(|d| Ok(Gvdb::parse(&d)?.to_bytes() == d)),
    );
    for rel in UI_ERPS {
        check(
            rel,
            read(&game, rel).and_then(|d| Ok(Erp::parse(&d)?.to_bytes() == d)),
        );
    }
    if !game.join(GAME_EXE).is_file() {
        ok = false;
        println!("FAIL  {GAME_EXE} not found in {}", game.display());
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn read(game: &Path, rel: &str) -> Result<Vec<u8>> {
    std::fs::read(game.join(rel)).with_context(|| rel.to_owned())
}

pub fn restore(store: &Store, game: Option<PathBuf>) -> Result<ExitCode> {
    let g = self::game(store, game)?;
    let done = swofty_core::recovery::restore(&store.root, &g)?;
    for n in &done.notes {
        println!("restored: {n}");
    }
    for p in &done.set_aside {
        println!("set aside: {}", p.display());
    }
    let left = done.left;
    if left.is_empty() {
        println!(
            "restored: the game folder matches the baseline for build {}",
            g.identity.build_id
        );
        return Ok(ExitCode::SUCCESS);
    }
    for d in &left {
        println!("not restored: {d}");
    }
    Ok(ExitCode::FAILURE)
}

#[cfg(test)]
mod tests;
