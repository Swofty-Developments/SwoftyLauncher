use crate::services::library::Library;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Instant, SystemTime};
use swofty_core::store::Store;

mod account;
mod barfit;
mod config;
mod console_window;
pub(crate) mod download_dock;
mod launch_alerts;
mod nav;
pub(crate) mod notices;
mod overflow;
mod overlays;
mod page;
pub(crate) mod page_parts;
mod play;
pub(crate) mod play_bar;
mod play_dialogs;
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

pub use page::{Page, PlayState};
pub(crate) use prompt::Prompt;
pub(crate) use seen::Seen;
pub(crate) use startup::Prepared;
pub(crate) use topbar::{BAR_HEIGHT, SUB_HEIGHT};

pub struct Shell {
    pub(crate) store: Store,
    pub(crate) page: Page,
    pub(crate) nav: Option<tabs::Cursor>,
    pub(crate) popover: Option<topbar::Popover>,
    pub(crate) popup_timer: bool,
    pub(crate) root_focus: FocusHandle,
    pub(crate) profiles_open: bool,
    pub(crate) online: crate::features::online::Online,
    pub(crate) library: Library,
    pub(crate) open_addon: Option<String>,
    pub(crate) mp_page: crate::features::multiplayer::Multiplayer,
    pub(crate) seen: Seen,
    pub(crate) sliders: HashMap<String, SliderSlot>,
    pub(crate) requests: Option<SystemTime>,
    pub(crate) console: Option<AnyWindowHandle>,
    pub(crate) developer: crate::features::developer::Developer,
    reload_generation: u64,
    slow_logged: bool,
    pub(crate) updates: crate::features::update::Updates,
    pub(crate) blocks: crate::blocks::Blocks,
    pub(crate) ticking: bool,
    painted: Instant,
    pub(crate) dock: crate::shell::download_dock::Dock,
    pub(crate) live_n: u64,
    /// The addon the Addons page highlights, and since when.
    pub(crate) highlight: Option<(String, Instant)>,
    pub(crate) addons: crate::features::addons::AddonsPage,
    pub(crate) saves: crate::features::career_saves::SavesPage,
    pub(crate) tt: crate::features::time_trial::TtPage,
    pub(crate) support: crate::features::support::SupportPage,
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
    pub(crate) prefs: crate::shell::config::Prefs,
    pub(crate) addon_views: crate::features::sections::AddonViews,
    pub(crate) scaling: crate::shell::ui_mode::Scaling,
    pub(crate) crashes: crate::dialogs::crash::Crashes,
    pub(crate) layout: crate::shell::render::Layout,
    pub(crate) _background: crate::shell::startup::Background,
    pub(crate) overlays: overlays::Overlays,
}

pub struct SliderSlot {
    pub state: Entity<SliderState>,
    pub synced: usize,
    pub _changes: Subscription,
}
