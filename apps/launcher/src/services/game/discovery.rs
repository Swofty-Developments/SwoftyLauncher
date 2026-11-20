use std::path::PathBuf;

use swofty_core::game_folder::copies::{self, Found, GameCopy};
use swofty_core::games;
use swofty_core::install::Source;
use swofty_core::platform::ea_app;
use swofty_core::platform::steam;
use swofty_core::store::Store;

use super::{Copies, Install};

pub fn discover(store: &Store) -> Option<Install> {
    discover_all(store).active
}

pub fn discover_all(store: &Store) -> Copies {
    let config = store.config();
    let found = games::f1_25::copies(&config);
    let active = copies::active(&found, &config);
    let recorded = if active.is_some_and(|c| c.source == Source::EaApp) {
        ea_app::recorded_here()
    } else {
        Vec::new()
    };
    let active = active.map(|c| c.dir.clone());
    let all: Vec<Install> = found.iter().map(|c| install(c, &recorded)).collect();
    Copies {
        active: active.and_then(|dir| all.iter().find(|i| i.dir == dir).cloned()),
        missing: copies::missing(&found, &config),
        all,
    }
}

pub(super) fn install(copy: &GameCopy, recorded: &[PathBuf]) -> Install {
    let steam_root = copy.steam.as_ref().map(|g| g.steam_root.clone());
    let via_steam = steam_root.as_deref().is_some_and(|root| {
        steam::launch_options(root)
            .iter()
            .any(|(_, o)| steam::wrapper_installed(o))
    });
    let mode = if cfg!(windows) {
        "Windows".to_owned()
    } else {
        match steam_root.as_deref().and_then(steam::compat_tool) {
            Some(tool) => format!("Proton · {tool}"),
            None => "Proton".to_owned(),
        }
    };
    Install {
        dir: copy.dir.clone(),
        build_id: copy.build_id(),
        exe_version: copy.version(),
        mode,
        via_steam,
        manual: copy.found == Found::Added,
        source: copy.source,
        moved: (copy.source == Source::EaApp)
            .then(|| ea_app::moved(&copy.dir, recorded, ea_app::is_install))
            .flatten(),
    }
}
