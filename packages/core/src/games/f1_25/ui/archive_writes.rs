//! The game archives a menu build writes into the overlay, the flag files that tell the core what the
//! build has, and the screens it registers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_formats::pkg::Node;

use super::addon_screens::{Resume, Routing, Targets};
use super::archives::{ArchiveWriter, read_frag, write_archive, write_erp};
use super::menu_rows::{career_row, customisation_built, main_menu_rows};
use super::pages::{ADJUST_TOOLTIP, Screens};
use super::{
    ANCHOR, CUSTOMISATION_ROUTES, ERPS, F1_WORLD_ROUTES, FE_HOME, FLOW, FLOWS, FRONTEND_TRANSITION,
    GAME_ROUTES, HUD_MESSAGES, LEADERBOARD_UI, MESSAGES, MenuBuild, MfdRow, OPTIONS_FLOW, OVERRIDES,
    PERSISTENT_UI, Patch, REGIONS, ROUTES, TOOLTIPS, UI_DATA, WIZARD_ROUTES, Writer, car_category,
    career_row, career_select, check, era, home_rows, icons, mfd, mfd_page_rows, patch, rows, ui_file,
};
use crate::addons::settings::{SessionScreen, UI_DIR, screen_file};
use crate::{Error, Result, io_err};

/// What writing persistent.erp leaves for the checks and flags after it: each routes file and the flows
/// file as written, the customisation routes or why the game's own were kept, and the stock route groups
/// and screens the career setup routes went into.
#[derive(Default)]
pub(super) struct Persistent {
    pub(super) routes: Vec<u8>,
    pub(super) game_routes: Vec<u8>,
    pub(super) wizard_routes: Vec<u8>,
    pub(super) flows: Vec<u8>,
    pub(super) customisation: Vec<u8>,
    pub(super) customisation_error: Option<String>,
    pub(super) placed: patch::Placed,
}

/// Writes persistent.erp from `at.0` to `at.1`: the routes, flows and regions that reach every screen,
/// the career setup routes, the customisation routes, the MFD page's metadata, and the addon tiles'
/// icons and row layout, so they are loaded wherever ADDON SETTINGS shows.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_persistent(
    at: &(PathBuf, PathBuf),
    flow_options: &Path,
    routing: &Routing,
    targets: &Targets,
    setups: &[patch::SetupRoutes],
    resume: Option<&Resume>,
    mfd: bool,
    archives: &ArchiveWriter,
) -> Result<Persistent> {
    let (src_p, dst_p) = at;
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
                let mut groups = patch::route_groups(&targets.homes, &targets.pages);
                groups.extend(routing.setup_groups.iter().cloned());
                let f1_world = read_frag(src_p, F1_WORLD_ROUTES, 0).unwrap_or_default();
                let b = patch::routes(
                    d,
                    groups,
                    patch::screen_route_groups(&routing.addon_groups),
                    &f1_world,
                )?;
                let b = patch::setup_routes(&b, setups, &mut placed.borrow_mut())?;
                let b = match resume {
                    Some(r) => patch::resume_routes(&b, &r.routes, &mut placed.borrow_mut().routes)?,
                    None => b,
                };
                *routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
        (
            FLOWS,
            Box::new(|d| {
                let b = patch::flow_entries(d, OPTIONS_FLOW, &routing.listed)?;
                let b = patch::f1_world_entries(&b)?;
                *flows_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
        (REGIONS, Box::new(|d| patch::regions(d, &routing.listed))),
        (
            GAME_ROUTES,
            Box::new(|d| {
                let b = patch::anchor_routes(d, &routing.opened)?;
                let b = patch::setup_routes(&b, setups, &mut placed.borrow_mut())?;
                *game_routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ),
    ];
    persistent.push((F1_WORLD_ROUTES, Box::new(patch::f1_world_routes)));
    persistent.push((
        CUSTOMISATION_ROUTES,
        Box::new(|d| {
            let built = read_frag(src_p, F1_WORLD_ROUTES, 0).and_then(|f| patch::customisation_routes(d, &f));
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
                let b = patch::setup_routes(d, setups, &mut placed.borrow_mut())?;
                *wizard_routes_out.borrow_mut() = b.clone();
                Ok(b)
            }),
        ));
    }
    if mfd {
        persistent.push((mfd::METADATA, Box::new(mfd::metadata)));
    }
    let [view, surface] = icons::resources(
        &read_frag(flow_options, icons::STOCK, 0)?,
        &read_frag(flow_options, &format!("{}.image", icons::STOCK), 0)?,
    )?;
    let persistent = persistent.into_iter().map(|(name, f)| (name, 0, f)).collect();
    let mut added = vec![
        (view.0, era::SRV_KIND, view.1),
        (surface.0, era::SURFACE_KIND, surface.1),
        (
            rows::O_ROW.to_owned(),
            "UILayout",
            vec![rows::row_scene(&read_frag(flow_options, rows::O_BLOCK, 0)?)?],
        ),
    ];
    if resume.is_some() {
        added.push((
            career_row::SCENE.to_owned(),
            "UILayout",
            vec![career_row::row_scene(&read_frag(
                flow_options,
                career_row::STOCK,
                0,
            )?)?],
        ));
    }
    write_archive(src_p, dst_p, persistent, added, archives)?;
    Ok(Persistent {
        routes: routes_out.into_inner(),
        game_routes: game_routes_out.into_inner(),
        wizard_routes: wizard_routes_out.into_inner(),
        flows: flows_out.into_inner(),
        customisation: customisation_out.into_inner(),
        customisation_error: customisation_error.into_inner(),
        placed: placed.into_inner(),
    })
}

/// Refuses a career setup whose routes found no route group of the game's to go into.
pub(super) fn check_setup_routes(setups: &[patch::SetupRoutes], placed: &patch::Placed) -> Result<()> {
    for s in setups {
        for id in [
            format!("{}.{}", s.back, s.route),
            format!("{}.{}", s.next, s.back_route),
        ] {
            if !placed.routes.contains(&id) {
                return Err(Error::Other(format!(
                    "career setup {}: the game's routes have no route group {} for {id}",
                    s.route,
                    id.split('.').next().unwrap_or_default()
                )));
            }
        }
    }
    Ok(())
}

/// Refuses a resume list whose entry route found no stock route group to go into.
pub(super) fn check_resume_routes(resume: Option<&Resume>, placed: &patch::Placed) -> Result<()> {
    if resume.is_none() {
        return Ok(());
    }
    let ours = |r: &&String| r.ends_with(&format!(".{}", crate::ui::pages::RESUME_ROUTE));
    if placed.routes.iter().filter(ours).count() == 0 {
        return Err(Error::Other(format!(
            "the Resume Career list: the game's routes have none of {} for its entry route",
            patch::ENTRY_GROUPS.join(", ")
        )));
    }
    Ok(())
}

/// What the route check finds wrong in the routes and flows persistent.erp now has.
pub(super) fn route_problems(
    p: &Persistent,
    routing: &Routing,
    targets: &Targets,
    setups: &[patch::SetupRoutes],
    resume: Option<&Resume>,
) -> Result<Vec<String>> {
    let registered: BTreeMap<String, String> = routing
        .listed
        .iter()
        .map(|n| (n.clone(), FLOW.to_owned()))
        .collect();
    let stock_screens: BTreeSet<String> = setups
        .iter()
        .flat_map(|s| [s.back.clone(), s.next.clone()])
        .chain(p.placed.screens.iter().cloned())
        .chain(resume.iter().flat_map(|_| {
            [
                "fe_career",
                "fe_career_dlc_select",
                patch::RESUME_CONTINUE_SCREEN,
                "career_continue_my_team",
            ]
            .map(str::to_owned)
        }))
        .collect();
    let mut route_files: Vec<&[u8]> = vec![&p.routes, &p.game_routes];
    if !p.wizard_routes.is_empty() {
        route_files.push(&p.wizard_routes);
    }
    check::route_files(
        &route_files,
        &p.flows,
        &targets.groups(),
        &registered,
        &stock_screens,
    )
}

/// Writes databases.erp: the adjust tooltip, ADDON SETTINGS' state and the icon overrides.
pub(super) fn write_databases(
    at: &(PathBuf, PathBuf),
    state: Vec<(String, Node)>,
    archives: &ArchiveWriter,
) -> Result<()> {
    write_erp(
        &at.0,
        &at.1,
        vec![
            (TOOLTIPS, Box::new(|d| patch::tooltips(d, ADJUST_TOOLTIP))),
            (UI_DATA, Box::new(move |d| patch::ui_data(d, state))),
            (OVERRIDES, Box::new(icons::overrides)),
        ],
        archives,
    )
}

/// Writes flow_persistent.erp: the online messages, the HUD messages and the popup gate, refused when
/// the gate would change the game's own popups.
pub(super) fn write_flow_persistent(
    at: &(PathBuf, PathBuf),
    lang: bool,
    archives: &ArchiveWriter,
) -> Result<()> {
    write_erp(
        &at.0,
        &at.1,
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
        archives,
    )
}

/// What writing flow_frontend.erp logged, and whether it built the main menu rows.
pub(super) struct Frontend {
    pub(super) lines: Vec<String>,
    pub(super) warnings: Vec<String>,
    pub(super) rows_built: bool,
}

/// Writes flow_frontend.erp: the home screen with its offline panel, shown or hidden, the transition,
/// the leaderboard glyphs, the career select's addon row and, with SWOFTY_HOME_ROWS set, the main menu
/// rows.
pub(super) fn write_frontend(
    at: &(PathBuf, PathBuf),
    writer: &Writer,
    show_panel: bool,
    archives: &ArchiveWriter,
) -> Result<Frontend> {
    let (src_h, dst_h) = at;
    let (mut career_lines, mut career_warnings) = (Vec::new(), Vec::new());
    let career = career_row(src_h, writer, &mut career_lines, &mut career_warnings)?;
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
            src_h,
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
    write_archive(src_h, dst_h, frontend, Vec::new(), archives)?;
    career_lines.extend(home_lines);
    career_warnings.extend(home_warnings);
    Ok(Frontend {
        lines: career_lines,
        warnings: career_warnings,
        rows_built,
    })
}

/// Writes the leaderboard container with the leaderboard glyphs.
pub(super) fn write_leaderboard(at: &(PathBuf, PathBuf), archives: &ArchiveWriter) -> Result<()> {
    write_erp(
        &at.0,
        &at.1,
        vec![(LEADERBOARD_UI, Box::new(patch::leaderboard_glyphs))],
        archives,
    )
}

/// Writes the player card archive with its strip hidden, unless the strip has a binding the build can't
/// read.
pub(super) fn write_playercard(at: &(PathBuf, PathBuf), archives: &ArchiveWriter) -> Result<()> {
    write_archive(
        &at.0,
        &at.1,
        vec![(
            home_rows::PLAYERCARD,
            0,
            Box::new(|d| match home_rows::has_strip_binding(d) {
                Ok(false) => home_rows::hide_strip(d),
                _ => Ok(d.to_vec()),
            }),
        )],
        Vec::new(),
        archives,
    )
}

/// Writes the flag files that tell the core the main menu, its navigation, the Customisation card and
/// the main menu rows are in this build, and logs whether the Customisation card is.
pub(super) fn write_flags(out: &Path, build: &mut MenuBuild, p: Persistent, rows_built: bool) -> Result<()> {
    let menu_flag = format!("{UI_DIR}/{}", home_rows::MENU_FLAG);
    let menu_path = out.join(&menu_flag);
    std::fs::create_dir_all(menu_path.parent().expect("has parent")).map_err(io_err(&menu_path))?;
    std::fs::write(&menu_path, b"1").map_err(io_err(&menu_path))?;
    build.screens.insert(menu_flag, menu_path);
    let nav_flag = format!("{UI_DIR}/{}", home_rows::NAV_FLAG);
    let nav_path = out.join(&nav_flag);
    std::fs::write(&nav_path, b"1").map_err(io_err(&nav_path))?;
    build.screens.insert(nav_flag, nav_path);
    match customisation_built(&p.routes, &p.customisation, p.customisation_error) {
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
    Ok(())
}

/// Writes the MFD page's rows into each archive that holds its templates, and logs which addons show
/// rows there.
pub(super) fn write_mfd_templates(
    input: &dyn Fn(&str) -> PathBuf,
    out: &Path,
    mfd_rows: &[MfdRow],
    build: &mut MenuBuild,
    archives: &ArchiveWriter,
) -> Result<()> {
    let rows = mfd_page_rows(mfd_rows);
    for rel in mfd::TEMPLATE_ERPS {
        let (src, dst) = (input(&ui_file(rel)), out.join(ui_file(rel)));
        write_erp(
            &src,
            &dst,
            vec![(mfd::TEMPLATES, Box::new(|d| mfd::templates(d, &rows)))],
            archives,
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
    Ok(())
}

/// Lists the archives the build wrote, leaving out the era and car category archives it did not edit,
/// then writes each built screen and lists it for the session.
pub(super) fn register(
    out: &Path,
    build: &mut MenuBuild,
    eras: &[&str],
    hubs: Vec<&str>,
    built: &Screens,
) -> Result<()> {
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
    Ok(())
}
