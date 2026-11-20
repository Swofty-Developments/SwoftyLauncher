use swofty_formats::pkg::{Node, Pkg};

use super::f1_world::*;
use super::home::*;
use super::messages::*;
use super::persistent::*;
use super::routes::*;
use super::setup::*;
use super::*;
use crate::ui::check::stock_kept;
use crate::ui::node::*;
use crate::ui::pages::{HOME, NEXT_PAGE, PREV_PAGE};

fn label(name: &str) -> Node {
    node(
        name,
        "UILB",
        vec![(
            "text",
            obj(vec![
                ("value", s("")),
                ("dbentry", s(name)),
                ("isloctag", boolv(true)),
            ]),
        )],
        vec![],
        &[],
        &[],
    )
}

fn with_kids(mut n: Node, kids: Vec<Node>) -> Node {
    n.slot_mut(CHILDREN).unwrap().extend(kids);
    n
}

fn stock_popup() -> Vec<u8> {
    let activate = vec![
        sig(
            "persistent.consume_input_wrapper",
            "consumeStart",
            "activate_message",
        ),
        sig(UIDM, "show", "activate_message"),
    ];
    let pressed = vec![
        sig(UIDM, "deactivate_message", "pressed"),
        sig(&format!("{UIDM}.disable_input"), "trigger", "pressed"),
    ];
    let button = with_kids(
        node(
            "button1",
            "UIBT",
            vec![],
            vec![event("pressed", pressed)],
            &[],
            &[],
        ),
        vec![label("title")],
    );
    let back = node(
        "back_listener",
        "UIBT",
        vec![],
        vec![event(
            "pressed",
            vec![
                sig(UIDM, "deactivate_message", "pressed"),
                sig(&format!("{UIDM}.disable_input"), "trigger", "pressed"),
                signal(
                    &format!("{UIDM}.user_response_setter"),
                    "setValue",
                    "pressed",
                    &[("value", "cancel")],
                ),
            ],
        )],
        &[],
        &[],
    );
    let message = with_kids(
        node(
            "persistent_multichoice_message",
            "UIDM",
            vec![],
            vec![event("activate_message", activate)],
            &[],
            &[],
        ),
        vec![
            label("title"),
            label("body"),
            button,
            back,
            node("disable_input", "UIGP", vec![], vec![], &[], &[]),
            node("user_response_setter", "UIDS", vec![], vec![], &[], &[]),
        ],
    );
    let wrapper = with_kids(
        node("consume_input_wrapper", "UIGP", vec![], vec![], &[], &[]),
        vec![message],
    );
    let mut screen = with_kids(
        node("persistent", "UISC", vec![], vec![], &[], &[]),
        vec![wrapper],
    );
    fix_ids(&mut screen, None);
    ui_bytes(vec![screen]).unwrap()
}

fn popup_node(data: &[u8]) -> Node {
    first_screen_of(data)
        .unwrap()
        .find_mut("consume_input_wrapper.persistent_multichoice_message")
        .unwrap()
        .clone()
}

fn event_signals(n: &Node, name: &str) -> Vec<Node> {
    n.slot(EVENTS)
        .unwrap()
        .iter()
        .find(|e| e.o().unwrap().str_at("eventName").unwrap() == name)
        .unwrap()
        .o()
        .unwrap()
        .at("signals")
        .unwrap()
        .a()
        .unwrap()
        .clone()
}

#[test]
fn the_popup_gate_leaves_stock_popups_alone() {
    let stock = stock_popup();
    let gated = persistent_gate(&stock).unwrap();
    let changed = stock_kept(&stock, &gated, "swofty_").unwrap();
    assert!(changed.is_empty(), "{changed:?}");

    let popup = popup_node(&gated);
    let shown = relabel(&event_signals(&popup_node(&stock), "activate_message"), "no").unwrap();
    assert_eq!(
        canonical(&arr(event_signals(
            popup.child("swofty_once_check").unwrap(),
            "no"
        ))),
        canonical(&arr(shown))
    );
}

#[test]
fn a_signal_to_the_popup_labels_fails_the_check() {
    let stock = stock_popup();
    let gated = edit(&persistent_gate(&stock).unwrap(), |p| {
        let m = multichoice(p)?;
        event_mut(m.child_mut("swofty_once_check")?, "no")?.push(sig_with(
            &format!("{UIDM}.title"),
            "setLocalise",
            "no",
            &[("localise", Attr::Bool(true))],
        ));
        Ok(())
    })
    .unwrap();
    let changed = stock_kept(&stock, &gated, "swofty_").unwrap();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(changed[0].contains(".title.setLocalise"), "{changed:?}");
}

fn message(id: &str, tokens: bool) -> Node {
    let mut items = vec![
        ("id", s(id)),
        ("title", s(&format!("lng_{id}_title"))),
        ("messageBody", s(&format!("lng_{id}_body"))),
        ("attributes", arr(vec![])),
        (
            "options",
            arr(vec![obj(vec![
                ("value", s("lng_msg_cancel")),
                ("response", s("cancel")),
                ("defaultSelection", boolv(false)),
            ])]),
        ),
    ];
    if tokens {
        items.push((
            "bodyTokens",
            obj(vec![(
                "ERRORCODE",
                obj(vec![("localise", boolv(false)), ("value", s(""))]),
            )]),
        ));
    }
    obj(items)
}

fn messages_file() -> Vec<u8> {
    let mut all: Vec<Node> = ONLINE_MESSAGES
        .iter()
        .map(|id| message(id, id.ends_with("_with_code")))
        .collect();
    all.push(message("quit_session", false));
    all.push(message(MESSAGE_TEMPLATE, true));
    Pkg {
        name: [0; 4],
        root: obj(vec![("messages", arr(all))]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn by_id(data: &[u8]) -> Vec<(String, Node)> {
    Pkg::parse(data)
        .unwrap()
        .root
        .o()
        .unwrap()
        .at("messages")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|m| (m.o().unwrap().str_at("id").unwrap().to_owned(), m.clone()))
        .collect()
}

#[test]
fn online_messages_show_language_strings_and_others_stay_stock() {
    let stock = by_id(&messages_file());
    let ours = by_id(&messages(&messages_file(), true).unwrap());
    assert_eq!(ours.len(), stock.len() + 2);
    assert_eq!(ours.last().unwrap().0, CONFIRM_ID);
    for ((id, m), (_, before)) in ours.iter().zip(&stock) {
        let o = m.o().unwrap();
        if !ONLINE_MESSAGES.contains(&id.as_str()) {
            assert_eq!(canonical(m), canonical(before), "{id} changed");
            continue;
        }
        assert_eq!(o.str_at("title").unwrap(), POPUP_TITLE);
        assert_eq!(o.str_at("messageBody").unwrap(), POPUP_BODY);
        assert!(o.get("titleTokens").is_none() && o.get("bodyTokens").is_none());
        assert_eq!(o.at("options").unwrap().a().unwrap().len(), 1);
    }
    let plain = by_id(&messages(&messages_file(), false).unwrap());
    assert_eq!(plain.len(), stock.len() + 2);
    for ((id, m), (_, before)) in plain.iter().zip(&stock) {
        let (o, b) = (m.o().unwrap(), before.o().unwrap());
        assert_eq!(o.str_at("title").unwrap(), b.str_at("title").unwrap(), "{id}");
        assert_eq!(
            o.str_at("messageBody").unwrap(),
            b.str_at("messageBody").unwrap(),
            "{id}"
        );
    }
    let strings = Popup::for_version("1.2.3").strings();
    assert_eq!(
        strings[0],
        (POPUP_TITLE.to_owned(), "Swofty Launcher v1.2.3".to_owned())
    );
    assert_eq!(strings[1].0, POPUP_BODY);
    assert!(strings[1].1.contains("New in v1.2.3"));
}

#[test]
fn the_runtime_message_reads_its_text_from_two_attributes() {
    let ours = by_id(&messages(&messages_file(), false).unwrap());
    let (_, m) = ours.iter().find(|(id, _)| id == MESSAGE_ID).unwrap();
    let o = m.o().unwrap();
    assert_eq!(o.str_at("title").unwrap(), "lng_value_tag");
    assert_eq!(o.str_at("messageBody").unwrap(), "lng_value_tag");
    for (tokens, attr) in [("titleTokens", "title"), ("bodyTokens", "message")] {
        let t = o
            .at(tokens)
            .unwrap()
            .o()
            .unwrap()
            .at("VALUE")
            .unwrap()
            .o()
            .unwrap();
        assert_eq!(t.str_at("name").unwrap(), "VALUE");
        assert_eq!(
            t.str_at("dbEntry").unwrap(),
            format!("{ATTRIBUTE_ROW}[{attr}].Value")
        );
        assert_eq!(canonical(t.get("localise").unwrap()), canonical(&boolv(false)));
    }
    let names: Vec<&str> = o
        .at("attributes")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|a| a.o().unwrap().str_at("name").unwrap())
        .collect();
    assert_eq!(names, ["title", "message"]);
    let options = o.at("options").unwrap().a().unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].o().unwrap().str_at("response").unwrap(), "ok");
    let twice = by_id(&messages(&messages(&messages_file(), false).unwrap(), false).unwrap());
    assert_eq!(twice.iter().filter(|(id, _)| id == MESSAGE_ID).count(), 1);
}

#[test]
fn the_confirm_message_offers_load_anyway_and_back() {
    let ours = by_id(&messages(&messages_file(), false).unwrap());
    let (_, m) = ours.iter().find(|(id, _)| id == CONFIRM_ID).unwrap();
    let options = m.o().unwrap().at("options").unwrap().a().unwrap();
    let responses: Vec<&str> = options
        .iter()
        .map(|o| o.o().unwrap().str_at("response").unwrap())
        .collect();
    assert_eq!(responses, ["load_anyway", "back"]);
    let twice = by_id(&messages(&messages(&messages_file(), false).unwrap(), false).unwrap());
    assert_eq!(twice.iter().filter(|(id, _)| id == CONFIRM_ID).count(), 1);
}

fn hud_message(id: &str) -> Node {
    obj(vec![
        ("id", s(id)),
        ("type", s("hud_main")),
        ("playerFilter", s("exclusive")),
        ("priority", u32v(2)),
        (
            "attributes",
            arr(vec![toast_attribute("message", s(&format!("lng_{id}")))]),
        ),
        ("options", arr(vec![])),
        ("queue", s("hud_main_queue")),
    ])
}

#[test]
fn toasts_are_hud_main_messages_with_literal_text_and_a_fixed_duration() {
    let stock = Pkg {
        name: [0; 4],
        root: obj(vec![(
            "messages",
            arr(vec![hud_message("drs_enabled"), hud_message(TOAST_TEMPLATE)]),
        )]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap();
    let ours = by_id(&hud_messages(&stock).unwrap());
    assert_eq!(ours.len(), 2 + TOAST_SECONDS.len());
    for (i, &n) in TOAST_SECONDS.iter().enumerate() {
        let (id, m) = &ours[2 + i];
        assert_eq!(id, &format!("{TOAST_PREFIX}{n}"));
        let o = m.o().unwrap();
        assert_eq!(o.str_at("type").unwrap(), "hud_main");
        assert_eq!(o.str_at("queue").unwrap(), "hud_main_queue");
        let attrs = o.at("attributes").unwrap().a().unwrap();
        let names: Vec<&str> = attrs
            .iter()
            .map(|a| a.o().unwrap().str_at("name").unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "message",
                "sub_message",
                "msg_duration",
                "flag",
                "enabled_in_spectator"
            ]
        );
        for a in attrs {
            let a = a.o().unwrap();
            assert_eq!(canonical(a.get("localise").unwrap()), canonical(&boolv(false)));
        }
        let duration = attrs[2].o().unwrap().get("value").unwrap();
        assert_eq!(canonical(duration), canonical(&f32v(n as f32)));
    }
    assert_eq!(canonical(&ours[0].1), canonical(&hud_message("drs_enabled")));
    let twice = by_id(&hud_messages(&hud_messages(&stock).unwrap()).unwrap());
    assert_eq!(twice.len(), ours.len());
}

fn routes_file() -> Vec<u8> {
    let options = route("options", vec![], "", 0, None);
    Pkg {
        name: [0; 4],
        root: obj(vec![(
            "RouteGroupings",
            arr(vec![
                route_group("fe_home", vec![options]),
                route_group("settings_container", vec![]),
            ]),
        )]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn pkg_of(groups: Vec<Node>) -> Vec<u8> {
    Pkg {
        name: [0; 4],
        root: obj(vec![("RouteGroupings", arr(groups))]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn route_ids_of(p: &Pkg, g: &str) -> Vec<String> {
    group(p, g)
        .unwrap()
        .o()
        .unwrap()
        .at("Routes")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .filter_map(|r| id_of(r).map(str::to_owned))
        .collect()
}

fn route_in<'a>(p: &'a Pkg, g: &str, id: &str) -> &'a Node {
    group(p, g)
        .unwrap()
        .o()
        .unwrap()
        .at("Routes")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .find(|r| id_of(r) == Some(id))
        .unwrap()
}

fn migrate_cmd(id: u32, flow: &str, entry: &str) -> Node {
    route_cmd("Migrate", id, &[("Flow", flow), ("Activator", entry)], None)
}

fn hub_showroom_route(callback: bool) -> Node {
    let mut cmds = vec![
        route_cmd("PlayerCard", 1, &[("PlayerCardMode", "0")], None),
        route_cmd(
            "TransitionScreen",
            2,
            &[("Direction", "out"), ("Target", F1_WORLD_HUB)],
            None,
        ),
        route_cmd("CloseScreen", 3, &[("Target", F1_WORLD_HUB)], Some("2")),
        route_cmd("OpenScreen", 4, &[("Target", SHOWROOM_SCREEN)], Some("3")),
    ];
    if callback {
        cmds.push(route_cmd(
            "ViewModelCallback",
            5,
            &[("Message", ""), ("Target", SHOWROOM_SCREEN)],
            Some("4"),
        ));
    }
    cmds.extend([
        route_cmd("InputState", 6, &[], Some("5")),
        route_cmd(
            "TransitionScreen",
            7,
            &[("Direction", "in"), ("Target", SHOWROOM_SCREEN)],
            Some("5"),
        ),
        route_cmd(
            "TriggerSubRoute",
            8,
            &[("SubRoute", "close"), ("Target", THEMING_OVERLAY)],
            None,
        ),
    ]);
    route("showroom", cmds, "6,7,8", 1, Some(SHOWROOM_SCREEN))
}

fn f1_world_with(showroom: Node) -> Vec<u8> {
    pkg_of(vec![
        route_group(
            F1_WORLD_HUB,
            vec![route("back_1", vec![], "fe_home", 2, None), showroom],
        ),
        route_group(SHOWROOM_SCREEN, vec![route("back_1", vec![], "", 2, None)]),
    ])
}

fn steps(r: &Node) -> Vec<String> {
    r.o()
        .unwrap()
        .at("Commands")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|c| {
            [
                cmd_type(c).map(str::to_owned),
                attr_value(c, "SubRoute"),
                attr_value(c, "Direction"),
                attr_value(c, "Target"),
                attr_value(c, "PlayerCardMode"),
                c.as_obj()
                    .and_then(|o| o.str_at("Dependencies").ok())
                    .map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect()
}

fn home_with_theatre() -> Vec<u8> {
    pkg_of(vec![
        route_group(
            "fe_home",
            vec![
                route("options", vec![], "", 0, None),
                route(
                    "theatre",
                    vec![
                        route_cmd(
                            "TriggerSubRoute",
                            5,
                            &[
                                ("SubRoute", "close"),
                                ("Target", "fe_home"),
                                ("To", "theatre_container"),
                            ],
                            None,
                        ),
                        route_cmd(
                            "TriggerSubRoute",
                            6,
                            &[("SubRoute", "open_no_input"), ("Target", "theatre_container")],
                            Some("5"),
                        ),
                    ],
                    "6",
                    1,
                    Some("theatre_container"),
                ),
            ],
        ),
        route_group("settings_container", vec![]),
    ])
}

#[test]
fn the_main_menu_showroom_route_is_the_hubs_own_showroom_route_from_the_home_screen() {
    let out = routes(
        &home_with_theatre(),
        Vec::new(),
        Vec::new(),
        &f1_world_with(hub_showroom_route(true)),
    )
    .unwrap();
    let p = Pkg::parse(&out).unwrap();
    let ours = route_in(&p, "fe_home", SHOWROOM_ROUTE);
    assert_eq!(
        steps(ours),
        [
            "PlayerCard 0",
            "TransitionScreen out fe_home",
            "CloseScreen fe_home 2",
            "OpenScreen fe_showroom 3",
            "ViewModelCallback fe_showroom 4",
            "InputState 5",
            "TransitionScreen in fe_showroom 5",
            "TriggerSubRoute close theming_overlay_frontend",
        ],
        "fe_home's slices stay open, fe_showroom gets its view model callback before input, and the theming overlay closes on its own"
    );
    let o = ours.o().unwrap();
    assert_eq!(o.str_at("Dependencies").unwrap(), "6,7,8");
    assert_eq!(o.at("Type").unwrap().as_val().unwrap().as_u32(), Some(1));
    assert_eq!(attr_value(ours, "Destination").as_deref(), Some(SHOWROOM_SCREEN));
    let text = format!("{ours:?}");
    for gone in [
        "theatre_container",
        F1_WORLD_HUB,
        "slice_bg_content_underlay",
        "CloseSlices",
        "open_no_input",
    ] {
        assert!(!text.contains(gone), "{gone} in {text}");
    }
    assert_eq!(
        steps(route_in(&p, "fe_home", "theatre")),
        [
            "TriggerSubRoute close fe_home",
            "TriggerSubRoute open_no_input theatre_container 5"
        ],
        "the game's own Theatre route stays"
    );
    let twice = routes(
        &out,
        Vec::new(),
        Vec::new(),
        &f1_world_with(hub_showroom_route(true)),
    )
    .unwrap();
    let ids = route_ids_of(&Pkg::parse(&twice).unwrap(), "fe_home");
    assert_eq!(ids.iter().filter(|i| *i == SHOWROOM_ROUTE).count(), 1);
}

#[test]
fn no_showroom_route_is_built_from_the_theatre_when_the_hubs_showroom_route_is_missing_or_changed() {
    let without_hub = pkg_of(vec![route_group(SHOWROOM_SCREEN, vec![])]);
    for f1_world in [Vec::new(), without_hub, f1_world_with(hub_showroom_route(false))] {
        let out = routes(&home_with_theatre(), Vec::new(), Vec::new(), &f1_world).unwrap();
        assert!(!has_route(&out, "fe_home", SHOWROOM_ROUTE));
        assert!(has_route(&out, "fe_home", "theatre"));
    }
}

#[test]
fn the_home_gets_showroom_time_trial_and_grand_prix_routes_copied_from_its_own() {
    let activity = route(
        "home_to_activity",
        vec![migrate_cmd(1, "f1_world_flow", ACTIVITY_ENTRY)],
        "1",
        0,
        Some("f1_world_activity_container"),
    );
    let data = pkg_of(vec![
        route_group(
            "fe_home",
            vec![
                route("options", vec![], "", 0, None),
                route("theatre", vec![], "theatre_container", 0, None),
                activity,
            ],
        ),
        route_group("settings_container", vec![]),
    ]);
    let out = routes(
        &data,
        Vec::new(),
        Vec::new(),
        &f1_world_with(hub_showroom_route(true)),
    )
    .unwrap();
    let p = Pkg::parse(&out).unwrap();
    let ids = route_ids_of(&p, "fe_home");
    for id in [
        SHOWROOM_ROUTE,
        TIME_TRIAL_ROUTE,
        GRAND_PRIX_ROUTE,
        GRAND_PRIX_SAVES_ROUTE,
        "theatre",
        "home_to_activity",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id} in {ids:?}");
    }
    let entry_of = |id: &str| {
        let r = route_in(&p, "fe_home", id);
        let cmds = r.o().unwrap().at("Commands").unwrap().a().unwrap();
        attr_value(&cmds[0], "Activator").unwrap()
    };
    assert_eq!(entry_of("home_to_activity"), ACTIVITY_ENTRY);
    assert_eq!(entry_of(TIME_TRIAL_ROUTE), TIME_TRIAL_ENTRY);
    assert_eq!(entry_of(GRAND_PRIX_ROUTE), GRAND_PRIX_ENTRY);
    let saves = route_in(&p, "fe_home", GRAND_PRIX_SAVES_ROUTE);
    assert_eq!(
        saves.o().unwrap().str_at("Dependencies").unwrap(),
        "grand_prix_continue"
    );
}

fn home_route_with(video: &str, curve: &str) -> Node {
    route(
        "home",
        vec![
            route_cmd(
                "Wait",
                1,
                &[("Time", "0"), ("DilationCurve", "tran_pre_wipe_dly_gen")],
                None,
            ),
            route_cmd("StartStandardWipe", 2, &[("Video", video)], Some("1")),
            route_cmd("Wait", 3, &[("Time", "0"), ("DilationCurve", curve)], None),
            route_cmd(
                "TriggerSubRoute",
                4,
                &[("SubRoute", "open"), ("Target", THEMING_OVERLAY)],
                None,
            ),
            migrate_cmd(5, "frontend_flow", "enter_fe_home"),
        ],
        "2,4,5",
        0,
        Some("fe_home"),
    )
}

#[test]
fn the_container_the_showroom_and_the_saves_get_a_route_that_leaves_for_the_main_menu() {
    let data = pkg_of(vec![
        route_group(
            "f1_world_activity_container",
            vec![
                route("f1_world", vec![], "fe_f1_world", 0, None),
                home_route_with("ui_wipes/ui_wipe_tiny_right.bk2", "tran_wipe_tiny_right_dly_c"),
            ],
        ),
        route_group("fe_showroom", vec![route("back_1", vec![], "", 0, None)]),
        route_group("grand_prix_continue", vec![route("back_1", vec![], "", 0, None)]),
        route_group("fe_f1_world", vec![route("back_1", vec![], "fe_home", 0, None)]),
    ]);
    let out = f1_world_routes(&data).unwrap();
    let p = Pkg::parse(&out).unwrap();
    assert_eq!(route_ids_of(&p, "fe_showroom"), ["back_1", HOME_BACK_ROUTE]);
    assert_eq!(
        route_ids_of(&p, "grand_prix_continue"),
        ["back_1", HOME_BACK_ROUTE]
    );
    assert_eq!(
        route_ids_of(&p, "f1_world_activity_container"),
        ["f1_world", "home", BACK_HOME_FROM_CONTAINER]
    );
    assert_eq!(route_ids_of(&p, "fe_f1_world"), ["back_1"]);
    let home = route_in(&p, "f1_world_activity_container", BACK_HOME_FROM_CONTAINER);
    let cmds = home.o().unwrap().at("Commands").unwrap().a().unwrap();
    assert_eq!(
        attr_value(&cmds[1], "Video").as_deref(),
        Some(TINY_WIPE),
        "the stock small wipe stays"
    );
    assert_eq!(
        attr_value(&cmds[2], "DilationCurve").as_deref(),
        Some(TINY_WIPE_CURVE)
    );
    assert_eq!(
        attr_value(&cmds[0], "DilationCurve").as_deref(),
        Some("tran_pre_wipe_dly_gen")
    );
    assert_eq!(
        cmds[3].o().unwrap().str_at("Dependencies").unwrap(),
        "5",
        "the overlay opens after the migration"
    );
    let stock = route_in(&p, "f1_world_activity_container", "home");
    let stock_cmds = stock.o().unwrap().at("Commands").unwrap().a().unwrap();
    assert_eq!(
        attr_value(&stock_cmds[1], "Video").as_deref(),
        Some("ui_wipes/ui_wipe_tiny_right.bk2")
    );
}

fn home_customisation_route(flow: &str) -> Node {
    route(
        "customisation",
        vec![
            route_cmd(
                "TransitionScreen",
                1,
                &[("Direction", "out"), ("Target", "fe_home")],
                None,
            ),
            route_cmd("CloseScreen", 2, &[("Target", "fe_home")], Some("1")),
            migrate_cmd(3, flow, CUSTOMISATION_ENTRY),
        ],
        "2,3",
        1,
        Some(CUSTOMISATION_GROUP),
    )
}

#[test]
fn the_home_gets_a_customisation_route_copied_from_its_own() {
    let data = pkg_of(vec![
        route_group(
            "fe_home",
            vec![
                route("options", vec![], "", 0, None),
                home_customisation_route(CUSTOMISATION_FLOW),
            ],
        ),
        route_group("settings_container", vec![]),
    ]);
    let out = routes(&data, Vec::new(), Vec::new(), &[]).unwrap();
    assert!(has_route(&out, "fe_home", CUSTOMISATION_ROUTE));
    assert!(has_route(&out, "fe_home", "customisation"));
    let p = Pkg::parse(&out).unwrap();
    let ours = route_in(&p, "fe_home", CUSTOMISATION_ROUTE);
    let cmds = ours.o().unwrap().at("Commands").unwrap().a().unwrap();
    assert_eq!(attr_value(&cmds[1], "Target").as_deref(), Some("fe_home"));
    assert_eq!(attr_value(&cmds[2], "Flow").as_deref(), Some(CUSTOMISATION_FLOW));
    assert_eq!(
        attr_value(&cmds[2], "Activator").as_deref(),
        Some(CUSTOMISATION_ENTRY)
    );
    let twice = routes(&out, Vec::new(), Vec::new(), &[]).unwrap();
    let ids = route_ids_of(&Pkg::parse(&twice).unwrap(), "fe_home");
    assert_eq!(ids.iter().filter(|i| *i == CUSTOMISATION_ROUTE).count(), 1);
}

#[test]
fn a_home_customisation_route_into_another_flow_is_not_copied() {
    for groups in [
        vec![route_group(
            "fe_home",
            vec![route("options", vec![], "", 0, None)],
        )],
        vec![route_group(
            "fe_home",
            vec![
                route("options", vec![], "", 0, None),
                home_customisation_route("frontend_flow"),
            ],
        )],
    ] {
        let mut groups = groups;
        groups.push(route_group("settings_container", vec![]));
        let out = routes(&pkg_of(groups), Vec::new(), Vec::new(), &[]).unwrap();
        assert!(!has_route(&out, "fe_home", CUSTOMISATION_ROUTE));
    }
}

#[test]
fn the_customisation_screen_gets_the_containers_route_to_the_main_menu() {
    let f1_world = pkg_of(vec![route_group(
        "f1_world_activity_container",
        vec![home_route_with(
            "ui_wipes/ui_wipe_tiny_right.bk2",
            "tran_wipe_tiny_right_dly_c",
        )],
    )]);
    let back = migrate_cmd(1, "frontend_flow", "enter_fe_f1_world");
    let data = pkg_of(vec![
        route_group(
            CUSTOMISATION_GROUP,
            vec![
                route("back_1", vec![back], "1", 2, Some("fe_f1_world")),
                route("back_2", vec![], "", 2, Some("fe_news")),
            ],
        ),
        route_group(
            "customisation_driver_suit",
            vec![route("back_1", vec![], "", 2, None)],
        ),
    ]);
    let out = customisation_routes(&data, &f1_world).unwrap();
    let p = Pkg::parse(&out).unwrap();
    assert_eq!(
        route_ids_of(&p, CUSTOMISATION_GROUP),
        ["back_1", "back_2", BACK_HOME_FROM_CONTAINER]
    );
    assert_eq!(route_ids_of(&p, "customisation_driver_suit"), ["back_1"]);
    assert!(has_route(&out, CUSTOMISATION_GROUP, BACK_HOME_FROM_CONTAINER));
    let ours = route_in(&p, CUSTOMISATION_GROUP, BACK_HOME_FROM_CONTAINER);
    let cmds = ours.o().unwrap().at("Commands").unwrap().a().unwrap();
    assert_eq!(attr_value(&cmds[4], "Flow").as_deref(), Some("frontend_flow"));
    assert_eq!(
        attr_value(&cmds[4], "Activator").as_deref(),
        Some("enter_fe_home")
    );
    assert_eq!(
        cmds[3].o().unwrap().str_at("Dependencies").unwrap(),
        "5",
        "the overlay opens after the migration"
    );
    let stock_back = route_in(&p, CUSTOMISATION_GROUP, "back_1");
    let stock_cmds = stock_back.o().unwrap().at("Commands").unwrap().a().unwrap();
    assert_eq!(
        attr_value(&stock_cmds[0], "Activator").as_deref(),
        Some("enter_fe_f1_world"),
        "the game's own Back stays"
    );
    let twice = customisation_routes(&out, &f1_world).unwrap();
    let ids = route_ids_of(&Pkg::parse(&twice).unwrap(), CUSTOMISATION_GROUP);
    assert_eq!(ids.iter().filter(|i| *i == BACK_HOME_FROM_CONTAINER).count(), 1);
}

#[test]
fn the_customisation_back_route_needs_the_stock_screen_and_container() {
    let f1_world = pkg_of(vec![route_group(
        "f1_world_activity_container",
        vec![home_route_with(
            "ui_wipes/ui_wipe_tiny_right.bk2",
            "tran_wipe_tiny_right_dly_c",
        )],
    )]);
    let no_back = pkg_of(vec![route_group(
        CUSTOMISATION_GROUP,
        vec![route("open", vec![], "", 0, None)],
    )]);
    assert!(customisation_routes(&no_back, &f1_world).is_err());
    let no_group = pkg_of(vec![route_group(
        "customisation_car",
        vec![route("back_1", vec![], "", 2, None)],
    )]);
    assert!(customisation_routes(&no_group, &f1_world).is_err());
    let no_home = pkg_of(vec![route_group("f1_world_activity_container", vec![])]);
    let data = pkg_of(vec![route_group(
        CUSTOMISATION_GROUP,
        vec![route("back_1", vec![], "", 2, None)],
    )]);
    assert!(customisation_routes(&data, &no_home).is_err());
    assert!(!has_route(&data, CUSTOMISATION_GROUP, BACK_HOME_FROM_CONTAINER));
    assert!(!has_route(b"not a pkg", CUSTOMISATION_GROUP, "back_1"));
}

fn group<'a>(p: &'a Pkg, id: &str) -> Option<&'a Node> {
    p.root
        .o()
        .unwrap()
        .at("RouteGroupings")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .find(|g| id_of(g) == Some(id))
}

fn route_ids(g: &Node) -> Vec<String> {
    g.o()
        .unwrap()
        .at("Routes")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|r| id_of(r).unwrap().to_owned())
        .collect()
}

#[test]
fn addon_screens_get_route_groups_and_an_open_route_on_their_anchor() {
    let screens = [("ai".to_owned(), "settings_container".to_owned())];
    let d = routes(
        &routes_file(),
        route_groups(&[], &[]),
        screen_route_groups(&screens),
        &[],
    )
    .unwrap();
    // a group already there is kept, not added twice
    let d = routes(&d, Vec::new(), screen_route_groups(&screens), &[]).unwrap();
    let opened = [
        (
            "ai".to_owned(),
            "settings_container".to_owned(),
            "swofty_addon_ai_3".to_owned(),
        ),
        ("x".to_owned(), "no_such_anchor".to_owned(), "x".to_owned()),
    ];
    let d = anchor_routes(&anchor_routes(&d, &opened).unwrap(), &opened).unwrap();
    let p = Pkg::parse(&d).unwrap();
    let ai: Vec<&Node> = p
        .root
        .o()
        .unwrap()
        .at("RouteGroupings")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .filter(|g| id_of(g) == Some("ai"))
        .collect();
    assert_eq!(ai.len(), 1);
    assert_eq!(route_ids(ai[0]), ["open", "open_no_input", "close", "back"]);
    let back = canonical(&ai[0].o().unwrap().at("Routes").unwrap().a().unwrap()[3]);
    assert_eq!(
        back,
        canonical(&subpage_route("back", "ai", "settings_container", 2))
    );
    assert_eq!(route_ids(group(&p, "settings_container").unwrap()), ["open_ai"]);
    assert!(group(&p, "no_such_anchor").is_none());
}

#[test]
fn a_long_addon_settings_list_gets_a_route_group_per_home_screen() {
    let homes = crate::ui::pages::home_plan(70);
    let pages: Vec<(String, String, String)> = (0..70)
        .map(|i| {
            (
                format!("open_addon_{i}"),
                format!("swofty_addon_{i}"),
                crate::ui::pages::home_of(i),
            )
        })
        .collect();
    let groups = route_groups(&homes, &pages);
    assert_eq!(groups.len(), 3 + 70);
    let ids: Vec<&str> = groups.iter().take(3).map(|g| id_of(g).unwrap()).collect();
    assert_eq!(ids, ["swofty_settings", "swofty_settings_2", "swofty_settings_3"]);
    let routes = |g: &Node| route_ids(g);
    let first = routes(&groups[0]);
    assert_eq!(&first[..5], ["open", "open_no_input", "close", "back", NEXT_PAGE]);
    assert_eq!(first.len(), 5 + 24);
    let second = routes(&groups[1]);
    assert_eq!(
        &second[..6],
        ["open", "open_no_input", "close", "back", NEXT_PAGE, PREV_PAGE]
    );
    assert_eq!(second.len(), 6 + 24);
    let third = routes(&groups[2]);
    assert_eq!(&third[..5], ["open", "open_no_input", "close", "back", PREV_PAGE]);
    assert_eq!(third.len(), 5 + 22);
    for (g, opens) in [(&groups[0], 0..24), (&groups[1], 24..48), (&groups[2], 48..70)] {
        let ids = routes(g);
        for i in 0..70 {
            assert_eq!(
                ids.contains(&format!("open_addon_{i}")),
                opens.contains(&i),
                "{i}"
            );
        }
    }
    let at = |g: &Node, id: &str| {
        canonical(
            g.o()
                .unwrap()
                .at("Routes")
                .unwrap()
                .a()
                .unwrap()
                .iter()
                .find(|r| id_of(r) == Some(id))
                .unwrap(),
        )
    };
    assert_eq!(
        at(&groups[0], "back"),
        canonical(&migrate_route(
            "back",
            "swofty_settings",
            "frontend_flow",
            "fe_home"
        ))
    );
    assert_eq!(
        at(&groups[1], "back"),
        canonical(&subpage_route("back", "swofty_settings_2", "swofty_settings", 2))
    );
    assert_eq!(
        at(&groups[1], NEXT_PAGE),
        canonical(&subpage_route(
            NEXT_PAGE,
            "swofty_settings_2",
            "swofty_settings_3",
            1
        ))
    );
    assert_eq!(
        at(&groups[2], PREV_PAGE),
        canonical(&subpage_route(
            PREV_PAGE,
            "swofty_settings_3",
            "swofty_settings_2",
            2
        ))
    );
    let page = |i: usize| {
        groups
            .iter()
            .find(|g| id_of(g) == Some(&format!("swofty_addon_{i}")))
            .unwrap()
    };
    for (i, home) in [
        (0, "swofty_settings"),
        (30, "swofty_settings_2"),
        (69, "swofty_settings_3"),
    ] {
        let name = format!("swofty_addon_{i}");
        assert_eq!(
            at(page(i), "back"),
            canonical(&subpage_route("back", &name, home, 2))
        );
    }
}

#[test]
fn a_short_addon_settings_list_keeps_one_home_screen() {
    let homes = crate::ui::pages::home_plan(5);
    let pages: Vec<(String, String, String)> = (0..5)
        .map(|i| {
            (
                format!("open_addon_{i}"),
                format!("swofty_addon_{i}"),
                HOME.to_owned(),
            )
        })
        .collect();
    let groups = route_groups(&homes, &pages);
    assert_eq!(groups.len(), 6);
    let ids = route_ids(&groups[0]);
    assert_eq!(
        ids,
        [
            "open",
            "open_no_input",
            "close",
            "back",
            "open_addon_0",
            "open_addon_1",
            "open_addon_2",
            "open_addon_3",
            "open_addon_4"
        ]
    );
}

fn setup() -> SetupRoutes {
    SetupRoutes {
        route: "career_wizard_driver_career_pg".into(),
        home: "swofty_setup_career_wizard_driver_career_pg".into(),
        rows: "swofty_addon_career_wizard_driver_career_pg_2".into(),
        next: "career_wizard_season_length_select".into(),
        next_flow: "career_season_setup_flow".into(),
        back: "fe_career_dlc_select".into(),
        back_flow: "frontend_flow".into(),
        back_route: "back_career_wizard_driver_career_pg".into(),
    }
}

fn attr_pairs(list: &Node) -> Vec<(String, String)> {
    list.a()
        .unwrap()
        .iter()
        .map(|a| {
            let a = a.o().unwrap();
            (
                a.str_at("Id").unwrap().to_owned(),
                a.str_at("Value").unwrap().to_owned(),
            )
        })
        .filter(|(k, _)| k != "Id")
        .collect()
}

fn command_attrs(r: &Node) -> Vec<(String, Vec<(String, String)>)> {
    r.o()
        .unwrap()
        .at("Commands")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|c| {
            let o = c.o().unwrap();
            (
                o.str_at("Type").unwrap().to_owned(),
                attr_pairs(o.at("Attributes").unwrap()),
            )
        })
        .collect()
}

fn route_by_id<'a>(g: &'a Node, id: &str) -> &'a Node {
    g.o()
        .unwrap()
        .at("Routes")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .find(|r| id_of(r) == Some(id))
        .unwrap()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

fn file(groups: Vec<Node>) -> Vec<u8> {
    Pkg {
        name: [0; 4],
        root: obj(vec![("RouteGroupings", arr(groups))]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

#[test]
fn a_career_setup_gets_an_entry_route_a_page_pair_and_a_way_back_from_the_wizard() {
    let s = setup();
    let to_f1 = route(
        SETUP_ENTRY_SOURCE,
        vec![
            route_cmd("InputState", 1, &[], None),
            route_cmd(
                "TriggerSubRoute",
                2,
                &[("SubRoute", "close"), ("Target", &s.back), ("To", &s.next)],
                Some("1"),
            ),
            route_cmd(
                "Migrate",
                3,
                &[("Flow", "career_season_setup_flow"), ("SkipActivator", "true")],
                Some("2"),
            ),
            route_cmd(
                "TriggerSubRoute",
                4,
                &[
                    ("SubRoute", "open_no_input"),
                    ("Target", &s.next),
                    ("From", &s.back),
                ],
                Some("3"),
            ),
        ],
        "1,4",
        1,
        Some(&s.next),
    );
    let frontend = file(vec![route_group(&s.back, vec![to_f1.clone()])]);
    let mut next_group = route_group(&s.next, screen_routes(&s.next));
    next_group
        .o_mut()
        .unwrap()
        .at_mut("Attributes")
        .unwrap()
        .a_mut()
        .unwrap()
        .push(obj(vec![
            ("Id", crate::ui::node::s("Slices")),
            (
                "Value",
                crate::ui::node::s("career_wizard_summary,slice_gen_screen_title"),
            ),
        ]));
    let wizard = file(vec![next_group]);
    let mut placed = Placed::default();
    let frontend = setup_routes(&frontend, &[setup()], &mut placed).unwrap();
    let wizard = setup_routes(&wizard, &[setup()], &mut placed).unwrap();
    assert_eq!(
        placed.routes,
        [
            format!("{}.{}", s.back, s.route),
            format!("{}.{}", s.next, s.back_route)
        ]
    );
    assert_eq!(
        placed.screens,
        ["career_wizard_summary", "slice_gen_screen_title"]
    );
    let scene = pairs(&SETUP_SCENE);

    let p = Pkg::parse(&frontend).unwrap();
    let dlc = group(&p, &s.back).unwrap();
    assert_eq!(route_ids(dlc), [SETUP_ENTRY_SOURCE, s.route.as_str()]);
    assert_eq!(canonical(route_by_id(dlc, SETUP_ENTRY_SOURCE)), canonical(&to_f1));
    let entry = route_by_id(dlc, &s.route);
    assert_eq!(
        command_attrs(entry),
        [
            ("InputState".to_owned(), vec![]),
            (
                "TriggerSubRoute".to_owned(),
                pairs(&[("SubRoute", "close"), ("Target", &s.back), ("To", &s.home)])
            ),
            (
                "Migrate".to_owned(),
                pairs(&[("Flow", "options_flow"), ("SkipActivator", "true")])
            ),
            (
                "TriggerSubRoute".to_owned(),
                pairs(&[("SubRoute", "open"), ("Target", &s.home), ("From", &s.back)])
            ),
            ("FrontendSceneSwitch".to_owned(), scene.clone()),
        ]
    );
    let switch = &entry.o().unwrap().at("Commands").unwrap().a().unwrap()[4];
    assert_eq!(switch.o().unwrap().str_at("Dependencies").unwrap(), "1");
    assert_eq!(
        attr_pairs(entry.o().unwrap().at("Attributes").unwrap()),
        pairs(&[("Destination", &s.home)])
    );

    let p = Pkg::parse(&wizard).unwrap();
    let back = route_by_id(group(&p, &s.next).unwrap(), &s.back_route);
    let out = |t: &str| {
        (
            "TransitionScreen".to_owned(),
            pairs(&[("Direction", "out"), ("Target", t)]),
        )
    };
    let close = |t: &str| ("CloseScreen".to_owned(), pairs(&[("Target", t)]));
    assert_eq!(
        command_attrs(back),
        [
            out(&s.next),
            out("career_wizard_summary"),
            close("career_wizard_summary"),
            out("slice_gen_screen_title"),
            close("slice_gen_screen_title"),
            (
                "Migrate".to_owned(),
                pairs(&[("Flow", "options_flow"), ("SkipActivator", "true")])
            ),
            (
                "TriggerSubRoute".to_owned(),
                pairs(&[
                    ("SubRoute", "open"),
                    ("Target", &s.home),
                    ("From", &s.next),
                    ("Message", "")
                ])
            ),
            ("FrontendSceneSwitch".to_owned(), scene.clone()),
        ]
    );
    let deps: Vec<&str> = back
        .o()
        .unwrap()
        .at("Commands")
        .unwrap()
        .a()
        .unwrap()
        .iter()
        .map(|c| c.o().unwrap().str_at("Dependencies").unwrap_or(""))
        .collect();
    assert_eq!(deps, ["", "", "2", "", "4", "1,3,5", "6", "6"]);
    let back = back.o().unwrap();
    assert_eq!(back.str_at("Dependencies").unwrap(), "7");
    assert_eq!(
        attr_pairs(back.at("Attributes").unwrap()),
        pairs(&[("Destination", &s.home), ("AutoFocus", "false")])
    );

    let groups = setup_route_groups(&s);
    assert_eq!(
        groups.iter().map(|g| id_of(g).unwrap()).collect::<Vec<_>>(),
        [s.home.as_str(), s.rows.as_str()]
    );
    assert_eq!(
        route_ids(&groups[0]),
        [
            "open",
            "open_no_input",
            "close",
            "back",
            "confirm",
            "open_options"
        ]
    );
    assert_eq!(route_ids(&groups[1]), ["open", "open_no_input", "close", "back"]);
    assert_eq!(
        attr_pairs(
            route_by_id(&groups[0], "back")
                .o()
                .unwrap()
                .at("Attributes")
                .unwrap()
        ),
        pairs(&[("Destination", &s.back), ("AutoFocus", "false")])
    );
    let back = command_attrs(route_by_id(&groups[0], "back"));
    assert_eq!(
        back[1],
        (
            "Migrate".to_owned(),
            pairs(&[("Flow", "frontend_flow"), ("SkipActivator", "true")])
        )
    );
    assert_eq!(
        back[2].1,
        pairs(&[
            ("SubRoute", "open"),
            ("Target", &s.back),
            ("From", &s.home),
            ("Message", DRIVER_CAREER)
        ])
    );
    let confirm = command_attrs(route_by_id(&groups[0], "confirm"));
    assert_eq!(
        confirm[1].1,
        pairs(&[("Flow", "career_season_setup_flow"), ("SkipActivator", "true")])
    );
    assert_eq!(confirm[2].1[1], ("Target".to_owned(), s.next.clone()));
    assert_eq!(
        canonical(route_by_id(&groups[1], "back")),
        canonical(&subpage_route("back", &s.rows, &s.home, 2))
    );

    let e = setup_routes(&frontend, &[setup()], &mut Placed::default()).unwrap_err();
    assert!(
        e.to_string()
            .contains(&format!("{} has a route {} already", s.back, s.route)),
        "{e}"
    );
    let bare = file(vec![route_group(&s.back, vec![])]);
    let e = setup_routes(&bare, &[setup()], &mut Placed::default()).unwrap_err();
    assert!(e.to_string().contains(SETUP_ENTRY_SOURCE), "{e}");
}

fn stock_continue(to: &str) -> Node {
    route(
        &format!(
            "solo_career_continue_{}",
            if to == "career_continue_driver" {
                "driver"
            } else {
                "my_team"
            }
        ),
        vec![
            route_cmd(
                "Wait",
                1,
                &[("Time", "0"), ("DilationCurve", "tran_pre_wipe_dly_gen")],
                None,
            ),
            route_cmd(
                "StartStandardWipe",
                2,
                &[("Video", "ui_wipes/ui_wipe_standard_in.bk2")],
                Some("1"),
            ),
            route_cmd(
                "Wait",
                3,
                &[("Time", "0"), ("DilationCurve", "tran_wipe_stndrd_in_dly_c")],
                None,
            ),
            route_cmd("InputState", 4, &[], Some("2")),
            route_cmd(
                "TriggerSubRoute",
                5,
                &[("SubRoute", "close"), ("Target", "fe_career"), ("To", to)],
                Some("3"),
            ),
            route_cmd(
                "TriggerSubRoute",
                6,
                &[
                    ("SubRoute", "open_no_input"),
                    ("Target", to),
                    ("From", "fe_career"),
                    ("Message", ""),
                ],
                Some("5"),
            ),
        ],
        "4,6",
        1,
        Some(to),
    )
}

fn stock_resume_routes() -> Vec<u8> {
    let career = route(
        "career",
        vec![
            route_cmd(
                "TriggerSubRoute",
                1,
                &[("SubRoute", "close"), ("Target", "fe_home"), ("To", "fe_career")],
                None,
            ),
            route_cmd(
                "TriggerSubRoute",
                2,
                &[
                    ("SubRoute", "open"),
                    ("Target", "fe_career"),
                    ("From", "fe_home"),
                    ("Message", ""),
                ],
                Some("1"),
            ),
        ],
        "2",
        1,
        Some("fe_career"),
    );
    let options = route(
        "options",
        vec![
            route_cmd(
                "Wait",
                1,
                &[("Time", "0"), ("DilationCurve", "tran_pre_wipe_dly_gen")],
                None,
            ),
            route_cmd(
                "TriggerSubRoute",
                2,
                &[
                    ("SubRoute", "close"),
                    ("Target", "fe_home"),
                    ("To", "settings_container"),
                ],
                Some("1"),
            ),
            route_cmd(
                "Migrate",
                3,
                &[("Flow", "options_flow"), ("SkipActivator", "true")],
                Some("2"),
            ),
            route_cmd(
                "TriggerSubRoute",
                4,
                &[
                    ("SubRoute", "open_no_input"),
                    ("Target", "settings_container"),
                    ("From", "fe_home"),
                    ("Message", ""),
                ],
                Some("3"),
            ),
        ],
        "4",
        1,
        Some("settings_container"),
    );
    pkg_of(vec![
        route_group("fe_home", vec![career, options]),
        route_group(
            "fe_career",
            vec![
                stock_continue("career_continue_driver"),
                stock_continue("career_continue_my_team"),
            ],
        ),
        route_group(
            "fe_career_dlc_select",
            vec![stock_continue("career_continue_driver")],
        ),
    ])
}

fn resumed() -> (Pkg, Pkg, Vec<String>) {
    let stock = stock_resume_routes();
    let mut placed = Vec::new();
    let out = resume_routes(
        &stock,
        &ResumeRoutes {
            frontend_flow: "frontend_flow".into(),
        },
        &mut placed,
    )
    .unwrap();
    (Pkg::parse(&stock).unwrap(), Pkg::parse(&out).unwrap(), placed)
}

#[test]
fn the_list_continues_with_every_command_of_the_stock_continue_route_retargeted() {
    let (before, after, _) = resumed();
    let list = crate::ui::pages::RESUME_SCREEN;
    for (id, to) in [
        ("solo_career_continue_driver", "career_continue_driver"),
        ("solo_career_continue_my_team", "career_continue_my_team"),
    ] {
        let stock = command_attrs(route_in(&before, "fe_career", id));
        let here = command_attrs(route_in(&after, list, id));
        let kinds: Vec<&str> = here.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "Wait",
                "StartStandardWipe",
                "Wait",
                "InputState",
                "TriggerSubRoute",
                "TriggerSubRoute",
                "Migrate"
            ]
        );
        let retargeted: Vec<_> = stock
            .into_iter()
            .map(|(k, a)| {
                let a = a
                    .into_iter()
                    .map(|(n, v)| (n, if v == "fe_career" { list.to_owned() } else { v }))
                    .collect();
                (k, a)
            })
            .collect();
        assert_eq!(&here[..6], &retargeted[..], "{id}");
        assert_eq!(
            here[6].1,
            pairs(&[("Flow", "frontend_flow"), ("SkipActivator", "true")])
        );
        let s = steps(route_in(&after, list, id));
        assert!(
            s[5].ends_with(" 7") && s[6].ends_with(" 5"),
            "open follows the move, the move the close: {s:?}"
        );
        assert!(
            s[4].contains(&format!("close {list}")) && s[5].contains(&format!("open_no_input {to}")),
            "{s:?}"
        );
    }
}

#[test]
fn the_career_tile_opens_the_list_and_the_list_reaches_the_career_menu_and_the_home() {
    let (before, after, placed) = resumed();
    let list = crate::ui::pages::RESUME_SCREEN;
    let career = route_in(&after, "fe_home", "career");
    assert_ne!(
        command_attrs(career),
        command_attrs(route_in(&before, "fe_home", "career"))
    );
    let kinds: Vec<_> = command_attrs(career);
    assert!(
        kinds
            .iter()
            .any(|(k, a)| k == "Migrate" && a.contains(&("Flow".to_owned(), "options_flow".to_owned())))
    );
    assert!(
        kinds
            .iter()
            .any(|(_, a)| a.contains(&("Target".to_owned(), list.to_owned())))
    );
    assert!(!kinds.iter().any(|(_, a)| a.iter().any(|(_, v)| v == "fe_career")));
    assert_eq!(route_ids_of(&after, "fe_home"), ["options", "career"]);

    let new_career = command_attrs(route_in(&after, list, crate::ui::pages::NEW_CAREER_ROUTE));
    let kinds: Vec<&str> = new_career.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, ["TriggerSubRoute", "Migrate", "TriggerSubRoute"]);
    assert_eq!(
        new_career[0].1,
        pairs(&[("SubRoute", "close"), ("Target", list), ("To", "fe_career")])
    );
    assert_eq!(
        new_career[2].1,
        pairs(&[
            ("SubRoute", "open"),
            ("Target", "fe_career"),
            ("From", list),
            ("Message", "")
        ]),
        "the same open step the main menu's Career route had"
    );
    let back = command_attrs(route_in(&after, list, "back"));
    assert!(
        back.iter()
            .any(|(_, a)| a.contains(&("Target".to_owned(), "fe_home".to_owned())))
    );
    assert!(!back.iter().any(|(_, a)| a.iter().any(|(_, v)| v == "fe_career")));
    assert_eq!(
        placed,
        [
            "fe_home.career".to_owned(),
            format!("fe_career.{}", crate::ui::pages::RESUME_ROUTE),
            format!("fe_career_dlc_select.{}", crate::ui::pages::RESUME_ROUTE)
        ]
    );
}

#[test]
fn resume_routes_fail_when_the_main_menu_has_no_career_route() {
    let mut p = Pkg::parse(&stock_resume_routes()).unwrap();
    let gs = root_obj(&mut p)
        .unwrap()
        .at_mut("RouteGroupings")
        .unwrap()
        .a_mut()
        .unwrap();
    let home = gs.iter_mut().find(|g| id_of(g) == Some("fe_home")).unwrap();
    home.o_mut()
        .unwrap()
        .at_mut("Routes")
        .unwrap()
        .a_mut()
        .unwrap()
        .retain(|r| id_of(r) != Some("career"));
    let e = resume_routes(
        &p.to_bytes().unwrap(),
        &ResumeRoutes {
            frontend_flow: "frontend_flow".into(),
        },
        &mut Vec::new(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("career"), "{e}");
}
