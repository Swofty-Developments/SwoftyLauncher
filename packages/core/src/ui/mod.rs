pub mod car_category;
pub mod career_select;
pub mod catalog;
pub mod check;
pub mod era;
pub mod home_menu;
pub mod home_rows;
pub mod hub;
pub mod icons;
pub mod idf;
pub mod merge;
pub mod mfd;
pub mod node;
pub mod pages;
pub mod patch;
pub mod rows;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_formats::erp::{ErpFile, Packer};
use swofty_formats::pkg::Node;

use crate::addons::settings::{SessionScreen, SessionSettings, UI_DIR, screen_file};
use crate::manifest::CAREER_SETUP_BACK;
use crate::{Error, Result, io_err};
use node::{boolv, err, s, uidb_var};
use pages::{ADJUST_TOOLTIP, Sources, TILES};
use patch::Popup;

mod addons_page;
mod archives;
mod menu_rows;
mod picker_rows;
mod toggles;

pub use addons_page::{MfdRow, mfd_page_rows, mfd_rows};
use archives::{read_frag, write_archive, write_erp};
use menu_rows::{career_row, customisation_built, hub_tabs, main_menu_rows};
use picker_rows::{car_category_boxes, era_rows};
use toggles::{core_version, panel_shown, season_picker_split};

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
    packer: &Packer,
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
    // Addon screens with page: options are their addon's options page under the name
    // swofty_addon_<name>_<addon index>; the view model reads the index after the last '_'.
    let mut generated: Vec<(String, String, String)> = Vec::new();
    for s in settings.ui.iter().flat_map(|u| &u.screens) {
        if s.page.as_deref() != Some("options") {
            continue;
        }
        let e = flow
            .iter()
            .find(|e| e.kind == pages::Kind::Addon && Some(e.id) == s.addon.as_deref())
            .ok_or_else(|| err(format!("screen {}: its addon is not on", s.name)))?;
        let full = e.screen.trim_start_matches("swofty_addon_");
        let screen = format!("swofty_addon_{}_{full}", s.name);
        let anchor = s.anchor.clone().unwrap_or_else(|| ANCHOR.to_owned());
        built
            .order
            .push((screen.clone(), pages::build_options_page(&src, e, &screen)?));
        if !e.options.is_empty() {
            built.view_models.push(screen.clone());
        }
        generated.push((s.name.clone(), anchor, screen));
    }
    let flows_ui = match flow.iter().any(|e| e.setup.is_some()) {
        true => read_frag(&input(&ui_file("persistent.erp")), FLOWS, 0)?,
        false => Vec::new(),
    };
    let mut setups: Vec<patch::SetupRoutes> = Vec::new();
    let mut setup_lines = Vec::new();
    for e in flow.iter().filter(|e| e.kind == pages::Kind::Addon) {
        let Some(setup) = e.setup else { continue };
        let (home, home_ui) = pages::build_setup_home(&src, e, setup)?;
        let (rows, rows_ui) = pages::build_setup_page(&src, e, setup)?;
        built.order.push((home.clone(), home_ui));
        built.order.push((rows.clone(), rows_ui));
        built.view_models.push(rows.clone());
        let back = CAREER_SETUP_BACK.to_owned();
        setups.push(patch::SetupRoutes {
            route: setup.route.clone(),
            next_flow: screen_flow(input, &flows_ui, SETUP_FLOW_ERP, &setup.next)?,
            back_flow: screen_flow(input, &flows_ui, SETUP_BACK_ERP, &back)?,
            next: setup.next.clone(),
            back,
            back_route: setup.back_route(),
            home,
            rows,
        });
        setup_lines.push(format!(
            "ui: {} has a career setup page {}: the route {} opens it from the career content select, CONFIRM \
             opens {}, and {}'s route {} opens it again",
            e.id,
            setup.screen(),
            setup.route,
            setup.next,
            setup.next,
            setup.back_route()
        ));
    }
    let names: Vec<String> = built.order.iter().map(|(n, _)| n.clone()).collect();
    // Addon screens in the options flow get the flow entry, region and route group an ADDON
    // SETTINGS page gets, so routes and signals to them open them.
    let addon_routes: Vec<(String, String)> = settings
        .ui
        .iter()
        .flat_map(|u| &u.screens)
        .filter(|s| s.flow == FLOW && s.page.is_none() && !names.contains(&s.name))
        .map(|s| {
            (
                s.name.clone(),
                s.anchor.clone().unwrap_or_else(|| ANCHOR.to_owned()),
            )
        })
        .collect();
    let addon_screens: Vec<String> = addon_routes.iter().map(|(n, _)| n.clone()).collect();
    let mut opened: Vec<(String, String, String)> = addon_routes
        .iter()
        .map(|(n, a)| (n.clone(), a.clone(), n.clone()))
        .collect();
    opened.extend(generated.iter().cloned());
    let mut addon_groups = addon_routes.clone();
    addon_groups.extend(generated.iter().map(|(_, a, s)| (s.clone(), a.clone())));
    let setup_groups: Vec<Node> = setups.iter().flat_map(patch::setup_route_groups).collect();
    let listed: Vec<String> = names.iter().chain(&addon_screens).cloned().collect();
    let mut problems = check::screens(&built.order, &built.view_models)?;
    let no_scenes = catalog::Scenes::default();
    for (name, data) in &built.order {
        let input = check::UiInput {
            resource: name,
            frag: "temp",
            data,
            stock: None,
        };
        let report = check::ui_edit(&input, &no_scenes, catalog::stock(), catalog::engine())?;
        problems.extend(report.hard.iter().map(|h| h.to_string()));
    }

    let homes = pages::home_plan(flow.len());
    let pages: Vec<(String, String, String)> = flow
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                format!("open_{}_{}", e.kind.key(), e.index),
                e.screen.clone(),
                pages::home_of(i),
            )
        })
        .collect();
    let mut setup_ids: Vec<String> = Vec::new();
    for s in &setups {
        setup_ids.extend([
            s.home.clone(),
            s.rows.clone(),
            format!("{}.{}", s.back, s.route),
            format!("{}.{}", s.next, s.back_route),
        ]);
    }
    let mut groups: Vec<&str> = homes.iter().map(|h| h.screen.as_str()).collect();
    groups.push("fe_home.mod_menu");
    groups.extend(pages.iter().map(|(_, s, _)| s.as_str()));
    groups.extend(setup_ids.iter().map(String::as_str));
    let mut state = pages::state_entries(&flow)?;
    let mfd_rows = mfd_rows(settings);
    if !mfd_rows.is_empty() {
        state.push((
            "mfd_enabled".to_owned(),
            uidb_var("mfd_enabled", "bool", boolv(true)),
        ));
        for (k, r) in mfd_rows.iter().enumerate() {
            let name = format!("m{k}");
            state.push((name.clone(), uidb_var(&name, "uistring", s(&r.text))));
        }
    }
    let popup = Popup::for_version(&core_version(settings));
    let show_panel = panel_shown(settings);

    let erp = |rel: &str| (input(&ui_file(rel)), out.join(ui_file(rel)));
    let (src_p, dst_p) = erp("persistent.erp");
    let routes_out = std::cell::RefCell::new(Vec::new());
    let game_routes_out = std::cell::RefCell::new(Vec::new());
    let wizard_routes_out = std::cell::RefCell::new(Vec::new());
    let flows_out = std::cell::RefCell::new(Vec::new());
    let customisation_out = std::cell::RefCell::new(Vec::new());
    let customisation_error = std::cell::RefCell::new(None);
    let placed = std::cell::RefCell::new(patch::Placed::default());
    let mut persistent: Vec<(&str, Patch)> = vec![
        (
            ROUTES,
            Box::new(|d| {
                let mut groups = patch::route_groups(&homes, &pages);
                groups.extend(setup_groups);
                let f1_world = read_frag(&src_p, F1_WORLD_ROUTES, 0).unwrap_or_default();
                let b = patch::routes(d, groups, patch::screen_route_groups(&addon_groups), &f1_world)?;
                let b = patch::setup_routes(&b, &setups, &mut placed.borrow_mut())?;
                *routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
        (
            FLOWS,
            Box::new(|d| {
                let b = patch::flow_entries(d, OPTIONS_FLOW, &listed)?;
                let b = patch::f1_world_entries(&b)?;
                *flows_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
        (REGIONS, Box::new(|d| patch::regions(d, &listed))),
        (
            GAME_ROUTES,
            Box::new(|d| {
                let b = patch::anchor_routes(d, &opened)?;
                let b = patch::setup_routes(&b, &setups, &mut placed.borrow_mut())?;
                *game_routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
    ];
    persistent.push((F1_WORLD_ROUTES, Box::new(patch::f1_world_routes)));
    persistent.push((
        CUSTOMISATION_ROUTES,
        Box::new(|d| {
            let built =
                read_frag(&src_p, F1_WORLD_ROUTES, 0).and_then(|f| patch::customisation_routes(d, &f));
            let b = match built {
                Ok(b) => b,
                Err(e) => {
                    *customisation_error.borrow_mut() = Some(e.to_string());
                    d.to_vec()
                }
            };
            *customisation_out.borrow_mut() = b.clone();
            Ok(b)
        }),
    ));
    if !setups.is_empty() {
        persistent.push((
            WIZARD_ROUTES,
            Box::new(|d| {
                let b = patch::setup_routes(d, &setups, &mut placed.borrow_mut())?;
                *wizard_routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ));
    }
    if !mfd_rows.is_empty() {
        persistent.push((mfd::METADATA, Box::new(mfd::metadata)));
    }
    // The addon tiles' icons, in persistent.erp so they are loaded wherever ADDON SETTINGS shows.
    let [view, surface] = icons::resources(
        &read_frag(&flow_options, icons::STOCK, 0)?,
        &read_frag(&flow_options, &format!("{}.image", icons::STOCK), 0)?,
    )?;
    let persistent = persistent.into_iter().map(|(name, f)| (name, 0, f)).collect();
    write_archive(
        &src_p,
        &dst_p,
        persistent,
        vec![
            (view.0, era::SRV_KIND, view.1),
            (surface.0, era::SURFACE_KIND, surface.1),
            (
                rows::O_ROW.to_owned(),
                "UILayout",
                vec![rows::row_scene(&read_frag(&flow_options, rows::O_BLOCK, 0)?)?],
            ),
        ],
        packer,
    )?;
    for s in &setups {
        for id in [
            format!("{}.{}", s.back, s.route),
            format!("{}.{}", s.next, s.back_route),
        ] {
            if !placed.borrow().routes.contains(&id) {
                return Err(Error::Other(format!(
                    "career setup {}: the game's routes have no route group {} for {id}",
                    s.route,
                    id.split('.').next().unwrap_or_default()
                )));
            }
        }
    }
    let registered: BTreeMap<String, String> = listed.iter().map(|n| (n.clone(), FLOW.to_owned())).collect();
    let stock_screens: std::collections::BTreeSet<String> = setups
        .iter()
        .flat_map(|s| [s.back.clone(), s.next.clone()])
        .chain(placed.borrow().screens.iter().cloned())
        .collect();
    let (r, g, w) = (
        routes_out.borrow(),
        game_routes_out.borrow(),
        wizard_routes_out.borrow(),
    );
    let mut route_files: Vec<&[u8]> = vec![&r, &g];
    if !w.is_empty() {
        route_files.push(&w);
    }
    problems.extend(check::route_files(
        &route_files,
        &flows_out.borrow(),
        &groups,
        &registered,
        &stock_screens,
    )?);
    if !problems.is_empty() {
        return Err(Error::Other(format!(
            "ADDON SETTINGS failed its checks:\n  {}",
            problems.join("\n  ")
        )));
    }

    let (src_d, dst_d) = erp("databases.erp");
    write_erp(
        &src_d,
        &dst_d,
        vec![
            (TOOLTIPS, Box::new(|d| patch::tooltips(d, ADJUST_TOOLTIP))),
            (UI_DATA, Box::new(move |d| patch::ui_data(d, state))),
            (OVERRIDES, Box::new(icons::overrides)),
        ],
        packer,
    )?;
    let (src_f, dst_f) = erp("flow_persistent.erp");
    write_erp(
        &src_f,
        &dst_f,
        vec![
            (MESSAGES, Box::new(|d| patch::messages(d, lang))),
            (HUD_MESSAGES, Box::new(patch::hud_messages)),
            (
                PERSISTENT_UI,
                Box::new(|d| {
                    let out = patch::persistent_gate(d)?;
                    let changed = check::stock_kept(d, &out, "swofty_")?;
                    if !changed.is_empty() {
                        return Err(Error::Other(format!(
                            "the popup gate changes the game's own popups:\n  {}",
                            changed.join("\n  ")
                        )));
                    }
                    Ok(out)
                }),
            ),
        ],
        packer,
    )?;
    let (src_h, dst_h) = erp("flow_frontend.erp");
    let (mut career_lines, mut career_warnings) = (Vec::new(), Vec::new());
    let career = career_row(&src_h, writer, &mut career_lines, &mut career_warnings)?;
    let mut frontend: Vec<(&str, usize, Patch)> = vec![(
        FE_HOME,
        0,
        Box::new(move |d| {
            let d = patch::offline_panel(&patch::fe_home(d)?)?;
            if show_panel { Ok(d) } else { patch::hide_panel(&d) }
        }),
    )];
    frontend.push((FRONTEND_TRANSITION, 0, Box::new(patch::frontend_transition)));
    frontend.push((LEADERBOARD_UI, 0, Box::new(patch::leaderboard_glyphs)));
    let mut home_lines = Vec::new();
    let mut home_warnings = Vec::new();
    let rows = match std::env::var_os("SWOFTY_HOME_ROWS") {
        Some(_) => main_menu_rows(
            &src_h,
            career.as_ref().map(|c| c.3.as_slice()),
            &mut home_lines,
            &mut home_warnings,
        )?,
        None => None,
    };
    let rows_built = rows.is_some();
    let mut templates = career.as_ref().map(|c| c.3.clone());
    if let Some((scene, with_rows)) = rows {
        templates = Some(with_rows);
        frontend.push((home_rows::SCENE, 0, Box::new(move |_| Ok(scene))));
    }
    if let Some((ui, type_ui, scene, _)) = career {
        frontend.push((career_select::UI, 0, Box::new(move |_| Ok(ui))));
        frontend.push((career_select::TYPE_UI, 0, Box::new(move |_| Ok(type_ui))));
        frontend.push((career_select::SCENE, 0, Box::new(move |_| Ok(scene))));
    }
    if let Some(templates) = templates {
        frontend.push((
            career_select::TEMPLATES,
            career_select::TEMPLATES_FRAGMENT,
            Box::new(move |_| Ok(templates)),
        ));
    }
    write_archive(&src_h, &dst_h, frontend, Vec::new(), packer)?;

    let (src_l, dst_l) = erp(LEADERBOARD_ARCHIVE);
    write_erp(
        &src_l,
        &dst_l,
        vec![(LEADERBOARD_UI, Box::new(patch::leaderboard_glyphs))],
        packer,
    )?;

    let (src_c, dst_c) = erp(home_rows::PLAYERCARD_ARCHIVE);
    write_archive(
        &src_c,
        &dst_c,
        vec![(
            home_rows::PLAYERCARD,
            0,
            Box::new(|d| match home_rows::has_strip_binding(d) {
                Ok(false) => home_rows::hide_strip(d),
                _ => Ok(d.to_vec()),
            }),
        )],
        Vec::new(),
        packer,
    )?;

    let mut build = MenuBuild::default();
    build
        .lines
        .push("ui: the player card strip hides while the main menu is up".to_owned());
    build.lines.extend(setup_lines);
    build.lines.extend(career_lines);
    build.warnings.extend(career_warnings);
    build.lines.extend(home_lines);
    build.warnings.extend(home_warnings);
    let menu_flag = format!("{UI_DIR}/{}", home_rows::MENU_FLAG);
    let menu_path = out.join(&menu_flag);
    std::fs::create_dir_all(menu_path.parent().expect("has parent")).map_err(io_err(&menu_path))?;
    std::fs::write(&menu_path, b"1").map_err(io_err(&menu_path))?;
    build.screens.insert(menu_flag, menu_path);
    let nav_flag = format!("{UI_DIR}/{}", home_rows::NAV_FLAG);
    let nav_path = out.join(&nav_flag);
    std::fs::write(&nav_path, b"1").map_err(io_err(&nav_path))?;
    build.screens.insert(nav_flag, nav_path);
    match customisation_built(
        &routes_out.borrow(),
        &customisation_out.borrow(),
        customisation_error.borrow_mut().take(),
    ) {
        Ok(()) => {
            let rel = format!("{UI_DIR}/{}", home_rows::CUSTOMISATION_FLAG);
            let path = out.join(&rel);
            std::fs::write(&path, b"1").map_err(io_err(&path))?;
            build.screens.insert(rel, path);
            build.lines.push(
                "ui: the main menu's Customisation card and its Back to the main menu are in this build"
                    .to_owned(),
            );
        }
        Err(why) => build
            .warnings
            .push(format!("ui: no Customisation card on the main menu: {why}")),
    }
    if rows_built {
        let rel = format!("{UI_DIR}/{}", home_rows::FLAG);
        let path = out.join(&rel);
        let dir = path.parent().expect("has parent");
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        std::fs::write(&path, b"1").map_err(io_err(&path))?;
        build.screens.insert(rel, path);
    }
    if !mfd_rows.is_empty() {
        let rows = mfd_page_rows(&mfd_rows);
        for rel in mfd::TEMPLATE_ERPS {
            let (src, dst) = erp(rel);
            write_erp(
                &src,
                &dst,
                vec![(mfd::TEMPLATES, Box::new(|d| mfd::templates(d, &rows)))],
                packer,
            )?;
            build.erps.insert(ui_file(rel), dst);
        }
        build.lines.push(format!(
            "ADDONS MFD page: {} row(s) from {}",
            mfd_rows.len(),
            mfd_rows
                .iter()
                .map(|r| r.addon.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut eras = if season_picker_split(settings) {
        era_rows(input, writer, out, &mut build.lines, &mut build.warnings, packer)?
    } else {
        build.lines.push(
            "ui: the season picker keeps the game's single row (SwoftyMod's Split the season picker is off)"
                .into(),
        );
        Vec::new()
    };
    for rel in car_category_boxes(
        input,
        writer,
        out,
        &eras,
        &mut build.lines,
        &mut build.warnings,
        packer,
    )? {
        if !eras.contains(&rel) {
            eras.push(rel);
        }
    }
    let hubs = hub_tabs(
        settings,
        input,
        writer,
        out,
        &mut build.lines,
        &mut build.warnings,
        packer,
    )?;
    if lang {
        build.strings = popup.strings();
        build
            .strings
            .push((patch::FRIENDS_TAB_LABEL.into(), patch::FRIENDS_TAB_TEXT.into()));
    }
    for rel in ERPS
        .iter()
        .filter(|r| !era::ARCHIVES.contains(r) && !car_category::ARCHIVES.contains(r) || eras.contains(r))
    {
        build.erps.insert(ui_file(rel), out.join(ui_file(rel)));
    }
    for rel in hubs {
        build.erps.insert(ui_file(rel), out.join(ui_file(rel)));
    }
    for (name, data) in &built.order {
        let rel = format!("{UI_DIR}/{name}.ui");
        let path = out.join(&rel);
        let dir = path.parent().expect("has parent");
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        std::fs::write(&path, data).map_err(io_err(&path))?;
        build.screens.insert(rel, path);
        build.session.push(SessionScreen {
            name: name.clone(),
            page: None,
            addon: None,
            file: screen_file(name),
            flow: FLOW.into(),
            anchor: Some(ANCHOR.into()),
            view_model: built.view_models.contains(name),
        });
    }
    Ok(build)
}

#[cfg(test)]
mod tests;
