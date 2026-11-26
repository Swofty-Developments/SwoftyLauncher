//! F1 25: every service the launcher has today, and detection through the copies the launcher already finds.

pub mod activation;
pub mod build;
pub mod build_support;
pub mod content;
pub mod dbedit;
pub mod erp;
pub mod game_db;
pub mod hud;
pub mod loader_check;
pub mod offline;
pub mod planner;
pub mod row_ids;
pub mod saves;
pub mod stdlib;
pub mod testsuite;
pub mod tracks;
pub mod ui;

use std::path::PathBuf;

use swofty_addon::game::{GameInfo, StoreId};
use swofty_addon::manifest::Kind;

use super::{Build, Detect, Found, GameDef, Launch, Loader, Probe};
use crate::game_folder::copies::{self as game_copies, GameCopy};
use crate::install::Source;
use crate::platform::{EaApp, LaunchPlatform, steam};
use crate::session::Config;

pub static DEF: GameDef = GameDef {
    info: &swofty_addon::game::F1_25,
    addon_kinds: &[
        Kind::Database,
        Kind::Erp,
        Kind::Text,
        Kind::Ui,
        Kind::Files,
        Kind::Native,
        Kind::Simplified,
    ],
    online: true,
    time_trial: true,
    detect: &Copies,
    build: Some(&Pipeline),
    loader: Some(&VersionProxy),
    launch: Some(&SteamOrEaApp),
};

/// The copies of F1 25 on this computer (Steam's, the EA app's and folders the player added), in the
/// order the switcher lists them.
pub fn copies(config: &Config) -> Vec<GameCopy> {
    game_copies::discover(config)
}

/// The copies [`copies`] finds, with Steam looked for in `steam_roots` instead of every Steam root here.
pub fn copies_in(config: &Config, steam_roots: &[PathBuf]) -> Vec<GameCopy> {
    game_copies::gather(
        steam_roots.iter().find_map(|r| steam::find_game_in(r)),
        EaApp.installs(),
        &config.games.added,
    )
}

pub struct Copies;

impl Detect for Copies {
    fn find(&self, info: &'static GameInfo, probe: &Probe<'_>) -> Vec<Found> {
        copies_in(probe.config, &probe.steam_roots)
            .into_iter()
            .map(|c| Found {
                game: info.game,
                store: store_of(info, c.source),
                build_id: c.build_id(),
                dir: c.dir,
            })
            .collect()
    }
}

fn store_of(info: &GameInfo, source: Source) -> Option<StoreId> {
    info.stores.iter().copied().find(|s| {
        matches!(
            (s, source),
            (StoreId::Steam { .. }, Source::Steam) | (StoreId::EaApp { .. }, Source::EaApp)
        )
    })
}

pub struct Pipeline;
impl Build for Pipeline {
    fn lower_manifest(
        &self,
        dir: &std::path::Path,
        m: &mut crate::manifest::AddonManifest,
    ) -> swofty_addon::Result<()> {
        stdlib::screens::lower(dir, m)?;
        stdlib::routes::lower(dir, m)
    }
}

pub struct VersionProxy;
impl Loader for VersionProxy {}

pub struct SteamOrEaApp;
impl Launch for SteamOrEaApp {}
