//! The routes and flow entries that take the main menu to F1 World's pickers and Customisation, and back home
//! without the F1 World hub.

use swofty_formats::pkg::Node;

use super::routes::{group_route, home_route, id_of};
use super::setup::cmd_type;
use crate::Result;
use crate::ui::node::*;

pub const SHOWROOM_ROUTE: &str = "swofty_showroom";

pub const CUSTOMISATION_ROUTE: &str = "swofty_customisation";

pub const CUSTOMISATION_GROUP: &str = "customisation_f1_life_entry";

pub(super) const CUSTOMISATION_FLOW: &str = "customisation_flow";

pub(super) const CUSTOMISATION_ENTRY: &str = "enter_f1_life_customisation";

pub const HOME_BACK_ROUTE: &str = "swofty_home";

pub const BACK_HOME_FROM_CONTAINER: &str = "swofty_back_home";

pub const TIME_TRIAL_ROUTE: &str = "swofty_time_trial";

pub const GRAND_PRIX_ROUTE: &str = "swofty_grand_prix";

pub const GRAND_PRIX_SAVES_ROUTE: &str = "swofty_grand_prix_saves";

pub const TIME_TRIAL_ENTRY: &str = "swofty_enter_time_trial";

pub const GRAND_PRIX_ENTRY: &str = "swofty_enter_grand_prix";

pub const RETURN_ENTRY: &str = "swofty_enter_return";

pub(super) const ACTIVITY_ENTRY: &str = "enter_f1_world_activity";

pub(super) const SPLITSCREEN_ENTRY: &str = "enter_splitscreen_era_select";

pub(super) const F1_WORLD_FLOW: &str = "f1_world_flow";

pub(super) const THEMING_OVERLAY: &str = "theming_overlay_frontend";

pub(super) const TINY_WIPE: &str = "ui_wipes/ui_wipe_tiny_right.bk2";

pub(super) const TINY_WIPE_CURVE: &str = "tran_wipe_tiny_right_dly_c";

pub(super) const STANDARD_WIPE: &str = "ui_wipes/ui_wipe_standard_out.bk2";

pub(super) const STANDARD_WIPE_CURVE: &str = "tran_wipe_stndrd_out_dly_c";

pub(super) const TRANSITION_ENTRY_NODE: &str = "migrate_to_f1_world";

pub(super) const F1_WORLD_HUB: &str = "fe_f1_world";

pub(super) const SHOWROOM_SCREEN: &str = "fe_showroom";

/// routes_f1_world.uir with the routes that leave the pickers for the main menu without the F1 World hub:
/// `swofty_back_home` in the activity container's group, a copy of the container's own `home` route (a wipe,
/// `Migrate frontend_flow enter_fe_home`, then fe_home with its slices and scene) that runs the standard
/// small wipe of its own and opens the theming overlay only after the migration, so nothing of the old screen or the hub
/// shows before the wipe covers it; `swofty_home` in the fe_showroom and grand_prix_continue groups, a copy of the
/// hub's `back_1` (a wipe, then close the screen and open fe_home; both screens are in the front-end flow, so no
/// migration is needed). The core swaps them in where the game's own handlers fire their Back route.
pub fn f1_world_routes(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let gs = root_obj(p)?.at_mut("RouteGroupings")?.a_mut()?;
        let back = group_route(gs, F1_WORLD_HUB, "back_1")?;
        let home = container_home_route(&group_route(gs, "f1_world_activity_container", "home")?)?;
        let mut added = vec![("f1_world_activity_container", home)];
        for group in ["fe_showroom", "grand_prix_continue"] {
            let mut r = back.clone();
            set_str(r.o_mut()?, "Id", HOME_BACK_ROUTE)?;
            replace_strings(&mut r, &|v| match v {
                "fe_f1_world" => Some(group.to_owned()),
                STANDARD_WIPE => Some(TINY_WIPE.to_owned()),
                STANDARD_WIPE_CURVE => Some(TINY_WIPE_CURVE.to_owned()),
                _ => None,
            });
            added.push((group, r));
        }
        for (group, r) in added {
            let id = id_of(&r).unwrap_or_default().to_owned();
            let target = gs
                .iter_mut()
                .find(|g| id_of(g) == Some(group))
                .ok_or_else(|| err(format!("the routes have no {group} grouping")))?;
            let routes = target.o_mut()?.at_mut("Routes")?.a_mut()?;
            routes.retain(|x| id_of(x) != Some(&id));
            routes.push(r);
        }
        Ok(())
    })
}

pub(super) fn attr_mut<'a>(cmd: &'a mut Node, id: &str) -> Option<&'a mut Node> {
    cmd.as_obj_mut()?
        .get_mut("Attributes")?
        .as_arr_mut()?
        .items
        .iter_mut()
        .find(|a| a.as_obj().and_then(|o| o.str_at("Id").ok()) == Some(id))
}

pub(super) fn attr_value(cmd: &Node, id: &str) -> Option<String> {
    cmd.as_obj()?
        .get("Attributes")?
        .as_arr()?
        .items
        .iter()
        .filter_map(Node::as_obj)
        .find(|a| a.str_at("Id").ok() == Some(id))?
        .str_at("Value")
        .ok()
        .map(str::to_owned)
}

pub(super) fn cmd_id(cmd: &Node) -> Option<String> {
    attr_value(cmd, "Id")
}

pub(super) fn container_home_route(stock: &Node) -> Result<Node> {
    let mut r = stock.clone();
    set_str(r.o_mut()?, "Id", BACK_HOME_FROM_CONTAINER)?;
    let cmds = r.o_mut()?.at_mut("Commands")?.a_mut()?;
    let migrate = cmds
        .iter()
        .find(|c| cmd_type(c) == Some("Migrate") && attr_value(c, "Flow").as_deref() == Some("frontend_flow"))
        .and_then(cmd_id)
        .ok_or_else(|| err("the container's home route has no Migrate to frontend_flow"))?;
    let mut wiped = 0;
    for c in cmds.iter_mut() {
        if attr_mut(c, "Video").is_some() {
            wiped += 1;
        }
        if let Some(v) = attr_mut(c, "DilationCurve") {
            let curve = v.o()?.str_at("Value")?.to_owned();
            if curve.starts_with("tran_wipe_") && curve.ends_with("_dly_c") {
                wiped += 1;
            }
        }
        if cmd_type(c) == Some("TriggerSubRoute")
            && attr_value(c, "Target").as_deref() == Some(THEMING_OVERLAY)
            && c.as_obj().and_then(|o| o.get("Dependencies")).is_none()
        {
            c.o_mut()?.set("Dependencies", s(&migrate));
        }
    }
    if wiped != 2 {
        return Err(err(format!(
            "the container's home route has {wiped} wipe settings, not the stock 2"
        )));
    }
    Ok(r)
}

/// The main menu's Showroom route: a copy of the F1 World hub's own `showroom` (routes_f1_world.uir) with the hub
/// swapped for the home screen. It closes only the home screen, so `slice_bg_content_underlay` and the player card
/// stay up for the series picks that close them, runs fe_showroom's view model callback before input, and closes the
/// theming overlay itself.
pub(super) fn showroom_route(f1_world: &[Node]) -> Result<Node> {
    let mut r = group_route(f1_world, F1_WORLD_HUB, "showroom")?;
    set_str(r.o_mut()?, "Id", SHOWROOM_ROUTE)?;
    replace_strings(&mut r, &|v| (v == F1_WORLD_HUB).then(|| "fe_home".to_owned()));
    let cmds = r.o()?.at("Commands")?.a()?;
    let has = |kind: &str, target: &str| {
        cmds.iter()
            .any(|c| cmd_type(c) == Some(kind) && attr_value(c, "Target").as_deref() == Some(target))
    };
    if !has("CloseScreen", "fe_home")
        || !has("OpenScreen", SHOWROOM_SCREEN)
        || !has("ViewModelCallback", SHOWROOM_SCREEN)
    {
        return Err(err(format!(
            "{F1_WORLD_HUB}'s showroom route does not close the hub and open {SHOWROOM_SCREEN} with its view model callback"
        )));
    }
    Ok(r)
}

/// The main menu's Customisation route: a copy of fe_home's own `customisation` (a wipe, close the home screen and
/// its slices, then `Migrate customisation_flow enter_f1_life_customisation`). F1 World's own `customisation`
/// closes the hub instead.
pub(super) fn customisation_route(groupings: &[Node]) -> Result<Node> {
    let mut r = home_route(groupings, "customisation")?;
    let cmds = r.o()?.at("Commands")?.a()?;
    let enters = cmds.iter().any(|c| {
        cmd_type(c) == Some("Migrate")
            && attr_value(c, "Flow").as_deref() == Some(CUSTOMISATION_FLOW)
            && attr_value(c, "Activator").as_deref() == Some(CUSTOMISATION_ENTRY)
    });
    let closes = cmds
        .iter()
        .any(|c| cmd_type(c) == Some("CloseScreen") && attr_value(c, "Target").as_deref() == Some("fe_home"));
    if !enters || !closes {
        return Err(err(format!(
            "fe_home's customisation route does not close fe_home and enter {CUSTOMISATION_FLOW} at {CUSTOMISATION_ENTRY}"
        )));
    }
    set_str(r.o_mut()?, "Id", CUSTOMISATION_ROUTE)?;
    Ok(r)
}

/// routes_customisation_and_store.uir with `swofty_back_home` in the Customisation screen's group, the activity
/// container's `home` route from `f1_world` (routes_f1_world.uir) as `f1_world_routes` adds it: a wipe, `Migrate
/// frontend_flow enter_fe_home`, then fe_home with its slices and scene. The game's `back_1` there migrates to
/// the F1 World hub; the core swaps it for this route after a Customisation card press.
pub fn customisation_routes(data: &[u8], f1_world: &[u8]) -> Result<Vec<u8>> {
    let stock = swofty_formats::pkg::Pkg::parse(f1_world)?;
    let home = stock
        .root
        .o()?
        .at("RouteGroupings")?
        .a()?
        .iter()
        .find(|g| id_of(g) == Some("f1_world_activity_container"))
        .ok_or_else(|| err("the routes have no f1_world_activity_container grouping"))?
        .o()?
        .at("Routes")?
        .a()?
        .iter()
        .find(|r| id_of(r) == Some("home"))
        .cloned()
        .ok_or_else(|| err("f1_world_activity_container has no home route"))?;
    let home = container_home_route(&home)?;
    edit(data, |p| {
        let routes = root_obj(p)?
            .at_mut("RouteGroupings")?
            .a_mut()?
            .iter_mut()
            .find(|g| id_of(g) == Some(CUSTOMISATION_GROUP))
            .ok_or_else(|| err(format!("the routes have no {CUSTOMISATION_GROUP} grouping")))?
            .o_mut()?
            .at_mut("Routes")?
            .a_mut()?;
        if !routes.iter().any(|r| id_of(r) == Some("back_1")) {
            return Err(err(format!("{CUSTOMISATION_GROUP} has no back_1 route")));
        }
        routes.retain(|r| id_of(r) != Some(BACK_HOME_FROM_CONTAINER));
        routes.push(home);
        Ok(())
    })
}

/// The f1_world flow's launcher entries: the activity container opened with the Time Trial picker's command
/// (`time_trial`) or the new Grand Prix picker's (`grand_prix_new`) already raised, the way the game's own
/// `enter_splitscreen_era_select` opens it with `ss_game_mode_select`, and the bare activity container
/// (`swofty_enter_return`) the session-end hand-over lands on.
pub fn f1_world_entries(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let flows = root_obj(p)?.at_mut("Flows")?.a_mut()?;
        let f = flows
            .iter_mut()
            .find(|f| f.name() == F1_WORLD_FLOW)
            .ok_or_else(|| err(format!("flows.ui has no {F1_WORLD_FLOW}")))?;
        let ents = f
            .slot_mut(CHILDREN)?
            .first_mut()
            .ok_or_else(|| err(format!("{F1_WORLD_FLOW} has no scenes")))?
            .slot_mut(CHILDREN)?;
        let stock = |ents: &[Node], name: &str| {
            ents.iter()
                .find(|e| e.name() == name)
                .cloned()
                .ok_or_else(|| err(format!("{F1_WORLD_FLOW} has no {name}")))
        };
        let plain = stock(ents, ACTIVITY_ENTRY)?;
        let with_command = stock(ents, SPLITSCREEN_ENTRY)?;
        for (name, command) in [
            (TIME_TRIAL_ENTRY, Some("time_trial")),
            (GRAND_PRIX_ENTRY, Some("grand_prix_new")),
            (RETURN_ENTRY, None),
        ] {
            if ents.iter().any(|e| e.name() == name) {
                return Err(err(format!("{F1_WORLD_FLOW} has {name} already")));
            }
            let mut e = if command.is_some() {
                with_command.clone()
            } else {
                plain.clone()
            };
            let p0 = e.props_mut()?;
            set_str(p0, "name", name)?;
            if let Some(c) = command {
                set_str(p0.at_mut("command")?.o_mut()?, "value", c)?;
            }
            ents.push(e);
        }
        fix_ids(f, None);
        Ok(())
    })
}

pub(super) fn named_mut<'a>(n: &'a mut Node, name: &str) -> Option<&'a mut Node> {
    if n.is_node() && n.name() == name {
        return Some(n);
    }
    n.as_arr_mut()?
        .items
        .iter_mut()
        .filter_map(Node::as_arr_mut)
        .flat_map(|a| a.items.iter_mut())
        .find_map(|c| named_mut(c, name))
}

/// frontend_transition.ui with the hand-over for a session that ended in F1 World landing on the bare activity
/// container (`swofty_enter_return`) instead of the F1 World hub (`enter_f1_world_container`).
pub fn frontend_transition(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let screen = first_screen(p)?;
        let n = named_mut(screen, TRANSITION_ENTRY_NODE)
            .ok_or_else(|| err(format!("frontend_transition has no {TRANSITION_ENTRY_NODE} node")))?;
        let entry = n.props_mut()?.at_mut("migrate_entry")?.o_mut()?;
        if entry.str_at("value")? != "enter_f1_world_container" {
            return Err(err(format!(
                "{TRANSITION_ENTRY_NODE} does not enter enter_f1_world_container"
            )));
        }
        set_str(entry, "value", RETURN_ENTRY)
    })
}
