//! Finding a game's install folders: by its store ids and exe, or through a game's own detection.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use swofty_addon::game::{GameId, GameInfo, StoreId};

use crate::game_folder::copies::same_dir;
use crate::platform::steam;
use crate::session::Config;

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub game: GameId,
    pub dir: PathBuf,
    pub store: Option<StoreId>,
    pub build_id: Option<u64>,
}

/// Where detection looks: the launcher's config (for folders the player added), the Steam roots and the
/// folders of Epic Games Launcher manifests.
pub struct Probe<'a> {
    pub config: &'a Config,
    pub steam_roots: Vec<PathBuf>,
    pub epic_manifests: Vec<PathBuf>,
}

impl<'a> Probe<'a> {
    pub fn here(config: &'a Config) -> Self {
        Self {
            config,
            steam_roots: steam::steam_roots(),
            epic_manifests: epic_manifest_dirs(),
        }
    }
}

pub trait Detect: Sync {
    fn find(&self, info: &'static GameInfo, probe: &Probe<'_>) -> Vec<Found>;
}

/// A game found by its store ids, with one of its exes in the folder.
pub struct ByStores;

impl Detect for ByStores {
    fn find(&self, info: &'static GameInfo, probe: &Probe<'_>) -> Vec<Found> {
        let mut out: Vec<Found> = Vec::new();
        let has_exe = |dir: &Path| info.exes.iter().any(|e| dir.join(e).is_file());
        for store in info.stores {
            match *store {
                StoreId::Steam { app_id } => {
                    for root in &probe.steam_roots {
                        if let Some(g) = steam::find_app_in(root, app_id)
                            && has_exe(&g.dir)
                        {
                            push(&mut out, info.game, g.dir, *store, g.build_id);
                        }
                    }
                }
                StoreId::Epic { exe } => {
                    for dir in epic_installs(&probe.epic_manifests, exe) {
                        push(&mut out, info.game, dir, *store, None);
                    }
                }
                StoreId::EaApp { .. } => {}
            }
        }
        out
    }
}

fn push(out: &mut Vec<Found>, game: GameId, dir: PathBuf, store: StoreId, build_id: Option<u64>) {
    if !out.iter().any(|f| same_dir(&f.dir, &dir)) {
        out.push(Found {
            game,
            dir,
            store: Some(store),
            build_id,
        });
    }
}

fn epic_manifest_dirs() -> Vec<PathBuf> {
    if !cfg!(windows) || steam::isolated_root().is_some() {
        return Vec::new();
    }
    std::env::var_os("ProgramData")
        .map(|p| PathBuf::from(p).join(r"Epic\EpicGamesLauncher\Data\Manifests"))
        .into_iter()
        .collect()
}

/// The install folders of the Epic Games Launcher manifests (`*.item`) in `dirs` whose launch exe is
/// `exe` and is in the folder.
pub fn epic_installs(dirs: &[PathBuf], exe: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let mut items: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
            .collect();
        items.sort();
        for install in items.iter().filter_map(|i| epic_item(i, exe)) {
            if !out.iter().any(|o| same_dir(o, &install)) {
                out.push(install);
            }
        }
    }
    out
}

fn epic_item(path: &Path, exe: &str) -> Option<PathBuf> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Item {
        install_location: PathBuf,
        launch_executable: String,
    }
    let item: Item = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let launch = PathBuf::from(item.launch_executable.replace('\\', "/"));
    let named = launch.file_name()?.to_str()?.eq_ignore_ascii_case(exe);
    (named && item.install_location.join(&launch).is_file()).then_some(item.install_location)
}
