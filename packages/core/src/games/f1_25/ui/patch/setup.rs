//! The career setups' routes: into a setup from the content select, and back out of the wizard.

use swofty_formats::pkg::Node;

use super::routes::{id_of, migrate_route_with, route, route_cmd, route_group, screen_routes, subpage_route};
use crate::Result;
use crate::ui::node::*;

/// The Driver Career tile's route to the season length step, which a career setup's entry route copies.
pub const SETUP_ENTRY_SOURCE: &str = "career_wizard_driver_career_to_f1";

/// The message the career menu opens the career content select with for Driver Career.
pub const DRIVER_CAREER: &str = "driver_career";

pub struct SetupRoutes {
    pub route: String,
    pub home: String,
    pub rows: String,
    pub next: String,
    pub next_flow: String,
    pub back: String,
    pub back_flow: String,
    pub back_route: String,
}

/// The route groups of a career setup's home and rows page. The home's `confirm` closes it, moves to the
/// wizard's flow and opens `next`; its `back` moves to the content select's flow and opens it for Driver Career.
pub fn setup_route_groups(s: &SetupRoutes) -> Vec<Node> {
    let mut home = screen_routes(&s.home);
    let mut back = migrate_route_with("back", &s.home, &s.back_flow, &s.back, ("open", DRIVER_CAREER), 2);
    no_auto_focus(&mut back);
    home.push(back);
    home.push(migrate_route_with(
        crate::ui::pages::SETUP_CONFIRM,
        &s.home,
        &s.next_flow,
        &s.next,
        ("open", ""),
        1,
    ));
    home.push(subpage_route(
        crate::ui::pages::SETUP_OPTIONS,
        &s.home,
        &s.rows,
        1,
    ));
    let mut rows = screen_routes(&s.rows);
    rows.push(subpage_route("back", &s.rows, &s.home, 2));
    vec![route_group(&s.home, home), route_group(&s.rows, rows)]
}

pub(super) fn entry_route(group: &Node, s: &SetupRoutes) -> Result<Node> {
    let mut r = group
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some(SETUP_ENTRY_SOURCE))
        .cloned()
        .ok_or_else(|| err(format!("{} has no {SETUP_ENTRY_SOURCE} route", s.back)))?;
    let from = r
        .o()?
        .at("Attributes")?
        .a()?
        .iter()
        .filter_map(Node::as_obj)
        .find(|a| a.str_at("Id").ok() == Some("Destination"))
        .and_then(|a| a.str_at("Value").ok())
        .map(str::to_owned)
        .ok_or_else(|| err(format!("{SETUP_ENTRY_SOURCE} has no destination")))?;
    replace_strings(&mut r, &|v| match v {
        v if v == from => Some(s.home.clone()),
        "career_season_setup_flow" => Some("options_flow".into()),
        "open_no_input" => Some("open".into()),
        _ => None,
    });
    set_str(r.o_mut()?, "Id", &s.route)?;
    let cmds = r.o_mut()?.at_mut("Commands")?.a_mut()?;
    let id = cmds
        .iter()
        .filter_map(|c| cmd_attr(c, "Id")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let wiped = cmds
        .iter()
        .find(|c| cmd_type(c) == Some("TriggerSubRoute") && cmd_attr(c, "SubRoute") == Some("close"))
        .and_then(|c| c.as_obj()?.get("Dependencies")?.as_val()?.as_str())
        .map(str::to_owned);
    cmds.push(route_cmd(
        "FrontendSceneSwitch",
        id,
        &SETUP_SCENE,
        wiped.as_deref(),
    ));
    Ok(r)
}

/// The stock CUSTOM settings page's backdrop, which every route into a career setup page switches to.
pub(super) const SETUP_SCENE: [(&str, &str); 4] = [
    ("Target", ""),
    ("SceneOverride", "Customisation"),
    ("SubSceneOverride", "Generic"),
    ("CameraOverride", "Main"),
];

pub(super) fn cmd_type(c: &Node) -> Option<&str> {
    c.as_obj()?.get("Type")?.as_val()?.as_str()
}

pub(super) fn cmd_attr<'a>(c: &'a Node, key: &str) -> Option<&'a str> {
    c.as_obj()?
        .get("Attributes")?
        .as_arr()?
        .items
        .iter()
        .filter_map(Node::as_obj)
        .find(|a| a.str_at("Id").ok() == Some(key))?
        .str_at("Value")
        .ok()
}

pub(super) fn no_auto_focus(r: &mut Node) {
    if let Some(a) = r
        .as_obj_mut()
        .and_then(|o| o.get_mut("Attributes"))
        .and_then(Node::as_arr_mut)
    {
        a.items
            .push(obj(vec![("Id", s("AutoFocus")), ("Value", s("false"))]));
    }
}

/// The way back from the wizard step `next` to the home, as the wizard's own routes leave its flow: `next` and the
/// slices its group opens transition out, the slices close, and the flow moves to the options flow.
pub(super) fn wizard_exit_route(s: &SetupRoutes, slices: &[&str]) -> Node {
    let out = |id: u32, target: &str| {
        route_cmd(
            "TransitionScreen",
            id,
            &[("Direction", "out"), ("Target", target)],
            None,
        )
    };
    let mut cmds = vec![out(1, &s.next)];
    let mut done = vec!["1".to_owned()];
    let mut id = 1;
    for slice in slices {
        cmds.push(out(id + 1, slice));
        cmds.push(route_cmd(
            "CloseScreen",
            id + 2,
            &[("Target", slice)],
            Some(&(id + 1).to_string()),
        ));
        done.push((id + 2).to_string());
        id += 2;
    }
    let migrate = (id + 1).to_string();
    cmds.push(route_cmd(
        "Migrate",
        id + 1,
        &[("Flow", "options_flow"), ("SkipActivator", "true")],
        Some(&done.join(",")),
    ));
    let open = (id + 2).to_string();
    cmds.push(route_cmd(
        "TriggerSubRoute",
        id + 2,
        &[
            ("SubRoute", "open"),
            ("Target", &s.home),
            ("From", &s.next),
            ("Message", ""),
        ],
        Some(&migrate),
    ));
    cmds.push(route_cmd(
        "FrontendSceneSwitch",
        id + 3,
        &SETUP_SCENE,
        Some(&migrate),
    ));
    let mut r = route(&s.back_route, cmds, &open, 2, Some(&s.home));
    no_auto_focus(&mut r);
    r
}

pub(super) fn slices_of(g: &Node) -> Vec<String> {
    g.as_obj()
        .and_then(|o| o.get("Attributes"))
        .and_then(Node::as_arr)
        .into_iter()
        .flat_map(|a| &a.items)
        .filter_map(Node::as_obj)
        .filter(|a| a.str_at("Id").ok() == Some("Slices"))
        .filter_map(|a| a.str_at("Value").ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn add_route(g: &mut Node, r: Node) -> Result<()> {
    let gid = id_of(g).unwrap_or_default().to_owned();
    let rid = id_of(&r).unwrap_or_default().to_owned();
    let routes = g.o_mut()?.at_mut("Routes")?.a_mut()?;
    if routes.iter().any(|x| id_of(x) == Some(&rid)) {
        return Err(err(format!("{gid} has a route {rid} already")));
    }
    routes.push(r);
    Ok(())
}

#[derive(Debug, Default)]
pub struct Placed {
    /// Each route added, as `<group>.<route>`.
    pub routes: Vec<String>,
    /// The game screens the added routes name: the slices of each `next` screen's group.
    pub screens: Vec<String>,
}

/// Adds each career setup's entry route to the content select's route group and its back route to the group of
/// its `next` screen, for the groups these routes hold.
pub fn setup_routes(data: &[u8], setups: &[SetupRoutes], placed: &mut Placed) -> Result<Vec<u8>> {
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        for s in setups {
            if let Some(g) = gs.iter_mut().find(|g| id_of(g) == Some(&s.back)) {
                let r = entry_route(g, s)?;
                add_route(g, r)?;
                placed.routes.push(format!("{}.{}", s.back, s.route));
            }
            if let Some(g) = gs.iter_mut().find(|g| id_of(g) == Some(&s.next)) {
                let slices = slices_of(g);
                let names: Vec<&str> = slices.iter().map(String::as_str).collect();
                add_route(g, wizard_exit_route(s, &names))?;
                placed.routes.push(format!("{}.{}", s.next, s.back_route));
                placed.screens.extend(slices);
            }
        }
        Ok(())
    })
}
