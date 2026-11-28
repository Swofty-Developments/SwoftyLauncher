//! Every game the launcher knows, as a [`GameDef`]: its identity from `swofty_addon::game` and the
//! services it has. A game has a capability when it has the service behind it.

pub mod detect;
pub mod f1_25;
pub mod overcooked_2;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

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

/// Places the game's mod loader. F1 25's arrives when its loader placing is split by game.
pub trait Loader: Sync {
    /// The folder of the in-game core the launcher starts the game with, when this launcher has one.
    fn core_dir(&self) -> Option<PathBuf> {
        None
    }
}

/// Starts the game. F1 25's arrives when its launching is split by game.
pub trait Launch: Sync {
    /// Starts the copy in `game_dir` with the in-game core and the addons on in `root`'s data folder.
    fn start(&self, _root: &Path, _game_dir: &Path) -> crate::Result<Started> {
        Err(crate::Error::Other(
            "this game is started through its own Play".into(),
        ))
    }
}

/// A game [`Launch::start`] started: the process the launcher spawned and the session it plays.
#[derive(Debug)]
pub struct Started {
    pub child: std::process::Child,
    /// The session folder: `session.json`, the core's log and status, and the game's own output.
    pub session: PathBuf,
    /// The exe the game runs as, to find its pid when a wrapper started it.
    pub exe: PathBuf,
    /// Files placed in the game folder for this session, taken out again by [`Started::finish`].
    pub placed: Vec<PathBuf>,
}

impl Started {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The pid of the game itself: the spawned process, or the one it started that runs [`Started::exe`].
    pub fn game_pid(&self) -> u32 {
        crate::platform::process::descendant_running(self.pid(), &self.exe).unwrap_or(self.pid())
    }

    /// Takes the files placed for this session back out of the game folder.
    pub fn finish(&self) {
        for path in &self.placed {
            std::fs::remove_file(path).ok();
        }
    }
}

/// One addon a game's own addon store holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    /// The addon as the catalog and the Addon Browser see it.
    pub manifest: crate::manifest::AddonManifest,
    pub dir: PathBuf,
    pub enabled: bool,
    /// The addon's `icon` file, when it names one that is there.
    pub icon: Option<PathBuf>,
}

/// An installed addon folder the store can't read, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    pub id: String,
    pub reason: String,
}

/// What a game's own addon store holds, addons in name order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub addons: Vec<Installed>,
    pub unreadable: Vec<Unreadable>,
}

/// The addons of a game that keeps them in its own store under `<data>/games/<id>/`, apart from F1 25's
/// profiles and build.
pub trait Addons: Sync {
    /// The game's folder in the data folder `root`.
    fn dir(&self, root: &Path) -> PathBuf;
    /// The folder the addons are installed in.
    fn addons_dir(&self, root: &Path) -> PathBuf;
    fn list(&self, root: &Path) -> Listing;
    /// Installs every addon the folder, zip or rar at `src` holds; each one installed comes back on.
    fn install(&self, root: &Path, src: &Path) -> Vec<crate::Result<Installed>>;
    fn set_enabled(&self, root: &Path, id: &str, on: bool) -> crate::Result<()>;
    fn uninstall(&self, root: &Path, id: &str) -> crate::Result<()>;
    /// The Addon Browser's catalog for the game.
    fn feed(&self, root: &Path) -> crate::catalog::Feed;
    /// Downloads `v` of `addon` from the catalog, checks it and installs it.
    fn install_listed(
        &self,
        root: &Path,
        addon: &crate::catalog::Addon,
        v: &crate::catalog::Version,
        progress: &dyn Fn(crate::catalog::requires::Event),
    ) -> crate::Result<Vec<crate::manifest::AddonManifest>>;
}

pub struct GameDef {
    pub info: &'static GameInfo,
    pub addon_kinds: &'static [Kind],
    pub online: bool,
    pub time_trial: bool,
    pub detect: &'static dyn Detect,
    pub build: Option<&'static dyn Build>,
    pub loader: Option<&'static dyn Loader>,
    pub launch: Option<&'static dyn Launch>,
    /// The game's own addon store; F1 25's addons go through its profiles and build instead.
    pub addons: Option<&'static dyn Addons>,
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
            addons: !self.addon_kinds.is_empty() || self.addons.is_some(),
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

/// The game the addon in `dir` names in its `swofty.json` `compat.game`, when it is one the launcher knows.
pub fn addon_game(dir: &Path) -> Option<GameId> {
    let text = std::fs::read(dir.join(crate::manifest::MANIFEST_FILE)).ok()?;
    let doc: serde_json::Value = serde_json::from_slice(&text).ok()?;
    GameId::parse(doc.get("compat")?.get("game")?.as_str()?)
}

/// Why the addon in `dir` is not installed for `game`: its `swofty.json` names another game the launcher knows.
pub fn other_game_refusal(dir: &Path, game: GameId) -> Option<String> {
    let made_for = addon_game(dir).filter(|g| *g != game)?;
    let text = std::fs::read(dir.join(crate::manifest::MANIFEST_FILE)).ok()?;
    let doc: serde_json::Value = serde_json::from_slice(&text).ok()?;
    let name = doc
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("this addon");
    Some(format!(
        "{name} is an addon for {}, not {}; pick {} in the bottom left of the launcher to install it",
        made_for.info().name,
        game.info().name,
        made_for.info().name
    ))
}
