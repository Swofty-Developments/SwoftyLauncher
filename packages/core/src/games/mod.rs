//! Every game the launcher knows, as a [`GameDef`]: its identity from `swofty_addon::game` and the
//! services it has. A game has a capability when it has the service behind it.

pub mod detect;
pub mod f1_25;
pub mod overcooked_2;

#[cfg(test)]
mod tests;

use swofty_addon::game::{GameId, GameInfo, Support};
use swofty_addon::manifest::Kind;

pub use detect::{Detect, Found, Probe};

/// Builds a profile for the game. Methods arrive when the build pipeline is split by game.
pub trait Build: Sync {
    /// Turns the game's registry sugar in an addon folder into manifest operations, right after the manifest loads.
    fn lower_manifest(
        &self,
        _dir: &std::path::Path,
        _m: &mut crate::manifest::AddonManifest,
    ) -> swofty_addon::Result<()> {
        Ok(())
    }
}

/// Places the game's mod loader. Methods arrive when the loader placing is split by game.
pub trait Loader: Sync {}

/// Starts the game. Methods arrive when launching is split by game.
pub trait Launch: Sync {}

pub struct GameDef {
    pub info: &'static GameInfo,
    pub addon_kinds: &'static [Kind],
    pub online: bool,
    pub time_trial: bool,
    pub detect: &'static dyn Detect,
    pub build: Option<&'static dyn Build>,
    pub loader: Option<&'static dyn Loader>,
    pub launch: Option<&'static dyn Launch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub play: bool,
    pub build: bool,
    pub native_core: bool,
    pub online: bool,
    pub time_trial: bool,
    pub addons: bool,
}

impl GameDef {
    pub fn caps(&self) -> Capabilities {
        Capabilities {
            play: self.info.support == Support::Full && self.launch.is_some(),
            build: self.build.is_some(),
            native_core: self.loader.is_some(),
            online: self.online,
            time_trial: self.time_trial,
            addons: !self.addon_kinds.is_empty(),
        }
    }

    pub fn accepts_kind(&self, kind: Kind) -> bool {
        self.addon_kinds.contains(&kind)
    }

    pub fn find(&self, probe: &Probe<'_>) -> Vec<Found> {
        self.detect.find(self.info, probe)
    }
}

pub static ALL: [&GameDef; 2] = [&f1_25::DEF, &overcooked_2::DEF];

pub fn def(game: GameId) -> &'static GameDef {
    ALL.iter()
        .copied()
        .find(|d| d.info.game == game)
        .expect("every GameId has a GameDef")
}

pub fn by_id(id: &str) -> Option<&'static GameDef> {
    GameId::parse(id).map(def)
}
