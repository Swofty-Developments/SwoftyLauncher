//! Time Trial boards: the launcher's side of uploading laps the core records.
//!
//! The core writes one `.swlap` file per completed lap into `<game>/swofty/laps`. [`queue`] moves them
//! into the launcher's data folder and uploads them through [`api`]; [`upload`] is the loop the wrapper
//! and the launcher run for it.

pub mod api;
pub mod queue;
pub mod swlap;
pub mod upload;

use std::path::{Path, PathBuf};

use crate::session::Config;

pub const LAPS_DIR: &str = "swofty/laps";
pub const SESSION_FILE: &str = "swofty/session.json";

pub fn laps_dir(game: &Path) -> PathBuf {
    game.join(LAPS_DIR)
}

/// The page that shows the boards.
pub fn boards_url() -> String {
    std::env::var("SWOFTY_TT_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_owned())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://tt.f1.swofty.net".to_owned())
}

/// The token of the sign-in when Time Trial boards is on: the setting is on and the player is signed in.
pub fn token(data: &Path, config: &Config) -> Option<String> {
    config
        .time_trial_boards
        .then(|| crate::mpauth::load(data))
        .flatten()
        .map(|s| s.token)
}
