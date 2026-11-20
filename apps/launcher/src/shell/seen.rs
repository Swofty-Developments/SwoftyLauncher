use swofty_core::diagnostics::launch;
use swofty_core::diagnostics::quarantine;
use swofty_core::game_folder::recovery::Notice;
use swofty_core::game_folder::stock;
use swofty_core::online::server::integrity;
use swofty_core::store::Store;

use crate::services::game;

/// The timestamp of the newest report of each kind the launcher has already shown, so a
/// report is announced once and an old one from before the launcher started is not.
#[derive(Clone, Copy, Default)]
pub(crate) struct Seen {
    pub(crate) recovered: f64,
    pub(crate) mods_blocked: f64,
    pub(crate) not_stock: f64,
    pub(crate) blocked: f64,
    pub(crate) launch_failed: f64,
    pub(crate) integrity_missing: f64,
}

impl Seen {
    pub(crate) fn read(store: &Store) -> Self {
        let root = &store.root;
        Self {
            recovered: Notice::read(root).map_or(0.0, |n| n.at),
            mods_blocked: quarantine::Report::read(root).map_or(0.0, |r| r.at),
            not_stock: stock::Report::read(root).map_or(0.0, |r| r.at),
            blocked: game::blocked_at(root).unwrap_or(0.0),
            launch_failed: launch::Failure::read(root).map_or(0.0, |f| f.at),
            integrity_missing: integrity::Notice::read(root).map_or(0.0, |n| n.at),
        }
    }
}
