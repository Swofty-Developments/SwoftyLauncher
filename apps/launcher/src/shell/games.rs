//! The game the launcher shows, picked in the bottom left of the play bar. F1 25 is the shell as it always was;
//! every other game is a [`Side`]: its copies, its own addon store and its Play, all through the services on its
//! `GameDef`, so a new game needs a `GameDef` and nothing here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use swofty_core::game::{GameId, StoreId};
use swofty_core::games::{self, Addons, Found, GameDef, Listing, Started};
use swofty_core::session::Config;
use swofty_i18n::{tr, trf};

use crate::features::browser::Browser;
use crate::services::library::{AddonItem, Library, Source};
use crate::shell::notices::{Notification, Notify};
use crate::shell::tabs::Tab;
use crate::shell::{Page, Shell};

/// How often a running game is checked for having closed.
const TICK: Duration = Duration::from_secs(1);
/// How often the copies of the games are looked for again.
const REDISCOVER: Duration = Duration::from_secs(20);

/// A game the launcher started and follows until it closes.
pub(crate) struct Run {
    pub(crate) started: Started,
    pub(crate) stopping: bool,
}

/// How the last session of a game ended.
#[derive(Clone)]
pub(crate) struct Ended {
    pub(crate) session: PathBuf,
    pub(crate) failed: Vec<(String, String)>,
}

/// Where Play stands for a [`Side`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SideState {
    Detecting,
    NotFound,
    NoCore,
    Ready,
    Starting,
    Running,
    Stopping,
}

/// A game whose copies, addons and Play the launcher runs through its `GameDef`.
pub(crate) struct Side {
    pub(crate) def: &'static GameDef,
    pub(crate) shelf: &'static dyn Addons,
    pub(crate) copies: Vec<Found>,
    pub(crate) detected: bool,
    pub(crate) picked: Option<PathBuf>,
    pub(crate) listing: Listing,
    /// The addons as the Addon Browser reads a library.
    pub(crate) library: Library,
    pub(crate) core: Option<PathBuf>,
    pub(crate) run: Option<Run>,
    pub(crate) starting: bool,
    pub(crate) notice: Option<String>,
    pub(crate) ended: Option<Ended>,
}

impl Side {
    fn new(def: &'static GameDef, shelf: &'static dyn Addons, root: &Path, config: &Config) -> Self {
        let mut side = Self {
            def,
            shelf,
            copies: Vec::new(),
            detected: false,
            picked: config.game_copies.get(def.info.id).cloned(),
            listing: Listing::default(),
            library: Library::of_game(def.info.id, Vec::new()),
            core: def.loader.and_then(|l| l.core_dir()),
            run: None,
            starting: false,
            notice: None,
            ended: None,
        };
        side.reload(root);
        side
    }

    pub(crate) fn game(&self) -> GameId {
        self.def.info.game
    }

    /// Reads the installed addons again.
    pub(crate) fn reload(&mut self, root: &Path) {
        self.listing = self.shelf.list(root);
        let items = self
            .listing
            .addons
            .iter()
            .map(|a| AddonItem {
                manifest: a.manifest.clone(),
                enabled: a.enabled,
                locked: false,
                icon: None,
                track: false,
                package: false,
                source: Source::Installed,
                left_out: Vec::new(),
            })
            .collect();
        self.library = Library::of_game(self.def.info.id, items);
    }

    /// The copy Play uses: the one the player picked while it is there, else the first one found.
    pub(crate) fn copy(&self) -> Option<&Found> {
        self.picked
            .as_ref()
            .and_then(|p| self.copies.iter().find(|c| &c.dir == p))
            .or_else(|| self.copies.first())
    }

    pub(crate) fn on(&self) -> usize {
        self.listing.addons.iter().filter(|a| a.enabled).count()
    }

    pub(crate) fn state(&self) -> SideState {
        match &self.run {
            Some(run) if run.stopping => SideState::Stopping,
            Some(_) => SideState::Running,
            None if self.starting => SideState::Starting,
            None if !self.detected => SideState::Detecting,
            None if self.copy().is_none() => SideState::NotFound,
            None if self.core.is_none() => SideState::NoCore,
            None => SideState::Ready,
        }
    }

    /// Takes a finished run: the files it placed are taken out and what the core reported is kept.
    fn reap(&mut self) -> Option<std::process::ExitStatus> {
        let run = self.run.as_mut()?;
        let status = run.started.child.try_wait().ok().flatten()?;
        let run = self.run.take()?;
        run.started.finish();
        let status_file = swofty_core::games::overcooked_2::session::read_status(&run.started.session);
        self.ended = Some(Ended {
            failed: status_file
                .map(|s| s.failed.into_iter().map(|f| (f.id, f.error)).collect())
                .unwrap_or_default(),
            session: run.started.session,
        });
        Some(status)
    }
}

/// The badge text a copy's store shows.
pub(crate) fn store_label(store: Option<StoreId>) -> &'static str {
    match store {
        Some(StoreId::Steam { .. }) => "STEAM",
        Some(StoreId::Epic { .. }) => "EPIC",
        Some(StoreId::EaApp { .. }) => "EA APP",
        None => tr("FOLDER"),
    }
}

pub(crate) struct Games {
    pub(crate) active: GameId,
    pub(crate) sides: Vec<Side>,
    /// The Addon Browser of the game not shown, kept while the other one is.
    pub(crate) parked: Option<Browser>,
    pub(crate) art: HashMap<GameId, Option<Arc<Image>>>,
}

/// The tabs a game has: F1 25 has every one; a game with its own addon store has Home, Library, Discover and
/// Donate.
pub(crate) fn tabs_of(game: GameId) -> &'static [Tab] {
    match games::def(game).addons {
        None => &Tab::ALL,
        Some(_) => &[Tab::Home, Tab::Library, Tab::Discover, Tab::Store],
    }
}

/// Whether a game has `page`: F1 25 has every page; a game with its own addon store has the news, its addons,
/// its Addon Browser and the launcher's own pages.
pub(crate) fn has_page(game: GameId, page: Page) -> bool {
    match games::def(game).addons {
        None => true,
        Some(_) => matches!(
            page,
            Page::News | Page::Addons | Page::Browser | Page::Downloads | Page::Store | Page::Settings
        ),
    }
}

/// Where `page` lands for `game`: the page itself when the game has it, else its tab's home, else Home.
pub(crate) fn page_for(game: GameId, page: Page) -> Page {
    if has_page(game, page) {
        return page;
    }
    page.tab()
        .filter(|t| tabs_of(game).contains(t))
        .map(Tab::home)
        .filter(|p| has_page(game, *p))
        .unwrap_or(Page::News)
}

/// Steam's library art for the app `app_id` in the Steam roots, when Steam has cached it.
fn steam_art(app_id: u32) -> Option<Arc<Image>> {
    let names = ["library_600x900.jpg", "library_capsule.jpg", "header.jpg"];
    for root in swofty_core::platform::steam::steam_roots() {
        let dir = root.join("appcache/librarycache").join(app_id.to_string());
        let mut dirs = vec![dir.clone()];
        dirs.extend(
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir()),
        );
        for name in names {
            if let Some(bytes) = dirs.iter().find_map(|d| std::fs::read(d.join(name)).ok()) {
                return Some(Arc::new(Image::from_bytes(ImageFormat::Jpeg, bytes)));
            }
        }
    }
    None
}

impl Games {
    pub(crate) fn new(root: &Path, config: &Config) -> Self {
        let sides: Vec<Side> = games::ALL
            .iter()
            .filter(|d| d.caps().play)
            .filter_map(|d| d.addons.map(|shelf| Side::new(d, shelf, root, config)))
            .collect();
        let active = config
            .game
            .as_deref()
            .and_then(GameId::parse)
            .filter(|g| *g == GameId::F1_25 || sides.iter().any(|s| s.game() == *g))
            .unwrap_or(GameId::F1_25);
        let art = sides
            .iter()
            .map(|s| (s.game(), s.def.info.steam_app_id().and_then(steam_art)))
            .collect();
        Self {
            active,
            sides,
            parked: None,
            art,
        }
    }

    /// The games the picker offers: F1 25, then every game that can Play through its own services.
    pub(crate) fn offered(&self) -> Vec<GameId> {
        std::iter::once(GameId::F1_25)
            .chain(self.sides.iter().map(Side::game))
            .collect()
    }

    pub(crate) fn side(&self) -> Option<&Side> {
        self.side_of(self.active)
    }

    pub(crate) fn side_of(&self, game: GameId) -> Option<&Side> {
        self.sides.iter().find(|s| s.game() == game)
    }

    pub(crate) fn side_mut(&mut self) -> Option<&mut Side> {
        let active = self.active;
        self.sides.iter_mut().find(|s| s.game() == active)
    }

    pub(crate) fn tabs(&self) -> &'static [Tab] {
        tabs_of(self.active)
    }
}

impl Shell {
    pub(crate) fn game_tabs(&self) -> &'static [Tab] {
        self.games.tabs()
    }

    /// The library the Addon Browser compares the catalog with: the active game's.
    pub(crate) fn browser_library(&self) -> &Library {
        match self.games.side() {
            Some(side) => &side.library,
            None => &self.library,
        }
    }

    /// Shows `game`: its tabs, its Addon Browser and its play bar. Picking F1 25 gives back the F1 25 launcher.
    pub(crate) fn pick_game(&mut self, game: GameId, window: &mut Window, cx: &mut Context<Self>) {
        if game == self.games.active {
            return;
        }
        if self.browser.busy() {
            window.notify(
                Notification::info(tr(
                    "The Addon Browser is still working; pick the game again when it has finished.",
                )),
                cx,
            );
            return;
        }
        let root = self.store.root.clone();
        let next = match self.games.parked.take() {
            Some(b) if b.game == game => b,
            parked => {
                self.games.parked = parked;
                Browser::new(&root, game, window, cx)
            }
        };
        let shown = std::mem::replace(&mut self.browser, next);
        self.games.parked = Some(shown);
        self.games.active = game;
        if let Some(side) = self.games.side_mut() {
            side.reload(&root);
        }
        self.browser.sync_updates(match self.games.side() {
            Some(side) => &side.library,
            None => &self.library,
        });
        let mut config = self.store.config();
        config.game = (game != GameId::F1_25).then(|| game.id().to_owned());
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save the game picked").failed(&e);
        }
        crate::app::app_log::note(&format!("games: showing {}", game.info().name));
        let page = page_for(game, self.navigation.page);
        self.navigation.page = Page::News;
        self.navigation.cursor = None;
        self.navigation.open_addon = None;
        self.open_page(page, cx);
        self.rediscover_games(cx);
        cx.notify();
    }

    /// Looks for every side game's copies again, off the UI thread.
    pub(crate) fn rediscover_games(&mut self, cx: &mut Context<Self>) {
        let config = self.store.config();
        let defs: Vec<&'static GameDef> = self.games.sides.iter().map(|s| s.def).collect();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let probe = swofty_core::games::Probe::here(&config);
                    defs.iter()
                        .map(|d| (d.info.game, d.find(&probe), d.loader.and_then(|l| l.core_dir())))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                for (game, copies, core) in found {
                    if let Some(side) = this.games.sides.iter_mut().find(|s| s.game() == game) {
                        side.copies = copies;
                        side.core = core;
                        side.detected = true;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Follows the games the launcher started and looks for copies now and then.
    pub(crate) fn watch_games(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut since = Instant::now();
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this.update(cx, |this, cx| {
                    for side in &mut this.games.sides {
                        if let Some(status) = side.reap() {
                            crate::app::app_log::note(&format!(
                                "games: {} closed ({status})",
                                side.def.info.name
                            ));
                            cx.notify();
                        }
                    }
                    if since.elapsed() >= REDISCOVER && this.games.side().is_some() {
                        since = Instant::now();
                        this.rediscover_games(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    /// Play for the side game: writes the session and starts the game with its core, off the UI thread.
    pub(crate) fn play_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        let Some(side) = self.games.side_mut() else {
            return;
        };
        if side.state() != SideState::Ready {
            return;
        }
        let (Some(launch), Some(copy)) = (side.def.launch, side.copy().map(|c| c.dir.clone())) else {
            return;
        };
        side.starting = true;
        side.notice = None;
        let game = side.game();
        let name = side.def.info.name;
        crate::app::app_log::note(&format!("play: starting {name} in {}", copy.display()));
        cx.spawn_in(window, async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move { launch.start(&root, &copy) })
                .await;
            this.update_in(cx, |this, window, cx| {
                let Some(side) = this.games.sides.iter_mut().find(|s| s.game() == game) else {
                    return;
                };
                side.starting = false;
                match started {
                    Ok(started) => {
                        crate::app::app_log::note(&format!(
                            "play: {name} started, pid {}, session {}",
                            started.pid(),
                            started.session.display()
                        ));
                        side.run = Some(Run {
                            started,
                            stopping: false,
                        });
                    }
                    Err(e) => {
                        crate::app::app_log::action(&format!("start {name}")).failed(&e);
                        side.notice = Some(e.to_string());
                        window.notify(
                            Notification::error(e.to_string())
                                .title(trf!("Couldn't start {game}", game = name)),
                            cx,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Stop: asks the side game's own process to close.
    pub(crate) fn stop_side(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.games.side_mut().and_then(|s| s.run.as_mut()) else {
            return;
        };
        let pid = run.started.game_pid();
        run.stopping = true;
        crate::app::app_log::note(&format!("play: asking pid {pid} to close"));
        if let Err(e) = swofty_core::platform::process::stop_process(pid) {
            crate::app::app_log::action("stop the game").failed(&e);
            run.stopping = false;
        }
        cx.notify();
    }

    /// Plays the side game's copy at `dir` from now on.
    pub(crate) fn pick_side_copy(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let Some(side) = self.games.side_mut() else {
            return;
        };
        side.picked = Some(dir.clone());
        let id = side.def.info.id;
        let mut config = self.store.config();
        config.game_copies.insert(id.to_owned(), dir);
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save the copy picked").failed(&e);
        }
        cx.notify();
    }

    pub(crate) fn reload_side(&mut self, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        if let Some(side) = self.games.side_mut() {
            side.reload(&root);
        }
        if let Some(side) = self.games.side() {
            self.browser.sync_updates(&side.library);
        }
        cx.notify();
    }

    pub(crate) fn set_side_addon(&mut self, id: &str, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        let Some(side) = self.games.side() else {
            return;
        };
        if let Err(e) = side.shelf.set_enabled(&root, id, on) {
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.reload_side(cx);
    }

    pub(crate) fn uninstall_side_addon(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        let Some(side) = self.games.side() else {
            return;
        };
        let name = side
            .listing
            .addons
            .iter()
            .find(|a| a.manifest.id == id)
            .map_or_else(|| id.to_owned(), |a| a.manifest.name.clone());
        match side.shelf.uninstall(&root, id) {
            Ok(()) => window.notify(Notification::info(trf!("Removed {name}.", name = name)), cx),
            Err(e) => window.notify(Notification::error(e.to_string()), cx),
        }
        self.reload_side(cx);
    }

    /// Installs the files and folders dropped or picked into the side game's store, off the UI thread.
    pub(crate) fn install_side(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.dropping = false;
        let root = self.store.root.clone();
        let Some(side) = self.games.side() else {
            return;
        };
        let shelf = side.shelf;
        let name = side.def.info.name;
        cx.spawn_in(window, async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .iter()
                        .flat_map(|p| shelf.install(&root, p))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                for result in results {
                    match result {
                        Ok(a) => window.notify(
                            Notification::success(trf!(
                                "Installed {addon} {version} for {game}; it is on.",
                                addon = a.manifest.name,
                                version = a.manifest.version,
                                game = name
                            )),
                            cx,
                        ),
                        Err(e) => window.notify(
                            Notification::error(e.to_string()).title(tr("Couldn't install the addon")),
                            cx,
                        ),
                    }
                }
                this.reload_side(cx);
                if this.navigation.page != Page::Addons {
                    this.open_page(Page::Addons, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn pick_side_addons(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: true,
            prompt: Some(tr("Install").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && !paths.is_empty()
            {
                this.update_in(cx, |s, window, cx| s.install_side(paths, window, cx))
                    .ok();
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Games, SideState, has_page, page_for, store_label, tabs_of};
    use crate::shell::Page;
    use crate::shell::tabs::Tab;
    use swofty_core::game::{GameId, StoreId};
    use swofty_core::session::Config;

    #[test]
    fn f1_25_keeps_every_tab_and_page() {
        assert_eq!(tabs_of(GameId::F1_25), Tab::ALL);
        for page in [
            Page::News,
            Page::Addons,
            Page::Shaders,
            Page::Importer,
            Page::Tools,
            Page::Careers,
            Page::Database,
            Page::Multiplayer,
            Page::Online,
            Page::Moderation,
            Page::Section(2),
        ] {
            assert!(has_page(GameId::F1_25, page));
            assert!(page_for(GameId::F1_25, page) == page);
        }
    }

    #[test]
    fn overcooked_2_has_home_library_discover_and_donate_and_no_f1_pages() {
        assert_eq!(
            tabs_of(GameId::Overcooked2),
            [Tab::Home, Tab::Library, Tab::Discover, Tab::Store]
        );
        for page in [
            Page::News,
            Page::Addons,
            Page::Browser,
            Page::Store,
            Page::Settings,
        ] {
            assert!(has_page(GameId::Overcooked2, page));
        }
        for page in [
            Page::Shaders,
            Page::Importer,
            Page::Tools,
            Page::Careers,
            Page::Database,
            Page::Multiplayer,
            Page::Online,
            Page::Moderation,
            Page::Section(0),
        ] {
            assert!(!has_page(GameId::Overcooked2, page));
        }
        assert!(page_for(GameId::Overcooked2, Page::Shaders) == Page::Addons);
        assert!(page_for(GameId::Overcooked2, Page::Section(1)) == Page::Addons);
        assert!(page_for(GameId::Overcooked2, Page::Multiplayer) == Page::News);
        assert!(page_for(GameId::Overcooked2, Page::Browser) == Page::Browser);
        for tab in tabs_of(GameId::Overcooked2) {
            assert!(has_page(GameId::Overcooked2, tab.home()));
        }
    }

    #[test]
    fn a_copy_names_its_store() {
        assert_eq!(store_label(Some(StoreId::Steam { app_id: 728_880 })), "STEAM");
        assert_eq!(
            store_label(Some(StoreId::Epic {
                exe: "Overcooked2.exe"
            })),
            "EPIC"
        );
        assert_eq!(store_label(None), "FOLDER");
    }

    #[test]
    fn the_picked_game_is_read_from_the_config_and_unknown_ones_fall_back_to_f1_25() {
        let root = std::env::temp_dir().join(format!("swofty-games-axis-{}", std::process::id()));
        let mut config = Config::default();
        assert_eq!(Games::new(&root, &config).active, GameId::F1_25);
        config.game = Some("overcooked_2".into());
        let games = Games::new(&root, &config);
        assert_eq!(games.active, GameId::Overcooked2);
        assert_eq!(games.offered(), [GameId::F1_25, GameId::Overcooked2]);
        assert!(games.side().is_some_and(|s| s.state() == SideState::Detecting));
        config.game = Some("minecraft".into());
        assert_eq!(Games::new(&root, &config).active, GameId::F1_25);
        std::fs::remove_dir_all(&root).ok();
    }
}
