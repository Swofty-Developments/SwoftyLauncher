//! routes_*.uir: ADDON SETTINGS' route groups, the addon screens' routes, and the main menu's routes into the
//! game's own screens.

use swofty_formats::pkg::Node;

use super::MOD_MENU;
use super::f1_world::{
    ACTIVITY_ENTRY, GRAND_PRIX_ENTRY, GRAND_PRIX_ROUTE, GRAND_PRIX_SAVES_ROUTE, SHOWROOM_ROUTE,
    TIME_TRIAL_ENTRY, TIME_TRIAL_ROUTE, customisation_route, showroom_route,
};
use crate::Result;
use crate::ui::node::*;
use crate::ui::pages::{HOME, HomePage, NEXT_PAGE, PREV_PAGE};

pub(super) fn route_cmd(kind: &str, id: u32, attrs: &[(&str, &str)], deps: Option<&str>) -> Node {
    let mut a = vec![obj(vec![("Id", s("Id")), ("Value", s(&id.to_string()))])];
    a.extend(
        attrs
            .iter()
            .map(|(k, v)| obj(vec![("Id", s(k)), ("Value", s(v))])),
    );
    let mut items = vec![("Attributes", arr(a)), ("Type", s(kind))];
    if let Some(d) = deps {
        items.push(("Dependencies", s(d)));
    }
    obj(items)
}

pub(super) fn route(id: &str, cmds: Vec<Node>, deps: &str, kind: i32, destination: Option<&str>) -> Node {
    let mut items = vec![("Id", s(id)), ("Commands", arr(cmds))];
    if let Some(dst) = destination {
        items.push((
            "Attributes",
            arr(vec![obj(vec![("Id", s("Destination")), ("Value", s(dst))])]),
        ));
    }
    items.push(("Type", i32v(kind)));
    items.push(("Dependencies", s(deps)));
    obj(items)
}

pub(super) fn screen_routes(name: &str) -> Vec<Node> {
    let open = |n| route_cmd("OpenScreen", n, &[("Target", name)], None);
    vec![
        route(
            "open",
            vec![
                open(1),
                route_cmd("InputState", 2, &[], Some("1")),
                route_cmd(
                    "TransitionScreen",
                    3,
                    &[("Direction", "in"), ("Target", name)],
                    Some("1"),
                ),
            ],
            "2,3",
            0,
            None,
        ),
        route(
            "open_no_input",
            vec![
                open(1),
                route_cmd(
                    "TransitionScreen",
                    2,
                    &[("Direction", "in"), ("Target", name)],
                    Some("1"),
                ),
            ],
            "2",
            0,
            None,
        ),
        route(
            "close",
            vec![
                route_cmd(
                    "TransitionScreen",
                    1,
                    &[("Direction", "out"), ("Target", name)],
                    None,
                ),
                route_cmd("CloseScreen", 2, &[("Target", name)], Some("1")),
            ],
            "2",
            0,
            None,
        ),
    ]
}

pub(super) fn subpage_route(id: &str, src: &str, dst: &str, kind: i32) -> Node {
    route(
        id,
        vec![
            route_cmd(
                "TriggerSubRoute",
                1,
                &[("SubRoute", "close"), ("Target", src), ("To", dst)],
                None,
            ),
            route_cmd(
                "TriggerSubRoute",
                2,
                &[
                    ("SubRoute", "open"),
                    ("Target", dst),
                    ("From", src),
                    ("Message", ""),
                ],
                Some("1"),
            ),
        ],
        "2",
        kind,
        Some(dst),
    )
}

pub(super) fn migrate_route(id: &str, src: &str, flow: &str, dst: &str) -> Node {
    migrate_route_with(id, src, flow, dst, ("open_no_input", ""), 2)
}

pub(super) fn migrate_route_with(
    id: &str,
    src: &str,
    flow: &str,
    dst: &str,
    open: (&str, &str),
    kind: i32,
) -> Node {
    let (sub, message) = open;
    route(
        id,
        vec![
            route_cmd(
                "TriggerSubRoute",
                1,
                &[("SubRoute", "close"), ("Target", src), ("To", dst)],
                None,
            ),
            route_cmd(
                "Migrate",
                2,
                &[("Flow", flow), ("SkipActivator", "true")],
                Some("1"),
            ),
            route_cmd(
                "TriggerSubRoute",
                3,
                &[
                    ("SubRoute", sub),
                    ("Target", dst),
                    ("From", src),
                    ("Message", message),
                ],
                Some("2"),
            ),
        ],
        "3",
        kind,
        Some(dst),
    )
}

pub(super) fn route_group(name: &str, routes: Vec<Node>) -> Node {
    obj(vec![
        ("Id", s(name)),
        ("Routes", arr(routes)),
        (
            "Attributes",
            arr(vec![obj(vec![("Id", s("Source")), ("Value", s(name))])]),
        ),
    ])
}

/// ADDON SETTINGS' route groups: each home screen's, with a route to each of its `pages` (route id,
/// screen, home screen) and to the home screens before and after it, and each page's, whose `back`
/// opens its home screen again. `back` on the first home screen leaves for the game's home menu, and on
/// a later one opens the home screen before it.
pub fn route_groups(homes: &[HomePage], pages: &[(String, String, String)]) -> Vec<Node> {
    let mut groups = Vec::new();
    for h in homes {
        let mut r = screen_routes(&h.screen);
        r.push(match &h.prev {
            Some(prev) => subpage_route("back", &h.screen, prev, 2),
            None => migrate_route("back", &h.screen, "frontend_flow", "fe_home"),
        });
        if let Some(next) = &h.next {
            r.push(subpage_route(NEXT_PAGE, &h.screen, next, 1));
        }
        if let Some(prev) = &h.prev {
            r.push(subpage_route(PREV_PAGE, &h.screen, prev, 2));
        }
        r.extend(
            pages
                .iter()
                .filter(|(_, _, home)| home == &h.screen)
                .map(|(rid, s, _)| subpage_route(rid, &h.screen, s, 1)),
        );
        groups.push(route_group(&h.screen, r));
    }
    for (_, s, home) in pages {
        let mut r = screen_routes(s);
        r.push(subpage_route("back", s, home, 2));
        groups.push(route_group(s, r));
    }
    groups
}

/// Route groups for addon screens, as ADDON SETTINGS pages have: open, open_no_input and close,
/// and back, which closes the screen and opens its anchor again.
pub fn screen_route_groups(screens: &[(String, String)]) -> Vec<Node> {
    screens
        .iter()
        .map(|(s, anchor)| {
            let mut r = screen_routes(s);
            r.push(subpage_route("back", s, anchor, 2));
            route_group(s, r)
        })
        .collect()
}

/// Adds open_<name> to each anchor's route group in these routes, when they hold it: the route
/// closes the anchor and opens the addon screen, as ADDON SETTINGS opens its pages. Each entry is
/// (name, anchor, screen).
pub fn anchor_routes(data: &[u8], screens: &[(String, String, String)]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        for (name, anchor, s) in screens {
            let Some(g) = gs.iter_mut().find(|g| id_of(g) == Some(anchor)) else {
                continue;
            };
            let id = format!("open_{name}");
            let routes = g.o_mut()?.at_mut("Routes")?.a_mut()?;
            if !routes.iter().any(|r| id_of(r) == Some(&id)) {
                routes.push(subpage_route(&id, anchor, s, 1));
            }
        }
        Ok(())
    })
}

pub(super) fn id_of(n: &Node) -> Option<&str> {
    n.as_obj()?.get("Id")?.as_val()?.as_str()
}

pub(super) fn mod_menu_route(groupings: &[Node]) -> Result<Node> {
    let home = groupings
        .iter()
        .find(|g| id_of(g) == Some("fe_home"))
        .ok_or_else(|| err("the routes have no fe_home grouping"))?;
    let mut r = home
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some("options"))
        .cloned()
        .ok_or_else(|| err("fe_home has no options route"))?;
    set_str(r.o_mut()?, "Id", MOD_MENU)?;
    replace_strings(&mut r, &|v| match v {
        "settings_container" => Some(HOME.into()),
        "open_no_input" => Some("open".into()),
        _ => None,
    });
    Ok(r)
}

pub(super) fn group_route(groupings: &[Node], group: &str, id: &str) -> Result<Node> {
    groupings
        .iter()
        .find(|g| id_of(g) == Some(group))
        .ok_or_else(|| err(format!("the routes have no {group} grouping")))?
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some(id))
        .cloned()
        .ok_or_else(|| err(format!("{group} has no {id} route")))
}

pub fn has_route(data: &[u8], group: &str, id: &str) -> bool {
    let Ok(p) = swofty_formats::pkg::Pkg::parse(data) else {
        return false;
    };
    p.root
        .o()
        .and_then(|o| o.at("RouteGroupings"))
        .and_then(|g| g.a())
        .is_ok_and(|gs| {
            gs.iter().filter(|g| id_of(g) == Some(group)).any(|g| {
                g.o()
                    .and_then(|o| o.at("Routes"))
                    .and_then(|r| r.a())
                    .is_ok_and(|rs| rs.iter().any(|r| id_of(r) == Some(id)))
            })
        })
}

pub(super) fn home_route(groupings: &[Node], id: &str) -> Result<Node> {
    group_route(groupings, "fe_home", id)
}

/// The main menu's Time Trial and Grand Prix routes: the game's own `home_to_activity` (wipe, close the home
/// screen and its slices, `Migrate f1_world_flow enter_f1_world_activity`) with the entry swapped for a launcher
/// entry that carries the picker's command, and a copy of `theatre` that opens the Grand Prix saves list in the
/// front-end flow itself.
pub(super) fn nav_routes(groupings: &[Node]) -> Result<Vec<Node>> {
    let activity = home_route(groupings, "home_to_activity")?;
    let mut out = Vec::new();
    for (id, entry) in [
        (TIME_TRIAL_ROUTE, TIME_TRIAL_ENTRY),
        (GRAND_PRIX_ROUTE, GRAND_PRIX_ENTRY),
    ] {
        let mut r = activity.clone();
        set_str(r.o_mut()?, "Id", id)?;
        let swapped = std::cell::Cell::new(0);
        replace_strings(&mut r, &|v| {
            (v == ACTIVITY_ENTRY).then(|| {
                swapped.set(swapped.get() + 1);
                entry.to_owned()
            })
        });
        if swapped.get() == 0 {
            return Err(err("home_to_activity does not name enter_f1_world_activity"));
        }
        out.push(r);
    }
    let mut saves = home_route(groupings, "theatre")?;
    set_str(saves.o_mut()?, "Id", GRAND_PRIX_SAVES_ROUTE)?;
    replace_strings(&mut saves, &|v| {
        (v == "theatre_container").then(|| "grand_prix_continue".to_owned())
    });
    out.push(saves);
    Ok(out)
}

/// Adds ADDON SETTINGS' route groups, which must be new, and each of `optional` whose group the
/// routes don't have yet. `f1_world` is the stock routes_f1_world.uir, the source of the main menu's Showroom route.
pub fn routes(data: &[u8], groups: Vec<Node>, optional: Vec<Node>, f1_world: &[u8]) -> Result<Vec<u8>> {
    let showroom = swofty_formats::pkg::Pkg::parse(f1_world)
        .ok()
        .and_then(|p| showroom_route(p.root.o().ok()?.at("RouteGroupings").ok()?.a().ok()?).ok());
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        let replace = mod_menu_route(gs)?;
        let customisation = customisation_route(gs).ok();
        let nav = nav_routes(gs).ok();
        for g in optional {
            if !gs.iter().any(|x| id_of(x) == id_of(&g)) {
                gs.push(g);
            }
        }
        for g in groups {
            if gs.iter().any(|x| id_of(x) == id_of(&g)) {
                return Err(err(format!(
                    "route group {} exists already",
                    id_of(&g).unwrap_or_default()
                )));
            }
            gs.push(g);
        }
        let rid = id_of(&replace).unwrap_or_default().to_owned();
        let grp = gs
            .iter_mut()
            .find(|x| id_of(x) == Some("fe_home"))
            .ok_or_else(|| err("the routes have no fe_home grouping"))?;
        let routes = grp.o_mut()?.at_mut("Routes")?.a_mut()?;
        routes.retain(|r| id_of(r) != Some(&rid));
        routes.push(replace);
        if let Some(showroom) = showroom {
            routes.retain(|r| id_of(r) != Some(SHOWROOM_ROUTE));
            routes.push(showroom);
        }
        for r in nav.into_iter().flatten().chain(customisation) {
            let id = id_of(&r).unwrap_or_default().to_owned();
            routes.retain(|x| id_of(x) != Some(&id));
            routes.push(r);
        }
        Ok(())
    })
}
