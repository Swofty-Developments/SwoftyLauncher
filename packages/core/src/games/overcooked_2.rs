//! Overcooked 2, not supported yet: found on Steam and the Epic Games Launcher, and nothing more.

use super::GameDef;
use super::detect::ByStores;

pub static DEF: GameDef = GameDef {
    info: &swofty_addon::game::OVERCOOKED_2,
    addon_kinds: &[],
    online: false,
    time_trial: false,
    detect: &ByStores,
    build: None,
    loader: None,
    launch: None,
};
