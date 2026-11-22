use crate::features::browser::Browser;
use crate::services::game::{self};
use crate::services::library::Library;
use crate::shell::notices::{Notification, Notify};
use gpui_kit::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use swofty_core::store::Store;

use super::account::Account;
use super::{Page, Seen, Shell};
use swofty_i18n::tr;

pub(crate) struct Prepared {
    store: Store,
    library: Library,
    news: crate::features::news::News,
    config: swofty_core::session::Config,
    seen: Seen,
}

impl Prepared {
    pub(crate) fn load() -> Self {
        crate::app::app_log::step("opening the store");
        swofty_core::store::set_warning_sink(crate::app::app_log::note);
        let store = Store::open_default().with_builtins(swofty_builtins::all());
        let migrated = store.migrate();
        crate::app::app_log::step("store migrated");
        swofty_core::online::server::usage::launcher_heartbeats(store.root.clone());
        let seen = Seen::read(&store);
        crate::app::app_log::step("loading the library");
        let mut library = Library::load(&store);
        crate::app::app_log::step("library loaded");
        match migrated {
            Ok(notes) => notes.iter().for_each(|n| {
                eprintln!("swofty-launcher: {n}");
                crate::app::app_log::note(n);
            }),
            Err(e) => library.error = Some(e.to_string()),
        }
        let config = store.config();
        let news = crate::features::news::News::load(&store.root);
        crate::app::app_log::step("settings and news read");
        Self {
            store,
            library,
            news,
            config,
            seen,
        }
    }
}

/// How often the signed-in account is checked, so a role given or taken away shows soon.
const ACCOUNT_CHECK: Duration = Duration::from_secs(10);

/// The subscriptions and background loops that live as long as the shell.
pub(crate) struct Background {
    _shaders_events: Subscription,
    _window_moved: Subscription,
    _console_closed: Subscription,
    _poll: Task<()>,
    _live: Task<()>,
    _updates: Task<()>,
    _catalog: Task<()>,
}

impl Shell {
    pub fn new(prepared: Prepared, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Prepared {
            store,
            library,
            news,
            config,
            seen,
        } = prepared;
        swofty_i18n::set_language(config.language.as_deref());
        let scaling = crate::shell::ui_mode::Scaling::new(&config, window, cx);
        let prefs = crate::shell::prefs::Prefs::from_config(&config);
        Self::guard_close(window, cx);
        Self::recover_in_background(&store, prefs.clear_old_session_logs, cx);
        let poll = Self::poll_game(window, cx);
        Self::rediscover_periodically(cx);
        let live = Self::poll_live(cx);
        let updates = crate::features::update::Updates::watch(cx);
        let shell = cx.weak_entity();
        let console_closed = cx.on_window_closed(move |cx, id| {
            shell.update(cx, |s, cx| s.console_closed(id, cx)).ok();
        });
        let browser = Browser::new(&store.root, window, cx);
        let shaders = cx.new(|cx| crate::features::shaders::ShadersView::new(store.clone(), window, cx));
        let shaders_events = cx.subscribe(&shaders, Self::shaders_event);
        let catalog = Browser::watch(cx);
        let mp = Self::start_account(&store, cx);
        Self::watch_multiplayer(window, cx);
        let root = store.root.clone();
        let mut ads = crate::features::ads::Ads::start(window, &root);
        if let Some(note) = ads.take_notice() {
            window.notify(note, cx);
        }
        let window_moved = cx.observe_window_bounds(window, |this, _, _| this.ads.moved());
        let mut shell = Self {
            library,
            store,
            popup_timer: false,
            root_focus: cx.focus_handle(),
            online: crate::features::online::Online::new(&root),
            rooms: crate::features::multiplayer::Multiplayer::new(window, cx),
            seen,
            sliders: HashMap::new(),
            requests: None,
            console: None,
            developer: Default::default(),
            consent_check: false,
            reload_generation: 0,
            slow_logged: false,
            updates: crate::features::update::Updates::started(),
            blocks: crate::blocks::Blocks::default(),
            painted: Instant::now(),
            dock: crate::shell::download_dock::Dock::default(),
            addons: crate::features::addons::AddonsPage::new(window, cx),
            saves: crate::features::career_saves::SavesPage::default(),
            tt: crate::features::time_trial::TtPage::default(),
            store_page: crate::features::store::StorePage::default(),
            db: crate::features::db::Db::new(cx),
            news,
            news_scroll: Default::default(),
            browser,
            shaders,
            tools: Default::default(),
            settings: Default::default(),
            ads,
            game: crate::shell::probe::GameWatch::new(),
            launching: crate::shell::play::Launching::default(),
            account: crate::shell::account::Account {
                session: mp,
                busy: false,
                ban: None,
                toasts: Vec::new(),
                avatar: crate::services::avatar::load(&root),
                avatar_link: None,
            },
            prefs,
            addon_views: crate::features::sections::AddonViews::default(),
            scaling,
            crashes: crate::dialogs::crash::Crashes::default(),
            layout: crate::shell::render::Layout {
                window_width: 1440.,
                content_width: 900.,
                short: false,
            },
            _background: crate::shell::startup::Background {
                _shaders_events: shaders_events,
                _window_moved: window_moved,
                _console_closed: console_closed,
                _poll: poll,
                _live: live,
                _updates: updates,
                _catalog: catalog,
            },
            overlays: super::overlays::Overlays::default(),
            navigation: crate::shell::nav::Navigation {
                page: Page::News,
                cursor: None,
                popover: None,
                profiles_open: true,
                open_addon: None,
                highlight: None,
            },
        };
        crate::features::sections::AddonViews::refresh(&mut shell, cx);
        shell.follow_online_access(cx);
        match std::env::var("SWOFTY_LAUNCHER_OPEN_PAGE").as_deref() {
            Ok("saves") => shell.open_page(Page::Careers, cx),
            Ok("shaders") => shell.open_page(Page::Shaders, cx),
            Ok("addons") => shell.open_page(Page::Addons, cx),
            Ok("online") => shell.open_page(Page::Online, cx),
            Ok("multiplayer") => shell.open_page(Page::Multiplayer, cx),
            Ok("time-trial" | "timetrial") => shell.open_page(Page::Online, cx),
            Ok("browser") => shell.open_page(Page::Browser, cx),
            Ok("settings") => shell.open_page(Page::Settings, cx),
            Ok("donate" | "store" | "support") => shell.open_page(Page::Store, cx),
            Ok("moderation") => shell.open_page(Page::Moderation, cx),
            Ok(page) if page.starts_with("section:") => {
                let section = page.trim_start_matches("section:");
                if let Some(i) = shell
                    .library
                    .sections
                    .iter()
                    .position(|s| format!("{}/{}", s.addon, s.spec.id) == section)
                {
                    shell.open_page(Page::Section(i), cx);
                }
            }
            _ => {}
        }
        if cfg!(debug_assertions) && std::env::var_os("SWOFTY_FAKE_NOTICES").is_some() {
            cx.update_global(|n: &mut crate::shell::notices::Notices, _| {
                let now = std::time::Instant::now();
                n.add(
                    Notification::success(swofty_i18n::tr("Updated Project Halo 0.1.2"))
                        .title(swofty_i18n::tr("Updated 1 of 1 addon"))
                        .action(swofty_i18n::tr("Turn on"), |_, _| {}),
                    now,
                );
                n.add(
                    Notification::warning(swofty_i18n::tr("Turning it off warns about your saves.")),
                    now,
                );
                n.add(
                    Notification::error(swofty_i18n::tr("Couldn't reach the addon registry."))
                        .title(swofty_i18n::tr("Download failed")),
                    now,
                );
            });
        }
        crate::features::news::News::fetch(&mut shell, cx);
        shell.browser.sync_updates(&shell.library);
        Browser::fetch_catalog(&mut shell, cx);
        shell.db_profile_changed(cx);
        Self::warn_if_elevated(window, cx);
        shell
    }

    fn guard_close(window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let confirming = Rc::new(std::cell::Cell::new(false));
        window.on_window_should_close(cx, move |window, cx| {
            if shell.update(cx, |s, _| s.data_move_locked()).unwrap_or(false) {
                return false;
            }
            let pending = shell.update(cx, |s, _| s.download_pending()).unwrap_or(0);
            if pending == 0 {
                return true;
            }
            if confirming.replace(true) {
                return false;
            }
            let answer = window.prompt(
                PromptLevel::Warning,
                tr("Downloads are still in progress"),
                Some(tr(
                    "Exiting interrupts active downloads and discards queued jobs.",
                )),
                &[tr("Keep launcher open"), tr("Exit anyway")],
                cx,
            );
            let handle = window.window_handle();
            let confirming = confirming.clone();
            cx.spawn(async move |cx| {
                if answer.await.ok() == Some(1) {
                    handle.update(cx, |_, window, _| window.remove_window()).ok();
                }
                confirming.set(false);
            })
            .detach();
            false
        });
    }

    fn recover_in_background(store: &Store, clear_old_session_logs: bool, cx: &mut Context<Self>) {
        let stale = store.clone();
        cx.background_executor()
            .spawn(async move {
                match game::recover_stale(&stale) {
                    Ok(lines) => lines.iter().for_each(|l| eprintln!("swofty-launcher: {l}")),
                    Err(e) => eprintln!("swofty-launcher: could not check for an unfinished session: {e}"),
                }
                game::vault_at_start(&stale);
                if clear_old_session_logs {
                    crate::features::settings::clear_logs_at_start(&stale.root);
                }
            })
            .detach();
    }

    fn poll_game(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok((store, rediscover, seen, profile, banner_due, saves_open)) =
                    this.update(cx, |s, _| {
                        let r = s.game.take_rediscover();
                        let profile = s.library.profile.name.clone();
                        let due = r || s.saves.banner_profile != profile || game::banner_stale();
                        (
                            s.store.clone(),
                            r,
                            s.requests,
                            profile,
                            due,
                            s.navigation.page == Page::Careers,
                        )
                    })
                else {
                    break;
                };
                let probe =
                    cx.background_executor()
                        .spawn(async move {
                            game::probe(&store, rediscover, seen, &profile, banner_due, saves_open)
                        })
                        .await;
                if this
                    .update_in(cx, |s, window, cx| s.apply(probe, window, cx))
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_millis(1000)).await;
            }
        })
    }

    fn rediscover_periodically(cx: &mut Context<Self>) {
        let rediscover = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(10)).await;
                if this.update(cx, |s, _| s.game.rediscover = true).is_err() {
                    break;
                }
            }
        });
        rediscover.detach();
    }

    fn poll_live(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                if this
                    .update(cx, |s, cx| {
                        crate::features::sections::AddonViews::live_poll(s, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    fn start_account(
        store: &Store,
        cx: &mut Context<Self>,
    ) -> Option<swofty_core::online::server::sign_in::Session> {
        let mp = swofty_core::online::server::sign_in::load(&store.root);
        if let Some(current) = mp.clone() {
            let dir = store.root.clone();
            if current.allowed {
                swofty_core::online::multiplayer::client::load_in_background(&dir, &current.token);
            }
            cx.spawn(async move |this, cx| {
                let refreshed = cx
                    .background_executor()
                    .spawn(
                        async move { swofty_core::online::server::sign_in::refresh_checked(&dir, &current) },
                    )
                    .await;
                this.update(cx, |s, cx| {
                    let allowed = s.account.session.as_ref().is_some_and(|m| m.allowed);
                    Account::apply_check(s, refreshed, allowed, cx);
                })
                .ok();
                let data = this
                    .update(cx, |s, _| {
                        (s.store.clone(), s.game.install.as_ref().map(|i| i.dir.clone()))
                    })
                    .ok();
                if let Some((store, game)) = data {
                    cx.background_executor()
                        .spawn(async move {
                            swofty_core::online::timetrial::upload::flush(
                                &store.root,
                                game.as_deref(),
                                &store.config(),
                            );
                        })
                        .await;
                }
            })
            .detach();
        }
        Account::keep_checked(ACCOUNT_CHECK, cx).detach();
        mp
    }

    fn warn_if_elevated(window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(windows) {
            cx.spawn_in(window, async move |_, cx| {
                let elevated = cx
                    .background_executor()
                    .spawn(async { swofty_core::platform::run_as_admin::elevated() })
                    .await;
                if elevated {
                    let note = Notification::warning(
                        tr("Windows blocks dragging files onto a program that runs as administrator. Choose files \
                         with the launcher's buttons instead, or open Swofty Launcher normally to drag and drop."),
                    )
                    .title(tr("Running as administrator"));
                    cx.update(|window, cx| window.notify(note, cx)).ok();
                }
            })
            .detach();
        }
    }
}
