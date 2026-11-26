mod f1_world;
mod home;
mod leaderboard;
mod messages;
mod persistent;
mod resume;
mod routes;
mod screens;
mod setup;
mod tooltips;

pub use f1_world::{
    BACK_HOME_FROM_CONTAINER, CUSTOMISATION_GROUP, CUSTOMISATION_ROUTE, GRAND_PRIX_ENTRY, GRAND_PRIX_ROUTE,
    GRAND_PRIX_SAVES_ROUTE, HOME_BACK_ROUTE, RETURN_ENTRY, SHOWROOM_ROUTE, TIME_TRIAL_ENTRY,
    TIME_TRIAL_ROUTE, customisation_routes, f1_world_entries, f1_world_routes, frontend_transition,
    hands_over_to_return,
};
pub use home::{OFFLINE_TEXT, OFFLINE_TEXT_PATH, Popup, fe_home, hide_panel, offline_panel, popup_seen};
pub use leaderboard::{FRIENDS_TAB_LABEL, FRIENDS_TAB_TEXT, leaderboard_glyphs};
pub use messages::{CONFIRM_ID, MESSAGE_ID, TOAST_PREFIX, TOAST_SECONDS, hud_messages, messages};
pub use persistent::persistent_gate;
pub use resume::{ENTRY_GROUPS, RESUME_CONTINUE_SCREEN, ResumeRoutes, resume_route_group, resume_routes};
pub use routes::{
    RouteClone, anchor_routes, clone_route_groups, has_route, route_groups, routes, screen_route_groups,
};
pub use screens::{flow_entries, regions, ui_data};
pub use setup::{DRIVER_CAREER, Placed, SETUP_ENTRY_SOURCE, SetupRoutes, setup_route_groups, setup_routes};
pub use tooltips::{PROMPT_LABEL, tooltips};

pub const ACTION: &str = "Menu Right Stick Click";
pub const MOD_MENU: &str = "mod_menu";
pub const MOD_MENU_OFFLINE: &str = "mod_menu_offline";
pub const OFFLINE_PROMPT: &str = "to open settings";
const ONCE_ATTR: &str = "swofty_once";
pub const POPUP_TITLE: &str = "lng_swofty_popup_title";
pub const POPUP_BODY: &str = "lng_swofty_popup_body";

#[cfg(test)]
mod tests;
