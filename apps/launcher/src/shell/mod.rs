use crate::dialogs::crash::CrashView;
use crate::services::library::Library;
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use swofty_core::crash::StartFailure;
use swofty_core::session::UiMode;
use swofty_core::store::Store;
use swofty_core::view::View;

mod account;
mod barfit;
mod config;
mod console_window;
mod dialogs;
pub(crate) mod download_dock;
mod nav;
pub(crate) mod notices;
mod overflow;
mod page;
pub(crate) mod pages;
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
    pub(crate) notch_beside: bool,
    pub(crate) root_focus: FocusHandle,
    pub(crate) profiles_open: bool,
    pub(crate) layout_width: f32,
    pub(crate) online: crate::features::online::Online,
    pub(crate) library: Library,
    pub(crate) open_addon: Option<String>,
    pub(crate) mp_page: crate::features::multiplayer::Multiplayer,
    pub(crate) crash: Option<(PathBuf, Result<String, StartFailure>)>,
    /// The newest crash's diagnosis, and whether its modal is open.
    pub(crate) crash_view: Option<CrashView>,
    pub(crate) crash_open: bool,
    pub(crate) dismissed: Option<PathBuf>,
    pub(crate) seen: Seen,
    pub(crate) sliders: HashMap<String, SliderSlot>,
    pub(crate) requests: Option<SystemTime>,
    pub(crate) console: Option<AnyWindowHandle>,
    pub(crate) developer: crate::features::developer::Developer,
    pub(crate) views: HashMap<String, Result<Arc<View>, String>>,
    pub(crate) views_profile: String,
    pub(crate) view_gen: u64,
    pub(crate) view_gens: HashMap<String, u64>,
    pub(crate) busy: HashSet<String>,
    pub(crate) open_row: Option<String>,
    pub(crate) progress: Option<(f32, String)>,
    pub(crate) tasks: Option<crate::dialogs::tasks::TaskDialog>,
    pub(crate) moving: Option<crate::dialogs::data_move::Moving>,
    pub(crate) found_data: Option<crate::dialogs::found_data::FoundPrompt>,
    pub(crate) dropping: bool,
    pub(crate) ui_mode: Option<UiMode>,
    pub(crate) ui_scale: Option<u16>,
    pub(crate) ui_scale_slider: Entity<SliderState>,
    pub(crate) ui_scale_drag: Option<u16>,
    pub(crate) _ui_scale_changes: Subscription,
    pub(crate) onboarding: Option<crate::features::onboarding::Onboarding>,
    reload_generation: u64,
    slow_logged: bool,
    pub(crate) updates: crate::features::update::Updates,
    pub(crate) blocks: crate::blocks::Blocks,
    pub(crate) content_width: f32,
    pub(crate) short: bool,
    pub(crate) ticking: bool,
    painted: Instant,
    pub(crate) dock: crate::shell::download_dock::Dock,
    pub(crate) dock_wake: bool,
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
    _shaders_events: Subscription,
    pub(crate) tools: crate::features::tools::Carousel,
    pub(crate) settings: crate::features::settings::Settings,
    pub(crate) ads: crate::features::ads::Ads,
    pub(crate) ad_video: Option<crate::features::ads::AdVideo>,
    _window_moved: Subscription,
    _console_closed: Subscription,
    _poll: Task<()>,
    _live: Task<()>,
    _updates: Task<()>,
    _catalog: Task<()>,
    pub(crate) game: crate::shell::probe::GameWatch,
    pub(crate) launching: crate::shell::play::Launching,
    pub(crate) account: crate::shell::account::Account,
    pub(crate) prefs: crate::shell::config::Prefs,
}

pub struct SliderSlot {
    pub state: Entity<SliderState>,
    pub synced: usize,
    pub _changes: Subscription,
}
