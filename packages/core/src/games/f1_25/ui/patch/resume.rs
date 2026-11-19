//! The Resume Career list's routes: into the list from the Career menu's Resume Career press, and out of it to the
//! stock continue screens or back to the Career menu.

use swofty_formats::pkg::Node;

use super::routes::{id_of, migrate_route_with, route_cmd, route_group, screen_routes};
use super::setup::{SETUP_SCENE, add_route, cmd_attr, cmd_type};
use crate::Result;
use crate::ui::node::*;
use crate::ui::pages::{CONTINUE_DRIVER, CONTINUE_MY_TEAM, RESUME_ROUTE, RESUME_SCREEN};

/// The stock groups the Resume Career press can come from, and the route of each that continues the Driver Career.
pub const ENTRY_GROUPS: [&str; 2] = ["fe_career", "fe_career_dlc_select"];

pub const RESUME_CONTINUE_SCREEN: &str = "career_continue_driver";
const DRIVER_SCREEN: &str = RESUME_CONTINUE_SCREEN;
const MY_TEAM_SCREEN: &str = "career_continue_my_team";
const MENU_SCREEN: &str = "fe_career";

pub struct ResumeRoutes {
    /// The flow that loads flow_frontend.erp, which holds the continue screens and the Career menu.
    pub frontend_flow: String,
}

/// The list screen's route group: its own open and close, `back` to the Career menu, and the two continue routes
/// under their stock names (so the core's career start gate sees a continue as it does from the Career menu), each
/// leaving for the stock continue screen of that mode.
pub fn resume_route_group(r: &ResumeRoutes) -> Node {
    let mut routes = screen_routes(RESUME_SCREEN);
    let mut back = migrate_route_with(
        "back",
        RESUME_SCREEN,
        &r.frontend_flow,
        MENU_SCREEN,
        ("open_no_input", ""),
        2,
    );
    super::setup::no_auto_focus(&mut back);
    routes.push(back);
    for (id, screen) in [
        (CONTINUE_DRIVER, DRIVER_SCREEN),
        (CONTINUE_MY_TEAM, MY_TEAM_SCREEN),
    ] {
        routes.push(migrate_route_with(
            id,
            RESUME_SCREEN,
            &r.frontend_flow,
            screen,
            ("open_no_input", ""),
            1,
        ));
    }
    route_group(RESUME_SCREEN, routes)
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

/// Adds the list's route group and its entry route to each of [`ENTRY_GROUPS`] these routes hold. Returns each
/// route added, as `<group>.<route>`.
pub fn resume_routes(data: &[u8], r: &ResumeRoutes, placed: &mut Vec<String>) -> Result<Vec<u8>> {
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        if gs.iter().any(|g| id_of(g) == Some(RESUME_SCREEN)) {
            return Ok(());
        }
        gs.push(resume_route_group(r));
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
