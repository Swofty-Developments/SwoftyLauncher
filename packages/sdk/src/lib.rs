mod assets;
mod capability;
mod career;
mod career_content;
mod career_saves;
mod career_wizard;
mod characters;
mod db;
mod era;
mod events;
mod headers;
mod hud;
mod lang;
mod mixin;
mod overlay;
mod popup;
mod settings;
mod storage;
mod telemetry;

use std::ffi::{c_char, c_int, c_void};

pub use assets::{ASSET_INVALID, ASSET_LATE, ASSET_OK, ASSET_UNAVAILABLE, SwoftyAssets, SwoftyFragment};
pub use capability::Capability;
pub use career::{
    CAREER_START_CONTINUE, CAREER_START_NEW, CAREER_START_PICK, SwoftyCareer, SwoftyCareerStart,
    SwoftyCareerState, SwoftyFlowState,
};
pub use career_content::{
    CAREER_CONTENT_DRIVER, CAREER_CONTENT_EXPORT, CAREER_CONTENT_LIVE, CAREER_CONTENT_MODEL_2025,
    CAREER_CONTENT_MODEL_2026, CAREER_CONTENT_MY_TEAM, CAREER_CONTENT_OFF, CAREER_CONTENT_QUEUED,
    EVENT_CAREER_CONTENT_PRESSED, SwoftyCareerContent, SwoftyCareerContentDesc, SwoftyCareerContentInfo,
};
pub use career_saves::{CAREER_SAVES_VERSION, SwoftyCareerSaves, SwoftyText};
pub use career_wizard::SwoftyCareerWizard;
pub use characters::SwoftyCharacters;
pub use db::{DB_BOOL, DB_ENUM, DB_FLOAT, DB_ID, DB_INT, DB_STRING, SwoftyDb, SwoftyDbCursor, SwoftyDbValue};
pub use era::{SwoftyEra, SwoftyEraBox};
pub use events::{SwoftyEvent, SwoftyEventFn, SwoftyEvents, SwoftyFrameFn};
pub use headers::{
    ASSETS_HEADER, CAREER_CONTENT_HEADER, CAREER_HEADER, CAREER_WIZARD_HEADER, CHARACTERS_HEADER, DB_HEADER,
    EASE_HEADER, ERA_HEADER, EVENTS_HEADER, FAKE_HUD_HEADER, FAKE_LIVE_HEADER, HEADER, HUD_FORMAT_HEADER,
    HUD_FRAME_HEADER, HUD_HEADER, HUD_LEAVES_HEADER, LANG_HEADER, LIVE_HEADER, LOG_CSV_HEADER, MEM_HEADER,
    MIXIN_HEADER, OVERLAY_HEADER, OVERLAY_UI_HEADER, POPUP_HEADER, POSTFX_HEADER, SETTINGS_STRUCT_HEADER,
    STORAGE_HEADER, TELEMETRY_HEADER, TIMING_HEADER,
};
pub use hud::{HUD_BOOL, HUD_INT, HUD_TEXT, HUD_VERSION, SwoftyHud};
pub use lang::SwoftyLang;
pub use mixin::{
    MIXIN_ON_LOAD, MIXIN_OVERWRITE, MIXIN_PATCH, MIXIN_REDIRECT, MIXIN_SITE, MIXIN_VTABLE, MIXIN_WRAP,
    RegisterMixinsFn, SwoftyMixin,
};
pub use overlay::{SwoftyOverlay, SwoftyPaint, SwoftyPaintFn};
pub use popup::SwoftyPopup;
pub use settings::{
    SETTING_CHOICE, SETTING_SLIDER, SETTING_TOGGLE, SettingChangedFn, SettingFn, SwoftySettingValue,
    WatchSettingFn,
};
pub use storage::SwoftyStorage;
pub use telemetry::{
    EVENT_TELEMETRY_COLLISION, EVENT_TELEMETRY_OVERTAKE, EVENT_TELEMETRY_RETIREMENT, SwoftyCarState,
    SwoftyParticipant, SwoftySessionState, SwoftyTelemetry, SwoftyTelemetryFeed, TELEMETRY_VERSION,
};

pub const SWOFTY_API_VERSION: u32 = 9;
pub const INIT_SYMBOLS: [&str; 2] = ["SwoftyAddonInit", "SwoftyModInit"];
pub const API_SYMBOLS: [&str; 2] = ["SwoftyAddonApi", "SwoftyModApi"];
pub const TOO_SMALL: c_int = 4;
pub const REFUSED: c_int = 5;

pub type LogFn = unsafe extern "C" fn(fmt: *const c_char, ...);
pub type ModuleBaseFn = unsafe extern "C" fn() -> usize;
pub type ScanFn =
    unsafe extern "C" fn(section: *const c_char, pattern: *const c_char, out: *mut usize) -> c_int;
pub type ResolveFn = unsafe extern "C" fn(id: *const c_char) -> usize;
pub type FieldsFn = unsafe extern "C" fn(obj: *const c_void, size: usize) -> *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyApi {
    pub version: u32,
    pub log: Option<LogFn>,
    pub module_base: Option<ModuleBaseFn>,
    pub scan: Option<ScanFn>,
    pub resolve: Option<ResolveFn>,
    pub register_mixins: Option<RegisterMixinsFn>,
    pub fields: Option<FieldsFn>,
    pub setting: Option<SettingFn>,
    pub watch_setting: Option<WatchSettingFn>,
    pub assets: *const SwoftyAssets,
    pub events: *const SwoftyEvents,
    pub overlay: *const SwoftyOverlay,
    pub telemetry: *const SwoftyTelemetry,
    pub db: *const SwoftyDb,
    pub storage: *const SwoftyStorage,
    pub career: *const SwoftyCareer,
    pub popup: *const SwoftyPopup,
    pub lang: *const SwoftyLang,
    pub career_content: *const SwoftyCareerContent,
    pub characters: *const SwoftyCharacters,
    pub career_wizard: *const SwoftyCareerWizard,
    pub era: *const SwoftyEra,
    pub registry: *const c_void,
    pub hooks: *const c_void,
    pub services: *const c_void,
}

pub type SwoftyAddonInitFn = unsafe extern "C" fn(api: *const SwoftyApi) -> c_int;
pub type SwoftyModInitFn = SwoftyAddonInitFn;

#[cfg(test)]
mod tests;
