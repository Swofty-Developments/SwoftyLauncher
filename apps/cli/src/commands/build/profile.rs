use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use swofty_core::addons::settings::SETTINGS_FILE;
use swofty_core::build::ADDONS_DIR;
use swofty_core::install::{Game, LOADER_DIR};
use swofty_core::online::modules::{self, Plugin};
use swofty_core::online::server::integrity::{self, Module};
use swofty_core::store::Store;

pub(super) fn timed_build(
    store: &Store,
    profile: &str,
    g: &Game,
    integrity: Option<&Module>,
    plugins: &[(Plugin, modules::Module)],
) -> Result<swofty_core::build::Built> {
    swofty_core::diagnostics::phases::time("play.build", || {
        let built = build_profile(store, profile, g, integrity, plugins);
        swofty_core::diagnostics::phases::end_step();
        built
    })
}

pub(super) fn build_profile(
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
        None => store.session_settings_on(
            &store.profile(profile)?,
            swofty_core::install::Source::detect(&g.dir),
        )?,
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
pub(super) fn server_plugins(store: &Store, out: &dyn Fn(&str)) -> Vec<(Plugin, modules::Module)> {
    let Some(session) = swofty_core::online::server::sign_in::load(&store.root) else {
        return Vec::new();
    };
    let api = swofty_core::online::server::api::Api::new(&session.token);
    let mut found = Vec::new();
    let wanted = [modules::TT_GAME]
        .into_iter()
        .chain(SESSION_ENV.load(Ordering::Relaxed).then_some(modules::MP_GAME));
    for plugin in wanted {
        match modules::ensure(
            &store.root,
            &api,
            plugin.name,
            modules::GAME_PLATFORM,
            plugin.abi,
            out,
        ) {
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

pub(super) fn report(profile: &str, built: &swofty_core::build::Built) {
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

fn common_root<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let mut roots: Vec<&str> = paths.filter_map(|p| p.split('/').next()).collect();
    roots.sort();
    roots.dedup();
    roots.join(", ")
}
