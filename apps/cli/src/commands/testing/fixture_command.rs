use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use swofty_core::store::Store;
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::sandbox::{self, RunDir};

use crate::commands::testing::{CaptureArgs, FixtureCommand, StoreArgs, infra};
use crate::time::stamp;

fn store_dir(store: &Store, a: &StoreArgs) -> PathBuf {
    a.store.clone().unwrap_or_else(|| fixture::store(&store.root))
}

pub(crate) fn fixture_command(store: &Store, cmd: FixtureCommand) -> Result<ExitCode> {
    match cmd {
        FixtureCommand::Capture(a) => capture_command(store, a),
        FixtureCommand::List(a) => {
            let dir = store_dir(store, &a);
            let all = fixture::list(&dir);
            if all.is_empty() {
                println!("no fixtures in {}", dir.display());
            }
            let mut bad = false;
            for f in all {
                match f {
                    Ok(f) => println!(
                        "{:<32} {} file(s), exe {}, {}",
                        f.manifest.name,
                        f.manifest.files.len(),
                        &f.manifest.exe.sha256[..f.manifest.exe.sha256.len().min(16)],
                        f.manifest.description
                    ),
                    Err(e) => {
                        bad = true;
                        println!("BROKEN: {e}");
                    }
                }
            }
            Ok(if bad { ExitCode::FAILURE } else { ExitCode::SUCCESS })
        }
        FixtureCommand::Check(a) => {
            let dir = store_dir(store, &a.store);
            let path = match Path::new(&a.fixture).join(fixture::FILE).is_file() {
                true => PathBuf::from(&a.fixture),
                false => match fixture::resolve(&a.fixture, None, &dir) {
                    Ok(p) => p,
                    Err(e) => return Ok(infra(e)),
                },
            };
            match fixture::load(&path) {
                Ok(f) => {
                    println!(
                        "{}: {} file(s) match fixture.json ({})",
                        f.manifest.name,
                        f.manifest.files.len(),
                        path.display()
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(e) => Ok(infra(e)),
            }
        }
    }
}

pub(crate) fn careers(specs: &[String]) -> std::result::Result<Vec<fixture::Career>, String> {
    specs
        .iter()
        .map(|c| {
            let (mode, slot) = match c.split_once(':') {
                Some((m, s)) => (
                    m,
                    Some(
                        s.parse::<u64>()
                            .map_err(|_| format!("--career {c}: the slot is not a number"))?,
                    ),
                ),
                None => (c.as_str(), None),
            };
            Ok(fixture::Career {
                mode: mode.to_owned(),
                slot,
                ..Default::default()
            })
        })
        .collect()
}

/// The saves under a folder given to `--from`: the folder itself when it
/// holds the profile save, else its `remote/`.
fn remote_in(dir: &Path) -> PathBuf {
    match dir.join(fixture::PROFILE_FILES[0]).is_file() {
        true => dir.to_path_buf(),
        false => dir.join(sandbox::REMOTE),
    }
}

pub(crate) fn capture_command(store: &Store, a: CaptureArgs) -> Result<ExitCode> {
    let dest = store_dir(store, &a.store);
    let mut addons: BTreeMap<String, String> = BTreeMap::new();
    let mut exe = fixture::Exe::default();
    let (remote, storage, source) = match &a.from {
        Some(dir) => {
            if let Some(live) =
                swofty_core::steam::find_game().and_then(|i| swofty_core::steam::save_dir(&i.steam_root))
                && let (Ok(x), Ok(y)) = (dir.canonicalize(), live.canonicalize())
                && (x == y || y.starts_with(&x))
            {
                return Ok(infra(format!(
                    "{} is the player's live Steam save folder; capture from a copy of it, never the live folder",
                    dir.display()
                )));
            }
            (remote_in(dir), Vec::new(), format!("folder {}", dir.display()))
        }
        None => {
            let run = match &a.run {
                Some(id) => RunDir::open(&store.root, id),
                None => RunDir::latest(&store.root),
            };
            let Some(run) = run else {
                return Ok(infra(format!(
                    "no test run with saves in {}; name one with --run or a save folder with --from",
                    sandbox::runs_dir(&store.root).display()
                )));
            };
            if let Ok(b) = std::fs::read(run.root.join("addons.json"))
                && let Ok(m) = serde_json::from_slice::<BTreeMap<String, String>>(&b)
            {
                addons = m;
            }
            if let Ok(b) = std::fs::read(run.root.join(sandbox::RUN_FILE))
                && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b)
                && let Some(sha) = v["exe_sha256"].as_str()
            {
                exe.sha256 = sha.to_owned();
            }
            let storage: Vec<(String, PathBuf)> = std::fs::read_dir(run.storage())
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect();
            (run.remote(), storage, format!("test run {}", run.id))
        }
    };
    for spec in &a.addons {
        let Some((id, v)) = spec.split_once('=') else {
            return Ok(infra(format!("--addon {spec}: expected ID=VERSION")));
        };
        addons.insert(id.to_owned(), v.to_owned());
    }
    if exe.sha256.is_empty()
        && let Ok(g) = crate::commands::build::game(store, None)
    {
        let mut hashes = swofty_core::game_folder::baseline::Hashes::load(&store.root);
        if let Ok(sha) = hashes.sha256(&g.dir.join(swofty_core::GAME_EXE)) {
            exe.sha256 = sha;
            exe.version = swofty_core::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
        }
        hashes.save();
    }
    let holds = fixture::Holds {
        careers: match careers(&a.careers) {
            Ok(c) => c,
            Err(e) => return Ok(infra(e)),
        },
        notes: a.notes.clone().unwrap_or_default(),
    };
    let meta = fixture::Meta {
        description: a.description.clone().unwrap_or_default(),
        created: stamp(),
        exe,
        addons,
        holds,
        source,
    };
    if let Err(e) = std::fs::create_dir_all(&dest) {
        return Ok(infra(format!("{}: {e}", dest.display())));
    }
    match fixture::capture(&a.name, &dest, &remote, &storage, meta) {
        Ok(f) => {
            println!(
                "fixture {} captured: {} file(s) in {}",
                f.manifest.name,
                f.manifest.files.len(),
                f.dir.display()
            );
            for rel in f.manifest.files.keys() {
                println!("  {rel}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Ok(infra(e)),
    }
}
