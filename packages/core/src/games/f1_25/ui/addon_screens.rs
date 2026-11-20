//! The screens a menu build adds to ADDON SETTINGS beside its pages: the addons' options pages and career
//! setup pages, the routes that open each screen, and the checks every screen passes.

use std::path::PathBuf;

use swofty_formats::pkg::Node;

use super::career_row;
use super::node::{boolv, err, s, uidb_var};
use super::pages::{self, Entry, HomePage, Screens, Sources};
use super::{
    ANCHOR, FLOW, MfdRow, SETUP_BACK_ERP, SETUP_FLOW_ERP, catalog, check, patch, screen_flow, ui_file,
};
use crate::addons::settings::SessionSettings;
use crate::manifest::CAREER_SETUP_BACK;
use crate::{Error, Result};

/// Builds the options page of each addon screen with `page: options` and adds it to `built`, named
/// swofty_addon_<name>_<addon index>, since the view model reads the index after the last '_'. Returns
/// each one's screen name, anchor and page name.
pub(super) fn options_pages(
    settings: &SessionSettings,
    src: &Sources,
    flow: &[Entry],
    built: &mut Screens,
) -> Result<Vec<(String, String, String)>> {
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
            .push((screen.clone(), pages::build_options_page(src, e, &screen)?));
        if !e.options.is_empty() {
            built.view_models.push(screen.clone());
        }
        generated.push((s.name.clone(), anchor, screen));
    }
    Ok(generated)
}

/// Builds the home and rows screens of each addon's career setup page and adds them to `built`. Returns
/// the routes each setup needs, found in the game's flows, and the line the build logs for each.
pub(super) fn career_setups(
    input: &dyn Fn(&str) -> PathBuf,
    src: &Sources,
    flow: &[Entry],
    built: &mut Screens,
) -> Result<(Vec<patch::SetupRoutes>, Vec<String>)> {
    let flows_ui = match flow.iter().any(|e| e.setup.is_some()) {
        true => super::archives::read_frag(&input(&ui_file("persistent.erp")), super::FLOWS, 0)?,
        false => Vec::new(),
    };
    let mut setups: Vec<patch::SetupRoutes> = Vec::new();
    let mut setup_lines = Vec::new();
    for e in flow.iter().filter(|e| e.kind == pages::Kind::Addon) {
        let Some(setup) = e.setup else { continue };
        let (home, home_ui) = pages::build_setup_home(src, e, setup)?;
        let (rows, rows_ui) = pages::build_setup_page(src, e, setup)?;
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
    Ok((setups, setup_lines))
}

/// The Resume Career list screen of a build: its routes, and the line the build logs for it.
pub(super) struct Resume {
    pub(super) routes: patch::ResumeRoutes,
    pub(super) line: String,
}

/// Builds the Career list screen for the rows the session has, which may be none, and adds it to `built`. `None`
/// when the list is off: the stock Career menu then stays as it is.
pub(super) fn resume_screen(
    settings: &SessionSettings,
    input: &dyn Fn(&str) -> PathBuf,
    src: &Sources,
    built: &mut Screens,
) -> Result<Option<Resume>> {
    if !settings.resume_list_on {
        return Ok(None);
    }
    let flows_ui = super::archives::read_frag(&input(&ui_file("persistent.erp")), super::FLOWS, 0)?;
    let frontend_flow = screen_flow(input, &flows_ui, SETUP_BACK_ERP, patch::RESUME_CONTINUE_SCREEN)?;
    let (name, ui) = pages::build_resume_page(src, &settings.resume_list)?;
    let stock = super::archives::read_frag(&input(&ui_file("flow_options.erp")), career_row::STOCK, 0)?;
    let problems = career_row::layout_problems(&ui, &career_row::row_scene(&stock)?, &stock)?;
    if !problems.is_empty() {
        return Err(Error::Other(format!(
            "the Career list rows break their scene: {}",
            problems.join("; ")
        )));
    }
    built.order.push((name.clone(), ui));
    built.view_models.push(name.clone());
    let line = format!(
        "ui: the Career list screen {name} has {} career row(s); the Career tile opens it, the route {} opens it from {}, and a row's \
         press routes to the stock continue screen of its mode in {frontend_flow}",
        settings.resume_list.len(),
        pages::RESUME_ROUTE,
        patch::ENTRY_GROUPS.join(" and ")
    );
    Ok(Some(Resume {
        routes: patch::ResumeRoutes { frontend_flow },
        line,
    }))
}

/// How the routes reach the addon screens in the options flow.
pub(super) struct Routing {
    /// Every screen the options flow lists: the built ones, then the addons' own.
    pub(super) listed: Vec<String>,
    /// The screen each anchor route opens, by screen name and anchor.
    pub(super) opened: Vec<(String, String, String)>,
    /// The route group of each addon screen and its anchor.
    pub(super) addon_groups: Vec<(String, String)>,
    /// The route groups of the career setups.
    pub(super) setup_groups: Vec<Node>,
}

impl Routing {
    /// Addon screens in the options flow get the flow entry, region and route group an ADDON SETTINGS
    /// page gets, so routes and signals to them open them.
    pub(super) fn of(
        settings: &SessionSettings,
        built: &Screens,
        generated: &[(String, String, String)],
        setups: &[patch::SetupRoutes],
    ) -> Self {
        let names: Vec<String> = built.order.iter().map(|(n, _)| n.clone()).collect();
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
        Self {
            listed,
            opened,
            addon_groups,
            setup_groups,
        }
    }
}

/// What the checks find wrong in each built screen.
pub(super) fn screen_problems(built: &Screens) -> Result<Vec<String>> {
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
    Ok(problems)
}

/// The route groups a menu build adds: the home screens, each page's open route, and each career
/// setup's screens and routes.
pub(super) struct Targets {
    pub(super) homes: Vec<HomePage>,
    pub(super) pages: Vec<(String, String, String)>,
    pub(super) setup_ids: Vec<String>,
}

impl Targets {
    pub(super) fn of(flow: &[Entry], setups: &[patch::SetupRoutes], resume: Option<&Resume>) -> Self {
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
        for s in setups {
            setup_ids.extend([
                s.home.clone(),
                s.rows.clone(),
                format!("{}.{}", s.back, s.route),
                format!("{}.{}", s.next, s.back_route),
            ]);
        }
        if resume.is_some() {
            setup_ids.push(pages::RESUME_SCREEN.to_owned());
            setup_ids.extend(
                patch::ENTRY_GROUPS
                    .iter()
                    .map(|g| format!("{g}.{}", pages::RESUME_ROUTE)),
            );
        }
        Self {
            homes,
            pages,
            setup_ids,
        }
    }

    /// Every route group name, in the order the route check reads them.
    pub(super) fn groups(&self) -> Vec<&str> {
        let mut groups: Vec<&str> = self.homes.iter().map(|h| h.screen.as_str()).collect();
        groups.push("fe_home.mod_menu");
        groups.extend(self.pages.iter().map(|(_, s, _)| s.as_str()));
        groups.extend(self.setup_ids.iter().map(String::as_str));
        groups
    }
}

/// The ADDON SETTINGS state ui_data.uidb holds: each page's values, then the MFD page's rows when an
/// addon shows any.
pub(super) fn ui_state(flow: &[Entry], mfd_rows: &[MfdRow]) -> Result<Vec<(String, Node)>> {
    let mut state = pages::state_entries(flow)?;
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
    Ok(state)
}
