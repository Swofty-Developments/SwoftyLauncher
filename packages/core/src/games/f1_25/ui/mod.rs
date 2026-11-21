pub mod car_category;
pub mod career_row;
pub mod career_select;
pub mod catalog;
pub mod check;
pub mod era;
pub mod home_menu;
pub mod home_rows;
pub mod hub;
pub mod icons;
pub mod idf;
pub mod marks;
pub mod merge;
pub mod mfd;
pub mod node;
pub mod pages;
pub mod patch;
pub mod rows;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_formats::erp::ErpFile;

use crate::addons::settings::{SessionScreen, SessionSettings};
use crate::{Error, Result};
use node::err;
use pages::{Sources, TILES};
use patch::Popup;

mod addon_screens;
mod addons_page;
mod archive_writes;
mod archives;
mod menu_rows;
mod picker_rows;
mod toggles;

use addon_screens::{
    Routing, Targets, career_setups, options_pages, resume_screen, screen_problems, ui_state,
};
pub use addons_page::{MfdRow, mfd_page_rows, mfd_rows};
use archive_writes::{
    check_resume_routes, check_setup_routes, register, route_problems, write_databases, write_flags,
    write_flow_persistent, write_frontend, write_leaderboard, write_mfd_templates, write_persistent,
    write_playercard,
};
pub use archives::ArchiveWriter;
use archives::read_frag;
use menu_rows::hub_tabs;
use picker_rows::picker_rows;
use toggles::{panel_shown, season_picker_split};

pub const UI: &str = "2025_asset_groups/ui_package";
pub const FLOW: &str = "flow_options";
pub const ANCHOR: &str = "settings_container";
const OPTIONS_FLOW: &str = "options_flow";
const FE_HOME: &str = "eaid://ui_package/screens/fe_home.ui";
const FE_CAREER: &str = "eaid://ui_package/screens/fe_career.ui";
const CONTAINER: &str = "eaid://ui_package/screens/settings_container.ui";
const ROUTES: &str = "eaid://ui_package/routes/routes_frontend_general.uir";
const GAME_ROUTES: &str = "eaid://ui_package/routes/routes.uir";
const F1_WORLD_ROUTES: &str = "eaid://ui_package/routes/routes_f1_world.uir";
const CUSTOMISATION_ROUTES: &str = "eaid://ui_package/routes/routes_customisation_and_store.uir";
const WIZARD_ROUTES: &str = "eaid://ui_package/routes/routes_career_wizard.uir";
const SETUP_FLOW_ERP: &str = "flow_career_season_setup";
const SETUP_BACK_ERP: &str = "flow_frontend";
const FLOWS: &str = "eaid://ui_package/flows.ui";
const REGIONS: &str = "eaid://ui_package/region_loading.uiconfig";
const TOOLTIPS: &str = "eaid://ui_package/databases/tool_tips.uidb";
const UI_DATA: &str = "eaid://ui_package/databases/ui_data.uidb";
const OVERRIDES: &str = "eaid://ui_package/databases/overrides.uidb";
const MESSAGES: &str = "eaid://ui_package/message_data/onlineservices_messages.uimsg";
const HUD_MESSAGES: &str = "eaid://ui_package/message_data/hud_messages.uimsg";
const PERSISTENT_UI: &str = "eaid://ui_package/screens/persistent.ui";
const FRONTEND_TRANSITION: &str = "eaid://ui_package/screens/frontend_transition.ui";
const LEADERBOARD_ARCHIVE: &str = "screens/leaderboard_container.erp";
const LEADERBOARD_UI: &str = "eaid://ui_package/screens/leaderboard_container.ui";

pub const ERPS: &[&str] = &[
    "persistent.erp",
    "databases.erp",
    "flow_persistent.erp",
    "flow_frontend.erp",
    LEADERBOARD_ARCHIVE,
    home_rows::PLAYERCARD_ARCHIVE,
    era::ARCHIVES[0],
    era::ARCHIVES[1],
    car_category::ARCHIVES[1],
];
pub const READS: &[&str] = &["flow_options.erp"];

pub fn ui_file(rel: &str) -> String {
    format!("{UI}/{rel}")
}

#[derive(Debug, Clone, Default)]
pub struct MenuBuild {
    pub erps: BTreeMap<String, PathBuf>,
    pub screens: BTreeMap<String, PathBuf>,
    pub session: Vec<SessionScreen>,
    pub strings: Vec<(String, String)>,
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
}

/// The addon whose copy of a resource (second) in a game archive (first)
/// the build reads; `None` for the game's own copy.
pub type Writer<'a> = dyn Fn(&str, &str) -> Option<String> + 'a;

type Patch<'a> = Box<dyn FnOnce(&[u8]) -> Result<Vec<u8>> + 'a>;

fn screen_flow(input: &dyn Fn(&str) -> PathBuf, flows_ui: &[u8], erp: &str, screen: &str) -> Result<String> {
    let path = input(&ui_file(&format!("{erp}.erp")));
    let file = ErpFile::open(&path)?;
    if file
        .erp
        .find(&format!("eaid://ui_package/screens/{screen}.ui"))
        .is_err()
    {
        return Err(err(format!("{erp}.erp has no screen {screen}")));
    }
    check::flow_loading(flows_ui, erp)?.ok_or_else(|| err(format!("no flow in flows.ui loads {erp}")))
}

pub fn build(
    settings: &SessionSettings,
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lang: bool,
    archives: &ArchiveWriter,
) -> Result<MenuBuild> {
    let flow_options = input(&ui_file("flow_options.erp"));
    let flow_frontend = input(&ui_file("flow_frontend.erp"));
    let src = Sources::parse(
        &read_frag(&flow_options, CONTAINER, 1)?,
        &read_frag(&flow_frontend, FE_CAREER, 0)?,
        &read_frag(&flow_frontend, FE_HOME, 0)?,
        &read_frag(&flow_options, CONTAINER, 0)?,
    )?;
    let flow = pages::entries(&settings.addons, &settings.maps, &settings.mods);
    let mut built = pages::build_all(&src, &TILES, &flow)?;
    let generated = options_pages(settings, &src, &flow, &mut built)?;
    let (setups, setup_lines) = career_setups(input, &src, &flow, &mut built)?;
    let resume = resume_screen(settings, input, &src, &mut built)?;
    let routing = Routing::of(settings, &built, &generated, &setups);
    let mut problems = screen_problems(&built)?;
    let targets = Targets::of(&flow, &setups, resume.as_ref());
    let mfd_rows = mfd_rows(settings);
    let state = ui_state(&flow, &mfd_rows)?;
    let popup = Popup::for_version(env!("CARGO_PKG_VERSION"));
    let show_panel = panel_shown(settings);

    let erp = |rel: &str| (input(&ui_file(rel)), out.join(ui_file(rel)));
    let persistent = write_persistent(
        &erp("persistent.erp"),
        &flow_options,
        &routing,
        &targets,
        &setups,
        resume.as_ref(),
        !mfd_rows.is_empty(),
        archives,
    )?;
    check_setup_routes(&setups, &persistent.placed)?;
    check_resume_routes(resume.as_ref(), &persistent.placed)?;
    problems.extend(route_problems(
        &persistent,
        &routing,
        &targets,
        &setups,
        resume.as_ref(),
    )?);
    if !problems.is_empty() {
        return Err(Error::Other(format!(
            "ADDON SETTINGS failed its checks:\n  {}",
            problems.join("\n  ")
        )));
    }

    write_databases(&erp("databases.erp"), state, archives)?;
    write_flow_persistent(&erp("flow_persistent.erp"), lang, archives)?;
    let frontend = write_frontend(&erp("flow_frontend.erp"), writer, show_panel, archives)?;
    write_leaderboard(&erp(LEADERBOARD_ARCHIVE), archives)?;
    write_playercard(&erp(home_rows::PLAYERCARD_ARCHIVE), archives)?;

    let mut build = MenuBuild::default();
    build
        .lines
        .push("ui: the player card strip hides while the main menu is up".to_owned());
    build.lines.extend(setup_lines);
    if let Some(r) = &resume {
        build.lines.push(r.line.clone());
    }
    build.lines.extend(frontend.lines);
    build.warnings.extend(frontend.warnings);
    write_flags(out, &mut build, persistent, frontend.rows_built)?;
    if !mfd_rows.is_empty() {
        write_mfd_templates(input, out, &mfd_rows, &mut build, archives)?;
    }
    let eras = picker_rows(
        season_picker_split(settings),
        input,
        writer,
        out,
        &mut build.lines,
        &mut build.warnings,
        archives,
    )?;
    let hubs = hub_tabs(
        settings,
        input,
        writer,
        out,
        &mut build.lines,
        &mut build.warnings,
        archives,
    )?;
    if lang {
        build.strings = popup.strings();
        build
            .strings
            .push((patch::FRIENDS_TAB_LABEL.into(), patch::FRIENDS_TAB_TEXT.into()));
    }
    register(out, &mut build, &eras, hubs, &built)?;
    Ok(build)
}

#[cfg(test)]
mod tests;
