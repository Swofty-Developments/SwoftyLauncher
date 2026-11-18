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
mod tests {
    use super::*;

    #[test]
    fn career_saves_table_matches_header() {
        assert_eq!(std::mem::size_of::<SwoftyCareerSaves>(), 80);
        assert_eq!(std::mem::offset_of!(SwoftyCareerSaves, describe), 32);
        assert_eq!(std::mem::offset_of!(SwoftyCareerSaves, missing), 72);
        let header = include_str!("../include/swofty_api.h");
        assert!(header.contains(&format!(
            "#define SWOFTY_CAREER_SAVES_VERSION {CAREER_SAVES_VERSION}"
        )));
        for field in [
            "int (*index)(char *json, size_t cap, size_t *len);",
            "int (*describe)(SwoftyText request_json);",
            "int (*need)(const char *addon, const char *range, const char *why);",
            "int (*data_set)(const char *career, const char *addon, uint32_t version, SwoftyText json);",
            "int (*missing)(char *json, size_t cap, size_t *len);",
            "SWOFTY_EXPORT const SwoftyCareerSaves *SwoftyCoreCareerSaves(void);",
        ] {
            assert!(header.contains(field), "{field}");
        }
    }

    #[test]
    fn layout_matches_header() {
        assert_eq!(std::mem::size_of::<SwoftyApi>(), 200);
        assert_eq!(std::mem::offset_of!(SwoftyApi, registry), 176);
        assert_eq!(std::mem::offset_of!(SwoftyApi, services), 192);
        assert_eq!(std::mem::offset_of!(SwoftyApi, era), 168);
        assert_eq!(std::mem::size_of::<SwoftyEraBox>(), 56);
        assert_eq!(std::mem::offset_of!(SwoftyEraBox, order), 32);
        assert_eq!(std::mem::offset_of!(SwoftyEraBox, time_trial_entrant_list), 40);
        assert_eq!(std::mem::offset_of!(SwoftyEraBox, only), 48);
        assert_eq!(std::mem::size_of::<SwoftyEra>(), 24);
        assert_eq!(std::mem::offset_of!(SwoftyApi, characters), 152);
        assert_eq!(std::mem::offset_of!(SwoftyApi, career_wizard), 160);
        assert_eq!(std::mem::size_of::<SwoftyCharacters>(), 64);
        assert_eq!(std::mem::size_of::<SwoftyCareerWizard>(), 72);
        assert_eq!(std::mem::offset_of!(SwoftyApi, career), 120);
        assert_eq!(std::mem::offset_of!(SwoftyApi, popup), 128);
        assert_eq!(std::mem::offset_of!(SwoftyApi, lang), 136);
        assert_eq!(std::mem::offset_of!(SwoftyApi, career_content), 144);
        assert_eq!(std::mem::size_of::<SwoftyLang>(), 40);
        assert_eq!(std::mem::size_of::<SwoftyCareer>(), 80);
        assert_eq!(std::mem::offset_of!(SwoftyCareer, block), 64);
        assert_eq!(std::mem::size_of::<SwoftyCareerStart>(), 80);
        assert_eq!(std::mem::offset_of!(SwoftyCareerStart, route), 16);
        assert_eq!(std::mem::size_of::<SwoftyPopup>(), 16);
        assert_eq!(std::mem::size_of::<SwoftyHud>(), 32);
        assert_eq!(std::mem::offset_of!(SwoftyHud, toast), 16);
        assert_eq!(std::mem::size_of::<SwoftyCareerContentDesc>(), 80);
        assert_eq!(std::mem::offset_of!(SwoftyCareerContentDesc, car_group), 64);
        assert_eq!(std::mem::offset_of!(SwoftyCareerContentDesc, careers), 72);
        assert_eq!(std::mem::size_of::<SwoftyCareerContentInfo>(), 352);
        assert_eq!(std::mem::offset_of!(SwoftyCareerContentInfo, id), 32);
        assert_eq!(std::mem::size_of::<SwoftyCareerContent>(), 40);
        assert_eq!(std::mem::size_of::<SwoftyFlowState>(), 88);
        assert_eq!(std::mem::offset_of!(SwoftyFlowState, name), 24);
        assert_eq!(std::mem::size_of::<SwoftyCareerState>(), 80);
        assert_eq!(std::mem::offset_of!(SwoftyCareerState, key), 48);
        assert_eq!(std::mem::offset_of!(SwoftyApi, assets), 72);
        assert_eq!(std::mem::offset_of!(SwoftyApi, events), 80);
        assert_eq!(std::mem::offset_of!(SwoftyApi, overlay), 88);
        assert_eq!(std::mem::offset_of!(SwoftyApi, telemetry), 96);
        assert_eq!(std::mem::offset_of!(SwoftyApi, db), 104);
        assert_eq!(std::mem::offset_of!(SwoftyApi, storage), 112);
        assert_eq!(std::mem::size_of::<SwoftyAssets>(), 64);
        assert_eq!(std::mem::offset_of!(SwoftyAssets, db_set_raw), 32);
        assert_eq!(std::mem::offset_of!(SwoftyAssets, overlay_file), 56);
        assert_eq!(std::mem::size_of::<SwoftyFragment>(), 24);
        assert_eq!(std::mem::size_of::<SwoftyEvent>(), 48);
        assert_eq!(std::mem::size_of::<SwoftyEvents>(), 32);
        assert_eq!(std::mem::size_of::<SwoftyOverlay>(), 88);
        assert_eq!(std::mem::size_of::<SwoftyCarState>(), 192);
        assert_eq!(std::mem::size_of::<SwoftySessionState>(), 80);
        assert_eq!(std::mem::size_of::<SwoftyParticipant>(), 92);
        assert_eq!(std::mem::size_of::<SwoftyTelemetryFeed>(), 6920);
        assert_eq!(std::mem::offset_of!(SwoftyTelemetryFeed, cars), 104);
        assert_eq!(std::mem::size_of::<SwoftyTelemetry>(), 40);
        assert_eq!(std::mem::size_of::<SwoftyDbValue>(), 32);
        assert_eq!(std::mem::offset_of!(SwoftyDbValue, i), 8);
        assert_eq!(std::mem::offset_of!(SwoftyDbValue, s), 24);
        assert_eq!(std::mem::size_of::<SwoftyDbCursor>(), 32);
        assert_eq!(std::mem::size_of::<SwoftyDb>(), 96);
        assert_eq!(std::mem::size_of::<SwoftyStorage>(), 56);
        assert_eq!(std::mem::offset_of!(SwoftyApi, log), 8);
        assert_eq!(std::mem::offset_of!(SwoftyApi, scan), 24);
        assert_eq!(std::mem::offset_of!(SwoftyApi, fields), 48);
        assert_eq!(std::mem::offset_of!(SwoftyApi, setting), 56);
        assert_eq!(std::mem::offset_of!(SwoftyApi, watch_setting), 64);
        assert_eq!(std::mem::size_of::<SwoftyMixin>(), 32);
        assert_eq!(std::mem::size_of::<SwoftySettingValue>(), 24);
        assert_eq!(std::mem::offset_of!(SwoftySettingValue, slider), 8);
        assert_eq!(std::mem::offset_of!(SwoftySettingValue, choice), 16);
        assert!(HEADER.contains(&format!("#define SWOFTY_API_VERSION {SWOFTY_API_VERSION}")));
        for name in [
            "SwoftyAddonInitFn",
            "SwoftyModInitFn",
            "SWOFTY_MIXIN_ON_LOAD = 6",
            "SWOFTY_SETTING_CHOICE = 3",
            "watch_setting",
            "SWOFTY_ASSET_UNAVAILABLE = 2",
            "const SwoftyAssets *assets;",
            "const SwoftyEvents *events;",
            "const SwoftyOverlay *overlay;",
            "const SwoftyTelemetry *telemetry;",
            "const SwoftyDb *db;",
            "const SwoftyStorage *storage;",
            "const SwoftyCareer *career;",
            "const SwoftyPopup *popup;",
            "const SwoftyLang *lang;",
            "} SwoftyLang;",
            "#define SWOFTY_EVENT_CAREER_STARTING \"career.starting\"",
            "SWOFTY_CAREER_START_CONTINUE = 2",
            "} SwoftyCareerStart;",
            "const SwoftyCareerContent *career_content;",
            "#define SWOFTY_EVENT_CAREER_CONTENT_PRESSED \"career.content.pressed\"",
            "} SwoftyCareerContentInfo;",
            "const SwoftyCharacters *characters;",
            "const SwoftyCareerWizard *career_wizard;",
            "#define SWOFTY_API_CHARACTERS 1",
            "#define SWOFTY_API_CAREER_WIZARD 1",
            "const SwoftyEra *era;",
            "#define SWOFTY_API_ERA 3",
            "const SwoftyRegistry *registry;",
            "const SwoftyHooks *hooks;",
            "const SwoftyServices *services;",
            "#define SWOFTY_REGISTRY_VERSION 1",
            "SwoftyCoreRegistry(void);",
            "#define SWOFTY_MEM_VERSION 1",
            "SwoftyCoreMem(void);",
            "} SwoftyEraBox;",
            "#define SWOFTY_EVENT_CAREER_HUB \"career.hub\"",
            "} SwoftyCareerState;",
            "#define SWOFTY_EVENT_SESSION_START \"session.start\"",
            "#define SWOFTY_TELEMETRY_VERSION 2",
            "#define SWOFTY_EVENT_TELEMETRY_OVERTAKE \"telemetry.overtake\"",
            "#define SWOFTY_OVERLAY_VERSION 3",
            "} SwoftyInput;",
            "} SwoftyTextStyle;",
            "SWOFTY_DB_BOOL = 5",
            "} SwoftyTelemetryFeed;",
        ] {
            assert!(HEADER.contains(name), "{name}");
        }
        for name in [
            "SWOFTY_SETTING(",
            "SWOFTY_ON_CHANGE(",
            "struct setting",
            "SWOFTY_ADDON(",
            "SWOFTY_MOD(",
        ] {
            assert!(MIXIN_HEADER.contains(name), "{name}");
        }
    }

    #[test]
    fn template_headers_match() {
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty_api.h"),
            HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/mixin.hpp"),
            MIXIN_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/assets.hpp"),
            ASSETS_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/events.hpp"),
            EVENTS_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/overlay.hpp"),
            OVERLAY_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/overlay_ui.hpp"),
            OVERLAY_UI_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/telemetry.hpp"),
            TELEMETRY_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/live.hpp"),
            LIVE_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/db.hpp"),
            DB_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/storage.hpp"),
            STORAGE_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/career.hpp"),
            CAREER_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/mem.hpp"),
            MEM_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/popup.hpp"),
            POPUP_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/lang.hpp"),
            LANG_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/career_content.hpp"),
            CAREER_CONTENT_HEADER
        );
        assert!(CAREER_CONTENT_HEADER.contains(CAREER_CONTENT_EXPORT));
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/characters.hpp"),
            CHARACTERS_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/career_wizard.hpp"),
            CAREER_WIZARD_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/era.hpp"),
            ERA_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/hud.hpp"),
            HUD_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/postfx.hpp"),
            POSTFX_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/ease.hpp"),
            EASE_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/hud_format.hpp"),
            HUD_FORMAT_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/hud_frame.hpp"),
            HUD_FRAME_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/hud_leaves.hpp"),
            HUD_LEAVES_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/log_csv.hpp"),
            LOG_CSV_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/settings_struct.hpp"),
            SETTINGS_STRUCT_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/timing.hpp"),
            TIMING_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/fake_hud.hpp"),
            FAKE_HUD_HEADER
        );
        assert_eq!(
            include_str!("../../../templates/addon/native/include/swofty/fake_live.hpp"),
            FAKE_LIVE_HEADER
        );
    }

    #[test]
    fn capability_names_round_trip() {
        for c in Capability::ALL {
            assert_eq!(Capability::parse(c.as_str()), Some(c));
        }
        assert!(Capability::Assets.available_in(4));
        assert!(!Capability::Assets.available_in(3));
        for c in [
            Capability::ReadDb,
            Capability::OnFrame,
            Capability::Events,
            Capability::Overlay,
            Capability::Telemetry,
            Capability::Storage,
        ] {
            assert!(c.available_in(5));
            assert!(!c.available_in(4));
        }
        assert!(Capability::Career.available_in(6));
        assert!(!Capability::Career.available_in(5));
        assert!(Capability::Popup.available_in(7));
        assert!(!Capability::Popup.available_in(6));
        assert!(Capability::Lang.available_in(8));
        assert!(!Capability::Lang.available_in(7));
    }

    #[test]
    fn assets_header_wraps_every_call() {
        for name in [
            "enum class Result { ok, late, unavailable, invalid };",
            "bool available(",
            "Result register_group(",
            "Result put_resource(",
            "Result db_set(std::string_view key",
            "Result db_set(uint64_t hash",
            "Result lang_set(",
            "Result lang_table(",
            "Result overlay_file(",
        ] {
            assert!(ASSETS_HEADER.contains(name), "{name}");
        }
    }

    #[test]
    fn stdlib_headers_wrap_every_call() {
        for name in [
            "namespace swofty::events {",
            "bool available(",
            "Result subscribe(",
            "Result on_frame(",
            "Result unsubscribe(",
        ] {
            assert!(EVENTS_HEADER.contains(name), "{name}");
        }
        for name in [
            "namespace swofty::overlay {",
            "Result on_paint(",
            "void rect(",
            "void text(",
            "Result load_image(",
            "Result image(",
            "inline bool v2()",
            "void rect_rounded(",
            "void arc(",
            "void polyline(",
            "struct TextStyle",
            "Result image_nine(",
            "void push_clip(",
            "Rect safe_area(",
            "float ui_scale(",
            "inline bool v3()",
            "inline Input input(",
            "inline Result focus(",
        ] {
            assert!(OVERLAY_HEADER.contains(name), "{name}");
        }
        for name in [
            "namespace swofty::overlay::ui {",
            "inline Rect panel(",
            "inline bool button(",
            "inline bool toggle(",
            "inline bool slider(",
            "bool list(",
            "inline void draw_toasts(",
            "inline bool text_field(",
        ] {
            assert!(OVERLAY_UI_HEADER.contains(name), "{name}");
        }
        for name in [
            "namespace swofty::telemetry {",
            "Result session(",
            "Result car(",
            "Result participant(",
            "Result shared_memory(",
        ] {
            assert!(TELEMETRY_HEADER.contains(name), "{name}");
        }
        for name in [
            "namespace swofty::db {",
            "Result get(",
            "Result int_at(",
            "Result row(",
            "Result step(",
            "Result cell(",
        ] {
            assert!(DB_HEADER.contains(name), "{name}");
        }
        for name in [
            "namespace swofty::storage {",
            "Result read(",
            "Result write(",
            "Result append(",
            "Result remove(",
            "Result list(",
        ] {
            assert!(STORAGE_HEADER.contains(name), "{name}");
        }
    }
}
