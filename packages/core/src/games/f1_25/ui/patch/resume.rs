//! The Career list's routes: into the list from the main menu's Career tile (and from the Career menu's Resume
//! Career press), and out of it to the stock continue screens, to the Career menu for a new career, or back to the
//! main menu.

use swofty_formats::pkg::Node;

use super::routes::{id_of, migrate_route, migrate_route_with, route_cmd, route_group, screen_routes};
use super::setup::{SETUP_SCENE, add_route, cmd_attr, cmd_type};
use crate::Result;
use crate::ui::node::*;
use crate::ui::pages::{CONTINUE_DRIVER, CONTINUE_MY_TEAM, NEW_CAREER_ROUTE, RESUME_ROUTE, RESUME_SCREEN};

/// The stock groups the Resume Career press can come from, and the route of each that continues the Driver Career.
pub const ENTRY_GROUPS: [&str; 2] = ["fe_career", "fe_career_dlc_select"];

pub const RESUME_CONTINUE_SCREEN: &str = "career_continue_driver";
const DRIVER_SCREEN: &str = RESUME_CONTINUE_SCREEN;
const HOME: &str = "fe_home";
const HOME_CAREER: &str = "career";
const HOME_OPTIONS: &str = "options";
const CAREER_MENU: &str = "fe_career";

pub struct ResumeRoutes {
    /// The flow that loads flow_frontend.erp, which holds the continue screens and the Career menu.
    pub frontend_flow: String,
}

fn group<'a>(gs: &'a [Node], id: &str) -> Result<&'a Node> {
    gs.iter()
        .find(|g| id_of(g) == Some(id))
        .ok_or_else(|| err(format!("the routes have no {id} grouping")))
}

fn stock_route(gs: &[Node], gid: &str, rid: &str) -> Result<Node> {
    group(gs, gid)?
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some(rid))
        .cloned()
        .ok_or_else(|| err(format!("{gid} has no {rid} route")))
}

fn destination(route: &Node) -> Result<String> {
    route
        .as_obj()
        .and_then(|o| o.get("Attributes"))
        .and_then(Node::as_arr)
        .and_then(|a| {
            a.items
                .iter()
                .filter_map(Node::as_obj)
                .find(|a| a.str_at("Id").ok() == Some("Destination"))
        })
        .and_then(|a| a.str_at("Value").ok())
        .map(str::to_owned)
        .ok_or_else(|| err(format!("{HOME}.{HOME_CAREER} has no Destination")))
}

/// A stock continue route as it leaves the list: the same commands, closing and opening from the list screen, with
/// the move to the frontend flow between them.
fn continue_route(stock: &Node, flow: &str) -> Result<Node> {
    let mut r = stock.clone();
    replace_strings(&mut r, &|v| (v == CAREER_MENU).then(|| RESUME_SCREEN.to_owned()));
    let cmds = r.o_mut()?.at_mut("Commands")?.a_mut()?;
    let next = cmds
        .iter()
        .filter_map(|c| cmd_attr(c, "Id")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let close_id = cmds
        .iter()
        .find(|c| cmd_type(c) == Some("TriggerSubRoute") && cmd_attr(c, "SubRoute") == Some("close"))
        .and_then(|c| cmd_attr(c, "Id"))
        .ok_or_else(|| err("a stock continue route has no close step"))?
        .to_owned();
    let migrate = next.to_string();
    let open = cmds
        .iter_mut()
        .find(|c| cmd_type(c) == Some("TriggerSubRoute") && cmd_attr(c, "SubRoute") == Some("open_no_input"))
        .ok_or_else(|| err("a stock continue route has no open step"))?;
    set_str(open.o_mut()?, "Dependencies", &migrate)?;
    cmds.push(route_cmd(
        "Migrate",
        next,
        &[("Flow", flow), ("SkipActivator", "true")],
        Some(&close_id),
    ));
    Ok(r)
}

/// The list screen's route group: its own open and close, `back` to the home menu, `swofty_new_career` to the
/// Career menu the Career tile opened before the list took its place (`new_career` is that screen), and the two
/// continue routes under their stock names (so the core's career start gate sees a continue as it does from the
/// Career menu), each the stock route of its mode leaving the list.
pub fn resume_route_group(r: &ResumeRoutes, new_career: &str, driver: &Node, my_team: &Node) -> Result<Node> {
    let mut routes = screen_routes(RESUME_SCREEN);
    routes.push(migrate_route("back", RESUME_SCREEN, &r.frontend_flow, HOME));
    routes.push(migrate_route_with(
        NEW_CAREER_ROUTE,
        RESUME_SCREEN,
        &r.frontend_flow,
        new_career,
        ("open", ""),
        1,
    ));
    routes.push(continue_route(driver, &r.frontend_flow)?);
    routes.push(continue_route(my_team, &r.frontend_flow)?);
    Ok(route_group(RESUME_SCREEN, routes))
}

/// The main menu's `career` route, opening the list the way `options` opens ADDON SETTINGS.
fn home_career_route(gs: &[Node]) -> Result<Node> {
    let mut r = stock_route(gs, HOME, HOME_OPTIONS)?;
    set_str(r.o_mut()?, "Id", HOME_CAREER)?;
    replace_strings(&mut r, &|v| match v {
        "settings_container" => Some(RESUME_SCREEN.into()),
        "open_no_input" => Some("open".into()),
        _ => None,
    });
    Ok(r)
}

fn entry_route(group: &Node) -> Result<Node> {
    let gid = id_of(group).unwrap_or_default().to_owned();
    let mut r = group
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some(CONTINUE_DRIVER))
        .cloned()
        .ok_or_else(|| err(format!("{gid} has no {CONTINUE_DRIVER} route")))?;
    set_str(r.o_mut()?, "Id", RESUME_ROUTE)?;
    replace_strings(&mut r, &|v| match v {
        DRIVER_SCREEN => Some(RESUME_SCREEN.into()),
        "open_no_input" => Some("open".into()),
        _ => None,
    });
    let cmds = r.o_mut()?.at_mut("Commands")?.a_mut()?;
    let next = cmds
        .iter()
        .filter_map(|c| cmd_attr(c, "Id")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let close = cmds
        .iter()
        .find(|c| cmd_type(c) == Some("TriggerSubRoute") && cmd_attr(c, "SubRoute") == Some("close"))
        .ok_or_else(|| err(format!("{gid}.{CONTINUE_DRIVER} has no close step")))?;
    let (close_id, wiped) = (
        cmd_attr(close, "Id").unwrap_or_default().to_owned(),
        close
            .as_obj()
            .and_then(|o| o.get("Dependencies")?.as_val()?.as_str())
            .map(str::to_owned),
    );
    let migrate = next.to_string();
    let open = cmds
        .iter_mut()
        .find(|c| cmd_type(c) == Some("TriggerSubRoute") && cmd_attr(c, "SubRoute") == Some("open"))
        .ok_or_else(|| err(format!("{gid}.{CONTINUE_DRIVER} has no open step")))?;
    set_str(open.o_mut()?, "Dependencies", &migrate)?;
    cmds.push(route_cmd(
        "Migrate",
        next,
        &[("Flow", "options_flow"), ("SkipActivator", "true")],
        Some(&close_id),
    ));
    cmds.push(route_cmd(
        "FrontendSceneSwitch",
        next + 1,
        &SETUP_SCENE,
        wiped.as_deref(),
    ));
    Ok(r)
}

/// Adds the list's route group, `fe_home.career` opening it with the game's own as `new_career`'s way to the
/// Career menu, and the entry route to each of [`ENTRY_GROUPS`] these routes hold. Returns each route added, as
/// `<group>.<route>`.
pub fn resume_routes(data: &[u8], r: &ResumeRoutes, placed: &mut Vec<String>) -> Result<Vec<u8>> {
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        if gs.iter().any(|g| id_of(g) == Some(RESUME_SCREEN)) {
            return Ok(());
        }
        let driver = stock_route(gs, CAREER_MENU, CONTINUE_DRIVER)?;
        let my_team = stock_route(gs, CAREER_MENU, CONTINUE_MY_TEAM)?;
        let home_career = stock_route(gs, HOME, HOME_CAREER)?;
        let new_career = destination(&home_career)?;
        let career_here = home_career_route(gs)?;
        gs.push(resume_route_group(r, &new_career, &driver, &my_team)?);
        let home = gs
            .iter_mut()
            .find(|g| id_of(g) == Some(HOME))
            .ok_or_else(|| err(format!("the routes have no {HOME} grouping")))?;
        let routes = home.o_mut()?.at_mut("Routes")?.a_mut()?;
        routes.retain(|x| id_of(x) != Some(HOME_CAREER));
        routes.push(career_here);
        placed.push(format!("{HOME}.{HOME_CAREER}"));
        for gid in ENTRY_GROUPS {
            if let Some(g) = gs.iter_mut().find(|g| id_of(g) == Some(gid)) {
                let route = entry_route(g)?;
                add_route(g, route)?;
                placed.push(format!("{gid}.{RESUME_ROUTE}"));
            }
        }
        Ok(())
    })
}
