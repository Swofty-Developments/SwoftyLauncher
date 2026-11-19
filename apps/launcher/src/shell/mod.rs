use crate::services::library::Library;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Instant, SystemTime};
use swofty_core::store::Store;

pub(crate) mod account;
mod console_window;
pub(crate) mod download_dock;
pub(crate) mod host;
mod launch_alerts;
mod nav;
pub(crate) mod notices;
mod overlays;
mod page;
pub(crate) mod page_parts;
mod play;
pub(crate) mod play_bar;
mod play_dialogs;
mod prefs;
mod probe;
mod profile_edit;
pub(crate) mod profile_panel;
mod prompt;
mod reload;
mod render;
mod seen;
mod startup;
mod tabs;
mod topbar;
pub(crate) mod ui_mode;

pub(crate) use launch_alerts::offline_terms;
pub use page::{Page, PlayState};
pub(crate) use play::Launching;
pub(crate) use prompt::Prompt;
pub(crate) use seen::Seen;
pub(crate) use startup::Prepared;
pub(crate) use topbar::{BAR_HEIGHT, SUB_HEIGHT};

pub struct Shell {
    pub(crate) store: Store,
    pub(crate) popup_timer: bool,
    pub(crate) root_focus: FocusHandle,
    pub(crate) online: crate::features::online::Online,
    pub(crate) library: Library,
    pub(crate) rooms: crate::features::multiplayer::Multiplayer,
    pub(crate) seen: Seen,
    pub(crate) sliders: HashMap<String, SliderSlot>,
    pub(crate) requests: Option<SystemTime>,
    pub(crate) console: Option<AnyWindowHandle>,
    pub(crate) developer: crate::features::developer::Developer,
    /// The check at start for an unanswered ad consent form is running.
    pub(crate) consent_check: bool,
    reload_generation: u64,
    slow_logged: bool,
    pub(crate) updates: crate::features::update::Updates,
    pub(crate) blocks: crate::blocks::Blocks,
    painted: Instant,
    pub(crate) dock: crate::shell::download_dock::Dock,
    pub(crate) addons: crate::features::addons::AddonsPage,
    pub(crate) saves: crate::features::career_saves::SavesPage,
    pub(crate) tt: crate::features::time_trial::TtPage,
    pub(crate) store_page: crate::features::store::StorePage,
    pub(crate) db: crate::features::db::Db,
    pub(crate) news: crate::features::news::News,
    pub(crate) news_scroll: crate::features::news::NewsScroll,
    pub(crate) browser: crate::features::browser::Browser,
    pub(crate) shaders: Entity<crate::features::shaders::ShadersView>,
    pub(crate) tools: crate::features::tools::Carousel,
    pub(crate) settings: crate::features::settings::Settings,
    pub(crate) ads: crate::features::ads::Ads,
    pub(crate) game: crate::shell::probe::GameWatch,
    pub(crate) launching: crate::shell::play::Launching,
    pub(crate) account: crate::shell::account::Account,
    pub(crate) prefs: crate::shell::prefs::Prefs,
    pub(crate) addon_views: crate::features::sections::AddonViews,
    pub(crate) scaling: crate::shell::ui_mode::Scaling,
    pub(crate) crashes: crate::dialogs::crash::Crashes,
    pub(crate) layout: crate::shell::render::Layout,
    pub(crate) _background: crate::shell::startup::Background,
    pub(crate) overlays: overlays::Overlays,
    pub(crate) navigation: crate::shell::nav::Navigation,
}

pub struct SliderSlot {
    pub state: Entity<SliderState>,
    pub synced: usize,
    pub _changes: Subscription,
}
