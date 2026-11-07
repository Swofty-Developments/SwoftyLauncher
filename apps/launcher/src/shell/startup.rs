use crate::game::{self};
use crate::library::Library;
use crate::play_checks;
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};
use swofty_core::store::Store;

use super::{Page, Seen, Shell};
use swofty_i18n::tr;

pub(crate) struct Prepared {
    store: Store,
    library: Library,
    news: crate::news::News,
    config: swofty_core::session::Config,
    seen: Seen,
}

impl Prepared {
    pub(crate) fn load() -> Self {
        crate::app_log::step("opening the store");
        swofty_core::store::set_warning_sink(crate::app_log::note);
        let store = Store::open_default().with_builtins(swofty_builtins::all());
        let migrated = store.migrate();
        crate::app_log::step("store migrated");
        swofty_core::multiplayer::diag::open(&store.root);
        swofty_core::usage::launcher_heartbeats(store.root.clone());
        let seen = Seen::read(&store);
        crate::app_log::step("loading the library");
        let mut library = Library::load(&store);
        crate::app_log::step("library loaded");
        match migrated {
            Ok(notes) => notes.iter().for_each(|n| {
                eprintln!("swofty-launcher: {n}");
                crate::app_log::note(n);
            }),
            Err(e) => library.error = Some(e.to_string()),
        }
        let config = store.config();
        let news = crate::news::News::load(&store.root);
        crate::app_log::step("settings and news read");
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
        let vr_profiles = config.vr_profiles;
        let start_store = config.launch.start_store;
        let ui_mode = config.ui_mode;
        let developer_mode = config.developer;
        let clear_old_session_logs = config.clear_old_session_logs;
        let update_check = config.updates.check;
        Self::guard_close(window, cx);
        Self::recover_in_background(&store, clear_old_session_logs, cx);
        let poll = Self::poll_game(window, cx);
        Self::rediscover_periodically(cx);
        let live = Self::poll_live(cx);
        let updates = Self::watch_updates(cx);
        let shell = cx.weak_entity();
        let console_closed = cx.on_window_closed(move |cx, id| {
            shell.update(cx, |s, cx| s.console_closed(id, cx)).ok();
        });
        let browser = crate::browser::Browser::new(&store.root, window, cx);
        let shaders = cx.new(|cx| crate::shaders::ShadersView::new(store.clone(), window, cx));
        let shaders_events = cx.subscribe(&shaders, Self::shaders_event);
        let catalog = Self::watch_catalog(cx);
        let mp = Self::start_account(&store, cx);
        Self::watch_multiplayer(window, cx);
        let mut shell = Self {
            library,
            store,
            page: Page::News,
            open_addon: None,
            install: None,
            copies: Vec::new(),
            missing_copies: Vec::new(),
            detected: false,
            rediscover: true,
            processes: Vec::new(),
            owners: Vec::new(),
            session: None,
            mp,
            mp_busy: false,
            mp_toasts: Vec::new(),
            mp_page: crate::multiplayer::Multiplayer::new(window, cx),
            crash: None,
            crash_view: None,
            crash_open: false,
            dismissed: None,
            steam_saves_once: false,
            launch: None,
            notice: None,
            play_anyway: false,
            play_pending: play_checks::Pending::default(),
            blocked: None,
            game_edited: None,
            seen,
            restoring: false,
            build: None,
            cancelling: false,
            sliders: HashMap::new(),
            requests: None,
            console: None,
            developer: Default::default(),
            views: HashMap::new(),
            views_profile: String::new(),
            view_gen: 0,
            view_gens: HashMap::new(),
            busy: HashSet::new(),
            open_row: None,
            progress: None,
            tasks: None,
            moving: None,
            found_data: None,
            dropping: false,
            vr_profiles,
            start_store,
            ui_mode,
            onboarding: None,
            developer_mode,
            clear_old_session_logs,
            reload_generation: 0,
            slow_logged: false,
            update_check,
            updates: crate::update::Updates::new(),
            blocks: crate::blocks::Blocks::default(),
            content_width: 900.,
            short: false,
            ticking: false,
            painted: Instant::now(),
            dock: crate::download_dock::Dock::default(),
            dock_wake: false,
            live_n: 0,
            highlight: None,
            addons: crate::addons::AddonsPage::new(window, cx),
            saves: crate::career_saves::SavesPage::default(),
            db: crate::db::Db::new(cx),
            news,
            news_scroll: Default::default(),
            browser,
            shaders,
            _shaders_events: shaders_events,
            tools: Default::default(),
            settings: Default::default(),
            _console_closed: console_closed,
            _poll: poll,
            _live: live,
            _updates: updates,
            _catalog: catalog,
        };
        shell.refresh_views(cx);
        match std::env::var("SWOFTY_LAUNCHER_OPEN_PAGE").as_deref() {
            Ok("saves") => shell.open_page(Page::Careers, cx),
            Ok("shaders") => shell.open_page(Page::Shaders, cx),
            Ok("addons") => shell.open_page(Page::Addons, cx),
            _ => {}
        }
        shell.fetch_news(cx);
        shell.sync_catalog_updates();
        shell.fetch_catalog(cx);
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
                    crate::settings::clear_logs_at_start(&stale.root);
                }
            })
            .detach();
    }

    fn poll_game(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok((store, rediscover, seen, profile, banner_due, saves_open)) =
                    this.update(cx, |s, _| {
                        let r = std::mem::take(&mut s.rediscover);
                        let profile = s.library.profile.name.clone();
                        let due = r || s.saves.banner_profile != profile || game::banner_stale();
                        (
                            s.store.clone(),
                            r,
                            s.requests,
                            profile,
                            due,
                            s.page == Page::Careers,
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
                if this.update(cx, |s, _| s.rediscover = true).is_err() {
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
                if this.update(cx, |s, cx| s.live_poll(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn start_account(store: &Store, cx: &mut Context<Self>) -> Option<swofty_core::mpauth::Session> {
        let mp = swofty_core::mpauth::load(&store.root);
        if let Some(current) = mp.clone() {
            let dir = store.root.clone();
            cx.spawn(async move |this, cx| {
                let refreshed = cx
                    .background_executor()
                    .spawn(async move { swofty_core::mpauth::refresh(&dir, &current) })
                    .await;
                this.update(cx, |s, cx| {
                    s.mp = refreshed;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        let mp_poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(ACCOUNT_CHECK).await;
                let current =
                    match this.update(cx, |s, _| s.mp.clone().map(|sess| (s.store.root.clone(), sess))) {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                let Some((dir, session)) = current else {
                    continue;
                };
                let was_allowed = session.allowed;
                let refreshed = cx
                    .background_executor()
                    .spawn(async move { swofty_core::mpauth::refresh(&dir, &session) })
                    .await;
                if this
                    .update(cx, |s, cx| s.mp_apply_refresh(refreshed, was_allowed, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        mp_poll.detach();
        mp
    }

    fn warn_if_elevated(window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(windows) {
            cx.spawn_in(window, async move |_, cx| {
                let elevated = cx
                    .background_executor()
                    .spawn(async { swofty_core::compat::elevated() })
                    .await;
                if elevated {
                    let note = Notification::warning(
                        tr("Windows blocks dragging files onto a program that runs as administrator. Choose files \
                         with the launcher's buttons instead, or open Swofty Launcher normally to drag and drop."),
                    )
                    .title(tr("Running as administrator"));
                    cx.update(|window, cx| window.push_notification(note, cx)).ok();
                }
            })
            .detach();
        }
    }
}
