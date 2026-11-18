use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_core::install::Game;
use swofty_core::store::Store;
use swofty_core::testsuite::fixture::{self, Fixture};
use swofty_core::testsuite::plan::{Batch, Plan};
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::suite::Kind;

use crate::commands::testing::run::MIN_FREE;
use crate::commands::testing::say;

/// Checks before the lock: nothing here changes anything.
pub(crate) fn preflight(store: &Store, g: &Game, plan: &Plan, out: &Path) -> std::result::Result<(), String> {
    terms(store)?;
    if !swofty_core::game_folder::setup::done(&store.root) {
        return Err(swofty_core::game_folder::setup::NEEDED.to_owned());
    }
    if swofty_core::platform::steam::in_steam_library(&g.dir) {
        let status = swofty_core::platform::steam::client::status();
        if !status.state.usable() {
            return Err(format!(
                "Steam is not ready ({status}); start Steam and sign in, then run the tests again"
            ));
        }
        if let Some(state) = swofty_core::platform::steam::find_game()
            .filter(|i| swofty_core::game_folder::copies::same_dir(&i.dir, &g.dir))
            .and_then(|i| swofty_core::platform::steam::busy(&i))
        {
            return Err(format!("Steam is {state} F1 25; wait for it to finish"));
        }
    }
    if let Some(free) = swofty_core::platform::disk::free_space(out)
        && free < MIN_FREE
    {
        return Err(format!(
            "{} has {} free; a test run needs at least {}",
            out.display(),
            swofty_core::platform::disk::gb(free),
            swofty_core::platform::disk::gb(MIN_FREE)
        ));
    }
    addons_installed(store, plan)
}

/// A career environment runs only in the save sandbox, and only on an exe
/// whose core bindings carry the sandbox's install point (verified in the
/// KB). Its fixture must resolve and match its fixture.json. Returns the
/// exe's sha256 when a batch needs the sandbox.
pub(crate) fn sandbox_preflight(
    store: &Store,
    g: &Game,
    plan: &Plan,
    bases: &BTreeMap<String, PathBuf>,
) -> std::result::Result<Option<String>, String> {
    let need: Vec<&Batch> = plan
        .batches
        .iter()
        .filter(|b| sandboxed(b) && b.starts_game())
        .collect();
    if need.is_empty() {
        return Ok(None);
    }
    let mut hashes = swofty_core::game_folder::baseline::Hashes::load(&store.root);
    let exe = g.dir.join(swofty_core::GAME_EXE);
    let sha = hashes
        .sha256(&exe)
        .map_err(|e| format!("could not hash {}: {e}", exe.display()))?;
    hashes.save();
    let loader = swofty_core::addons::builtin::loader_dir();
    check_sandbox(
        loader.as_deref(),
        &sha,
        &need,
        bases,
        &fixture::store(&store.root),
    )?;
    Ok(Some(sha))
}

pub(crate) fn sandboxed(b: &Batch) -> bool {
    b.env.kind == Kind::Career || (b.env.kind == Kind::Session && b.env.str("fixture").is_some())
}

pub(crate) fn check_sandbox(
    loader: Option<&Path>,
    exe_sha: &str,
    batches: &[&Batch],
    bases: &BTreeMap<String, PathBuf>,
    store: &Path,
) -> std::result::Result<(), String> {
    let Some(loader) = loader else {
        return Err(
            "a career environment needs the save sandbox, and the launcher's loader (version.dll and the \
                    core) was not found"
                .to_owned(),
        );
    };
    let proof = sandbox::verified(loader, exe_sha)
        .map_err(|e| format!("a career environment needs the save sandbox: {e}"))?;
    say(&format!("save sandbox: verified for this F1_25.exe by {proof}"));
    for b in batches {
        let f = batch_fixture(b, bases, store).map_err(|e| format!("{}: {e}", b.label()))?;
        if f.manifest.exe.sha256 != exe_sha {
            say(&format!(
                "{}: fixture {} was saved on another F1_25.exe ({}); older saves normally load forward",
                b.label(),
                f.manifest.name,
                &f.manifest.exe.sha256[..f.manifest.exe.sha256.len().min(16)]
            ));
        }
    }
    Ok(())
}

pub(crate) fn batch_fixture(
    b: &Batch,
    bases: &BTreeMap<String, PathBuf>,
    store: &Path,
) -> std::result::Result<Fixture, String> {
    let spec = b
        .env
        .str("fixture")
        .ok_or("a career environment names no fixture")?;
    let base = b
        .tests
        .first()
        .and_then(|t| bases.get(&t.suite))
        .map(PathBuf::as_path);
    fixture::load(&fixture::resolve(spec, base, store)?)
}

/// The offline play terms are the player's to accept; a test run never
/// accepts them.
pub(crate) fn terms(store: &Store) -> std::result::Result<(), String> {
    match swofty_core::offline::is_accepted(&store.root) {
        true => Ok(()),
        false => Err(format!(
            "the offline play terms are not accepted in {}; a test run never accepts them for you: run swofty \
             offline accept yourself, then run the tests again",
            store.root.display()
        )),
    }
}

pub(crate) fn addons_installed(store: &Store, plan: &Plan) -> std::result::Result<(), String> {
    let installed = store.addons().map_err(|e| e.to_string())?;
    for b in plan.batches.iter().filter(|b| b.starts_game()) {
        if let Some(id) = b.addons.iter().find(|id| !installed.contains_key(*id)) {
            return Err(format!(
                "{id} is not installed, and the tests in {} need it: install it with swofty addons install <folder>",
                b.label()
            ));
        }
    }
    Ok(())
}
