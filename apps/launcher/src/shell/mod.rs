use crate::crash_modal::CrashView;
use crate::game::{Install, Launch, Owner};
use crate::game_edited::Edited;
use crate::library::Library;
use crate::play_checks;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use swofty_core::busy::Busy;
use swofty_core::crash::StartFailure;
use swofty_core::process::GameProcess;
use swofty_core::recovery::Blocked;
use swofty_core::session::{SessionRecord, UiMode};
use swofty_core::store::Store;
use swofty_core::view::View;

mod config;
mod console_window;
mod dialogs;
mod nav;
mod page;
mod play;
mod play_dialogs;
mod probe;
mod profile_edit;
mod prompt;
mod reload;
mod render;
mod seen;
mod startup;
mod title_bar;

pub use page::{Page, PlayState};
pub(crate) use prompt::Prompt;
pub(crate) use seen::Seen;
pub(crate) use startup::Prepared;

pub struct Shell {
    pub(crate) store: Store,
    pub(crate) page: Page,
    pub(crate) library: Library,
    pub(crate) open_addon: Option<String>,
    pub(crate) install: Option<Install>,
    pub(crate) copies: Vec<Install>,
    pub(crate) missing_copies: Vec<PathBuf>,
    pub(crate) detected: bool,
    pub(crate) rediscover: bool,
    pub(crate) processes: Vec<GameProcess>,
    pub(crate) owners: Vec<Owner>,
    pub(crate) session: Option<SessionRecord>,
    pub(crate) mp: Option<swofty_core::mpauth::Session>,
    pub(crate) mp_busy: bool,
    pub(crate) mp_toasts: Vec<Notification>,
    pub(crate) mp_page: crate::multiplayer::Multiplayer,
    pub(crate) crash: Option<(PathBuf, Result<String, StartFailure>)>,
    /// The newest crash's diagnosis, and whether its modal is open.
    pub(crate) crash_view: Option<CrashView>,
    pub(crate) crash_open: bool,
    pub(crate) dismissed: Option<PathBuf>,
    pub(crate) steam_saves_once: bool,
    pub(crate) launch: Option<Launch>,
    pub(crate) notice: Option<String>,
    play_anyway: bool,
    play_pending: play_checks::Pending,
    pub(crate) blocked: Option<Blocked>,
    pub(crate) game_edited: Option<Edited>,
    pub(crate) seen: Seen,
    pub(crate) restoring: bool,
    pub(crate) build: Option<Busy>,
    pub(crate) cancelling: bool,
    pub(crate) sliders: HashMap<String, SliderSlot>,
    pub(crate) requests: Option<SystemTime>,
    pub(crate) console: Option<AnyWindowHandle>,
    pub(crate) developer: crate::developer::Developer,
    pub(crate) views: HashMap<String, Result<Arc<View>, String>>,
    pub(crate) views_profile: String,
    pub(crate) view_gen: u64,
    pub(crate) view_gens: HashMap<String, u64>,
    pub(crate) busy: HashSet<String>,
    pub(crate) open_row: Option<String>,
    pub(crate) progress: Option<(f32, String)>,
    pub(crate) tasks: Option<crate::tasks::TaskDialog>,
    pub(crate) moving: Option<crate::data_move::Moving>,
    pub(crate) found_data: Option<crate::found_data::FoundPrompt>,
    pub(crate) dropping: bool,
    pub(crate) vr_profiles: BTreeSet<String>,
    pub(crate) start_store: bool,
    pub(crate) ui_mode: Option<UiMode>,
    pub(crate) onboarding: Option<crate::onboarding::Onboarding>,
    pub(crate) developer_mode: bool,
    /// `clear_old_session_logs` from config.json: opt-in, old session logs are cleared at start.
    pub(crate) clear_old_session_logs: bool,
    reload_generation: u64,
    slow_logged: bool,
    /// `updates.check` from config.json, read when the library loads.
    pub(crate) update_check: bool,
    pub(crate) updates: crate::update::Updates,
    pub(crate) blocks: crate::blocks::Blocks,
    pub(crate) content_width: f32,
    pub(crate) short: bool,
    pub(crate) ticking: bool,
    painted: Instant,
    pub(crate) dock: crate::download_dock::Dock,
    pub(crate) dock_wake: bool,
    pub(crate) live_n: u64,
    /// The addon the Addons page highlights, and since when.
    pub(crate) highlight: Option<(String, Instant)>,
    pub(crate) addons: crate::addons::AddonsPage,
    pub(crate) saves: crate::career_saves::SavesPage,
    pub(crate) db: crate::db::Db,
    pub(crate) news: crate::news::News,
    pub(crate) news_scroll: crate::news::NewsScroll,
    pub(crate) browser: crate::browser::Browser,
    pub(crate) shaders: Entity<crate::shaders::ShadersView>,
    _shaders_events: Subscription,
    pub(crate) tools: crate::tools::Carousel,
    pub(crate) settings: crate::settings::Settings,
    _console_closed: Subscription,
    _poll: Task<()>,
    _live: Task<()>,
    _updates: Task<()>,
    _catalog: Task<()>,
}

pub struct SliderSlot {
    pub state: Entity<SliderState>,
    pub synced: usize,
    pub _changes: Subscription,
}

impl Shell {
    pub(super) fn mp_sign_in(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mp_busy || self.mp.is_some() {
            return;
        }
        let (pending, url) = match swofty_core::mpauth::begin() {
            Ok(v) => v,
            Err(e) => {
                self.mp_toasts
                    .push(Notification::error(format!("Sign-in failed: {e}")));
                cx.notify();
                return;
            }
        };
        self.mp_busy = true;
        cx.open_url(&url);
        let dir = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { pending.wait(&dir) })
                .await;
            this.update(cx, |s, cx| {
                s.mp_busy = false;
                match result {
                    Ok(session) => {
                        s.mp_toasts
                            .push(Notification::success(format!("Signed in as {}", session.name)));
                        if session.allowed {
                            s.mp_toasts.push(Notification::success(
                                "Your account has been granted access to: Multiplayer",
                            ));
                        }
                        s.mp = Some(session);
                    }
                    Err(e) => s
                        .mp_toasts
                        .push(Notification::error(format!("Sign-in failed: {e}"))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn mp_sign_out(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.mp.take() else {
            return;
        };
        self.mp_leave(cx);
        let name = session.name.clone();
        let dir = self.store.root.clone();
        cx.background_executor()
            .spawn(async move { swofty_core::mpauth::sign_out(&dir, &session) })
            .detach();
        self.mp_toasts
            .push(Notification::info(format!("Signed out {name}")));
        cx.notify();
    }

    pub(super) fn mp_apply_refresh(
        &mut self,
        refreshed: Option<swofty_core::mpauth::Session>,
        was_allowed: bool,
        cx: &mut Context<Self>,
    ) {
        match refreshed {
            Some(session) => {
                let gained = session.allowed && !was_allowed;
                self.mp = Some(session);
                if gained {
                    self.mp_toasts.push(Notification::success(
                        "Your account has been granted access to: Multiplayer",
                    ));
                }
            }
            None => self.mp = None,
        }
        cx.notify();
    }
}
