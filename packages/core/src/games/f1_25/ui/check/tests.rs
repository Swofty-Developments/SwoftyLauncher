use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Pkg};

use super::archives::*;
use super::findings::*;
use super::flows::*;
use super::integrity::*;
use super::props::*;
use super::stock::*;
use super::*;
use crate::ui::catalog::{Catalog, Engine, Scenes};
use crate::ui::idf;
use crate::ui::node::*;
use crate::ui::node::{arr, boolv, command, event, f32v, fix_ids, node, obj, prop, s, sig, u32v, ui_bytes};

fn screen_with(kids: Vec<Node>, vm: bool) -> Vec<u8> {
    let mut props = vec![];
    if vm {
        props.push(("allowVMBinding", prop(boolv(true), None)));
    }
    let mut scr = node("page", "UISC", props, vec![], &[], &[]);
    scr.a_mut().unwrap()[CHILDREN] = arr(kids);
    ui_bytes(vec![scr]).unwrap()
}

#[test]
fn integrity_finds_stale_hashes_and_missing_targets() {
    use crate::ui::node::{event, sig};
    let kid = node(
        "kid",
        "UIGP",
        vec![],
        vec![event("pressed", vec![sig("page.kid", "focus", "pressed")])],
        &["slot"],
        &[],
    );
    let mut scr = node("page", "UISC", vec![], vec![], &[], &[]);
    scr.a_mut().unwrap()[CHILDREN] = arr(vec![kid]);
    let mut bad = scr.clone();
    fix_ids(&mut scr, None);
    let (nodes, signals, problems) = integrity(&ui_bytes(vec![scr]).unwrap()).unwrap();
    assert_eq!((nodes, signals), (2, 1));
    assert!(problems.is_empty(), "{problems:?}");
    let kid = &mut bad.slot_mut(CHILDREN).unwrap()[0];
    let e = &mut kid.slot_mut(EVENTS).unwrap()[0];
    let s = &mut e.o_mut().unwrap().at_mut("signals").unwrap().a_mut().unwrap()[0];
    set_str(s.o_mut().unwrap(), "to_object_name", "page.gone").unwrap();
    let (_, _, problems) = integrity(&ui_bytes(vec![bad]).unwrap()).unwrap();
    assert_eq!(
        problems,
        [
            "page: id",
            "page.kid: id",
            "page.kid: hash page.gone.focus",
            "page.kid: on pressed -> missing page.gone.focus"
        ]
    );
}

#[test]
fn list_controls_need_bound_data() {
    let bound = node(
        "ok",
        "UITG",
        vec![("data", obj(vec![("value", s("")), ("dbentry", s("#a.b"))]))],
        vec![],
        &[],
        &[],
    );
    let literal = node(
        "bad",
        "UITG",
        vec![("data", prop(s("x"), None))],
        vec![],
        &[],
        &[],
    );
    let files = vec![("page".to_owned(), screen_with(vec![bound, literal], false))];
    let out = screens(&files, &[]).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("page.bad [UITG]"));
}

#[test]
fn view_model_binding_needs_a_view_model() {
    let files = vec![("page".to_owned(), screen_with(vec![], true))];
    assert_eq!(screens(&files, &[]).unwrap().len(), 1);
    assert!(screens(&files, &["page".to_owned()]).unwrap().is_empty());
    let plain = vec![("page".to_owned(), screen_with(vec![], false))];
    assert_eq!(screens(&plain, &["page".to_owned()]).unwrap().len(), 1);
}

fn cmd(kind: &str, attrs: &[(&str, &str)]) -> Node {
    let a = attrs
        .iter()
        .map(|(k, v)| obj(vec![("Id", s(k)), ("Value", s(v))]))
        .collect();
    obj(vec![("Attributes", arr(a)), ("Type", s(kind))])
}

fn route(id: &str, cmds: Vec<Node>) -> Node {
    obj(vec![("Id", s(id)), ("Commands", arr(cmds))])
}

fn pkg(root: Node) -> Vec<u8> {
    Pkg {
        name: [0; 4],
        root,
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

#[test]
fn routes_must_reach_screens_and_sub_routes() {
    let entry = node(
        "enter_page",
        "FLEN",
        vec![("screen", prop(s("page"), None))],
        vec![],
        &[],
        &[],
    );
    let mut scene = node("scene", "FLSC", vec![], vec![], &[], &[]);
    scene.a_mut().unwrap()[CHILDREN] = arr(vec![entry]);
    let mut flow = node(
        "options_flow",
        "FLOW",
        vec![(
            "asset_group",
            prop(s("asset_groups/ui_package/flow_options.assetgroup"), None),
        )],
        vec![],
        &[],
        &[],
    );
    flow.a_mut().unwrap()[CHILDREN] = arr(vec![scene]);
    let flows = pkg(obj(vec![("Flows", arr(vec![flow]))]));
    let group = |id: &str, routes: Vec<Node>| obj(vec![("Id", s(id)), ("Routes", arr(routes))]);
    let routes = pkg(obj(vec![(
        "RouteGroupings",
        arr(vec![
            group(
                "page",
                vec![route("open", vec![cmd("OpenScreen", &[("Target", "page")])])],
            ),
            group(
                "home",
                vec![
                    route(
                        "go",
                        vec![cmd(
                            "TriggerSubRoute",
                            &[("SubRoute", "open"), ("Target", "page")],
                        )],
                    ),
                    route(
                        "bad",
                        vec![
                            cmd("TriggerSubRoute", &[("SubRoute", "shut"), ("Target", "page")]),
                            cmd("OpenScreen", &[("Target", "nowhere")]),
                            cmd("Migrate", &[("Flow", "no_flow")]),
                        ],
                    ),
                ],
            ),
        ]),
    )]));
    let registered = BTreeMap::from([("page".to_owned(), "flow_options".to_owned())]);
    let out = super::routes(&routes, &flows, &["page", "home.go"], &registered).unwrap();
    assert!(out.is_empty(), "{out:?}");
    let out = super::routes(&routes, &flows, &["home"], &registered).unwrap();
    assert_eq!(out.len(), 3, "{out:?}");
    let elsewhere = BTreeMap::from([("page".to_owned(), "flow_frontend".to_owned())]);
    let out = super::routes(&routes, &flows, &[], &elsewhere).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
}

fn cat() -> Catalog {
    serde_json::from_value(serde_json::json!({
        "node_types": {
            "UIGP": {
                "roles": { "template_root": 1, "child": 1 },
                "parents": { "UIGP": 1 },
                "props": { "visible": { "count": 1, "shapes": { "bool": 1 } } },
                "events": { "navUp": { "count": 3, "no_signals": 0 }, "anim_complete": { "count": 2, "no_signals": 2 } },
                "commands": { "focus": { "count": 1, "no_signals": 0 } },
                "pins_in": { "focus": { "count": 1 }, "raise_command": { "count": 1, "attributes": { "command.name.value:stri": 1 } } },
                "pins_out": { "navUp": 1, "anim_complete": 1, "focus": 1 },
                "links": { "UIGP.focus": 1, "UIGP.raise_command": 1 },
                "layout": { "child:node": 1, "child:node>node": 1 },
                "layout_elements": { "eXRef": 1, "eXRef>eNull": 1 }
            }
        },
        "scene_elements": {
            "eScene": {},
            "eXRef": { "parents": { "eScene": 1, "eMaskTexture": 1 }, "params": { "fileRef": { "count": 1, "types": { "stri": 1 } } } },
            "eNull": { "parents": { "eScene": 1 } },
            "eMaskTexture": { "parents": { "eScene": 1 } },
            "eMask": { "parents": { "eNull": 1 }, "params": {} }
        }
    }))
    .unwrap()
}

fn u64v(v: u64) -> Node {
    Node::Val(swofty_formats::pkg::Val::raw(*b"ui64", v.to_le_bytes()))
}

fn el(name: &str, ty: &str, kids: Vec<Node>, xref: Option<u64>) -> Node {
    let mut t = ty.as_bytes().to_vec();
    t.resize(16, 0);
    let mut pairs = vec![
        ("name", s(name)),
        ("typeName", Node::Val(swofty_formats::pkg::Val::raw(*b"shnm", t))),
        ("params", arr(vec![])),
        ("children", arr(kids)),
    ];
    if let Some(x) = xref {
        pairs.push(("xref", u64v(x)));
    }
    obj(pairs)
}

fn scene(id: u64, kids: Vec<Node>) -> Vec<u8> {
    let entry = obj(vec![
        ("scene", el("Scene Root", "eScene", kids, None)),
        ("name", u64v(id)),
        ("nodeidlookup", arr(vec![])),
    ]);
    pkg(obj(vec![("scenelist", arr(vec![entry]))]))
}

fn template_node(kids: Vec<Node>, scene_id: u64) -> Node {
    let mut root = node("tpl", "UIGP", vec![], vec![], &[], &[]);
    root.a_mut().unwrap()[CHILDREN] = arr(kids);
    root.a_mut().unwrap()[COMPONENTS] = arr(vec![obj(vec![
        ("componentId", u64v(scene_id)),
        ("debugName", s("eaid://ui_package/art/x")),
        ("name", s("x")),
    ])]);
    fix_ids(&mut root, None);
    root
}

fn template(kids: Vec<Node>, scene_id: u64) -> Vec<u8> {
    pkg(obj(vec![("Templates", arr(vec![template_node(kids, scene_id)]))]))
}

#[test]
fn catalogue_flags_what_stock_never_does() {
    let mut row = node(
        "row",
        "UIGP",
        vec![
            ("visible", prop(s("yes"), None)),
            ("invented", prop(boolv(true), None)),
        ],
        vec![
            event("navUp", vec![]),
            event("anim_complete", vec![sig("tpl.row", "focus", "anim_complete")]),
            event("mystery", vec![sig("tpl.row", "focus", "mystery")]),
        ],
        &[],
        &[],
    );
    row.a_mut().unwrap()[COMMANDS] = arr(vec![command("focus", vec![sig("tpl.row", "explode", "focus")])]);
    let other = node("other", "UICX", vec![], vec![], &[], &[]);
    let out = against_stock(&template(vec![row, other], 1), &cat(), &Scenes::default()).unwrap();
    let has = |s: &str| out.iter().any(|f| f.to_string().contains(s));
    assert!(has("property visible is stri; stock UIGP uses bool"), "{out:#?}");
    assert!(has("property invented is never set"), "{out:#?}");
    assert!(has("event navUp has no signals"), "{out:#?}");
    assert!(has("event anim_complete has signals; no stock"), "{out:#?}");
    assert!(has("event mystery is never declared"), "{out:#?}");
    assert!(has("no stock signal reaches pin explode on a UIGP"), "{out:#?}");
    assert!(has("node type UICX is not in any stock .ui"), "{out:#?}");
    assert_eq!(out.len(), 8, "{out:#?}");
}

#[test]
fn layout_paths_through_a_mask_are_flagged() {
    let mut scenes = Scenes::default();
    let row = el("row", "eXRef", vec![], Some(2));
    scenes
        .add(&scene(1, vec![el("row_mask", "eMaskTexture", vec![row], None)]))
        .unwrap();
    scenes
        .add(&scene(2, vec![el("item_spawner", "eNull", vec![], None)]))
        .unwrap();
    let direct = node("a", "UIGP", vec![], vec![], &["row"], &[]);
    let nested = node("b", "UIGP", vec![], vec![], &["row", "item_spawner"], &[]);
    let masked = node("c", "UIGP", vec![], vec![], &["row_mask", "row"], &[]);
    let out = against_stock(&template(vec![direct, nested, masked], 1), &cat(), &scenes).unwrap();
    assert_eq!(out.len(), 1, "{out:#?}");
    assert!(
        out[0]
            .to_string()
            .starts_with("tpl.c [UIGP]: layout row_mask>row walks eMaskTexture>eXRef"),
        "{out:#?}"
    );
}

#[test]
fn scenes_are_checked_for_parenting_and_params() {
    let mut xref = el("row", "eXRef", vec![], Some(2));
    xref.as_obj_mut().unwrap().set(
        "params",
        arr(vec![obj(vec![("name", s("fileRef")), ("value", u32v(3))])]),
    );
    let data = scene(
        1,
        vec![
            el("m", "eMask", vec![xref], None),
            el("w", "eWiggle", vec![], None),
        ],
    );
    let out = scene_against_stock(&data, &cat()).unwrap();
    let has = |s: &str| out.iter().any(|f| f.to_string().contains(s));
    assert!(
        has("m [eMask]: stock never puts a eMask under a eScene"),
        "{out:#?}"
    );
    assert!(
        has("m.row [eXRef]: stock never puts a eXRef under a eMask"),
        "{out:#?}"
    );
    assert!(has("param fileRef is ui32; stock eXRef uses stri"), "{out:#?}");
    assert!(has("element type eWiggle is not in any stock scene"), "{out:#?}");
}

#[test]
fn the_stock_catalogue_knows_the_row_navigation_pins() {
    let c = super::super::catalog::stock();
    let f1ts = &c.node_types["F1TS"];
    for pin in ["focus", "inputAllow", "inputDisallow", "focus_and_reset"] {
        assert!(f1ts.pins_in.contains_key(pin), "{pin}");
    }
    assert!(c.node_types["UIGP"].commands.contains_key("block_input"));
    assert!(c.scene_elements["eMaskTexture"].children.contains_key("eXRef"));
}

fn engine() -> Engine {
    let rule = |id: &str, code: u64| serde_json::json!({ "id": id, "level": "L2", "check": "", "fail": { "code": code, "scope": "node" }, "fn": format!("0x14000{code:04x} {id}") });
    let with = |mut r: serde_json::Value, applies: serde_json::Value, props: serde_json::Value| {
        r["applies_to"] = applies;
        r["props"] = props;
        r
    };
    serde_json::from_value(serde_json::json!({
        "schema": 1,
        "rules": [
            rule("node.record", 5), rule("node.class_registered", 5), rule("layout.path_nonempty", 34),
            rule("layout.step_keys", 3), rule("components.keys", 3), rule("behaviours.type", 3),
            rule("commands.entry", 3), rule("events.entry", 3), rule("signals.entry", 3),
            with(rule("listbase.data_binding", 5), serde_json::json!({ "classes": ["UIL2"] }), serde_json::json!([{ "key": "data", "required": true, "shape": "binding" }])),
            with(rule("svcd.value", 3), serde_json::json!({ "classes": ["SVCD"] }), serde_json::json!([{ "key": "value", "required": true }])),
            with(rule("f1ts.input_type", 87), serde_json::json!({ "classes": ["F1TS"] }), serde_json::json!([{ "key": "inputType", "shape": "value", "values": ["normal", "dpadOnly"], "case_insensitive": true }])),
            with(rule("behaviour.nbvn_visualnav", 3), serde_json::json!({ "behaviours": ["NBVN"] }), serde_json::json!([{ "key": "visualNav", "required": true, "shape": "object" }])),
            rule("template.duplicate", 88), rule("screen.duplicate", 85)
        ],
        "classes": {
            "UIGP": { "init_calls_element_base": true },
            "F1TS": { "init_calls_element_base": true },
            "SVCD": { "init_calls_element_base": true },
            "UIL2": { "init_calls_element_base": true, "init_calls_list_base": true }
        }
    }))
    .unwrap()
}

#[test]
fn a_visual_nav_behaviour_needs_its_visual_nav_object() {
    let root = |props: Vec<(&str, Node)>| {
        let mut r = node("tpl", "UIGP", props, vec![], &[], &["NBEL", "NBVN"]);
        r.a_mut().unwrap()[COMPONENTS] = arr(vec![obj(vec![
            ("componentId", u64v(1)),
            ("debugName", s("eaid://ui_package/art/x")),
            ("name", s("x")),
        ])]);
        fix_ids(&mut r, None);
        pkg(obj(vec![("Templates", arr(vec![r]))]))
    };
    let nav = obj(vec![
        ("xOffset", prop(f32v(0.0), None)),
        ("yOffset", prop(f32v(0.0), None)),
    ]);
    let missing = root(vec![]);
    let report = ui_edit(&input(&missing, None), &Scenes::default(), &cat(), &engine()).unwrap();
    assert_eq!(report.hard.len(), 1, "{report:#?}");
    assert_eq!(report.hard[0].code, "behaviour.nbvn_visualnav");
    assert!(
        report.hard[0]
            .message
            .contains("UIGP with NBVN: visualNav is missing"),
        "{report:#?}"
    );
    let scalar = root(vec![("visualNav", u32v(0))]);
    let report = ui_edit(&input(&scalar, None), &Scenes::default(), &cat(), &engine()).unwrap();
    assert!(
        report
            .hard
            .iter()
            .any(|h| h.message.contains("visualNav is not an object")),
        "{report:#?}"
    );
    let fine = root(vec![("visualNav", nav)]);
    let report = ui_edit(&input(&fine, None), &Scenes::default(), &cat(), &engine()).unwrap();
    assert!(report.hard.is_empty(), "{report:#?}");
}

fn input<'a>(data: &'a [u8], stock: Option<&'a [u8]>) -> UiInput<'a> {
    UiInput {
        resource: "eaid://ui_package/screens/x.ui",
        frag: "syst",
        data,
        stock,
    }
}

#[test]
fn engine_rules_reject_what_the_loader_rejects() {
    let mut row = node("row", "UIGP", vec![], vec![], &[], &[]);
    row.a_mut().unwrap()[EVENTS] = arr(vec![obj(vec![
        ("eventName", s("navUp")),
        ("event", u32v(djb2("navUp"))),
    ])]);
    row.a_mut().unwrap()[LAYOUT] = arr(vec![arr(vec![])]);
    let spawner = node(
        "sp",
        "F1TS",
        vec![("inputType", prop(s("stick"), None))],
        vec![],
        &["a"],
        &[],
    );
    let query = node("q", "SVCD", vec![], vec![], &[], &[]);
    let list = node("l", "UIL2", vec![("data", prop(s("x"), None))], vec![], &[], &[]);
    let alien = node("z", "UIZZ", vec![], vec![], &[], &[]);
    let mut bad_step = node("b", "UIGP", vec![], vec![], &[], &[]);
    bad_step.a_mut().unwrap()[LAYOUT] = arr(vec![arr(vec![obj(vec![("debugName", s("b"))])])]);
    let tpl = template_node(vec![row, spawner, query, list, alien, bad_step], 1);
    let mut other = template_node(vec![], 2);
    other.props_mut().unwrap().set("name", s("tile"));
    let mut cat = cat();
    cat.root_names.insert(
        "Templates".into(),
        BTreeMap::from([(
            "tile".to_owned(),
            vec!["eaid://ui_package/screens/y.ui".to_owned()],
        )]),
    );
    let data = pkg(obj(vec![("Templates", arr(vec![tpl.clone(), tpl, other]))]));
    let report = ui_edit(&input(&data, None), &Scenes::default(), &cat, &engine()).unwrap();
    let ids: BTreeSet<&str> = report.hard.iter().map(|h| h.code.as_str()).collect();
    for id in [
        "events.entry",
        "layout.path_nonempty",
        "layout.step_keys",
        "f1ts.input_type",
        "svcd.value",
        "listbase.data_binding",
        "node.class_registered",
        "template.duplicate",
    ] {
        assert!(ids.contains(id), "{id} missing: {report:#?}");
    }
    let dup: Vec<&Finding> = report
        .hard
        .iter()
        .filter(|h| h.code == "template.duplicate")
        .collect();
    assert_eq!(dup.len(), 2, "{dup:#?}");
    assert!(dup.iter().any(|h| h.message.contains("y.ui")), "{dup:#?}");
    let f1ts = report.hard.iter().find(|h| h.code == "f1ts.input_type").unwrap();
    assert!(
        f1ts.source.starts_with("f1ts.input_type L2: rc 0x57"),
        "{f1ts:#?}"
    );
    assert!(f1ts.source.contains("0x140000057"), "{f1ts:#?}");
    let stock = pkg(obj(vec![("Templates", arr(vec![template_node(vec![], 1)]))]));
    let clean = ui_edit(&input(&stock, Some(&stock)), &Scenes::default(), &cat, &engine()).unwrap();
    assert!(clean.hard.is_empty() && clean.warnings.is_empty(), "{clean:#?}");
}

#[test]
fn the_engine_rules_file_loads() {
    let e = super::super::catalog::engine();
    assert!(e.rules.len() >= 18);
    assert_eq!(e.classes.len(), 136);
    for id in [
        "node.record",
        "events.entry",
        "signals.entry",
        "template.duplicate",
        "f1ts.input_type",
    ] {
        assert!(e.rule(id).is_some(), "{id}");
    }
    assert!(e.classes["F1TS"].native_pins.iter().any(|p| p == "inputAllow"));
    for id in [
        "behaviour.nbvn_visualnav",
        "svcd.value",
        "f1ts.input_type",
        "listbase.data_binding",
    ] {
        let r = e.rule(id).unwrap();
        assert!(!r.props.is_empty(), "{id}");
        assert!(
            !r.applies_to.classes.is_empty() || !r.applies_to.behaviours.is_empty(),
            "{id}"
        );
    }
}

fn scene_at(path: &str, kids: Vec<Node>) -> Vec<u8> {
    let mut entry = obj(vec![
        ("scene", el("Scene Root", "eScene", kids, None)),
        ("name", u64v(idf::scene_id(path))),
        ("nodeidlookup", arr(vec![])),
    ]);
    idf::relookup(&mut entry).unwrap();
    let mut p = Pkg::parse(&pkg(obj(vec![("scenelist", arr(vec![entry]))]))).unwrap();
    for e in idf::scenes_mut(&mut p).unwrap() {
        let root = e.o_mut().unwrap().at_mut("scene").unwrap();
        for c in idf::children_mut(root).unwrap() {
            let n = idf::elem_name(c).to_owned();
            idf::recompute_ids(c, &n).unwrap();
        }
    }
    p.to_bytes().unwrap()
}

#[test]
fn scene_edits_are_checked_against_the_ui_that_binds_them() {
    let path = "eaid://ui_package/art/x.idf";
    let id = idf::scene_id(path);
    let ui = "eaid://ui_package/screens/x.ui";
    let tpl = |kids: Vec<Node>| pkg(obj(vec![("Templates", arr(vec![template_node(kids, id)]))]));
    let bound = || node("row", "UIGP", vec![], vec![], &["row"], &[]);
    let stock_scene = scene_at(path, vec![el("row", "eNull", vec![], None)]);
    let archive = |name: &str, scene: &[u8], ui_data: &[u8]| ArchiveUi {
        archive: name.into(),
        resources: BTreeMap::from([
            (path.to_owned(), ("UILayout".to_owned(), vec![scene.to_vec()])),
            (ui.to_owned(), ("UIDF".to_owned(), vec![ui_data.to_vec()])),
        ]),
    };
    let stock_ui = tpl(vec![bound()]);
    let stock = [
        archive("a.erp", &stock_scene, &stock_ui),
        archive("b.erp", &stock_scene, &stock_ui),
    ];
    let mut cat = cat();
    cat.scene_ids = vec![format!("{id:016x}")];
    let engine = engine();

    let renamed = scene_at(path, vec![el("row_2", "eNull", vec![], None)]);
    let new_ui = tpl(vec![
        bound(),
        node("extra", "UIGP", vec![], vec![], &["row"], &[]),
    ]);
    let edited = [
        archive("a.erp", &renamed, &new_ui),
        archive("b.erp", &stock_scene, &stock_ui),
    ];
    let report = archives(&edited, &stock, &cat, &engine).unwrap();
    let has = |code: &str, at: &str| {
        report
            .hard
            .iter()
            .chain(&report.warnings)
            .any(|f| f.code == code && f.to_string().contains(at))
    };
    assert!(
        has(
            "layout-node-unknown",
            "a.erp: eaid://ui_package/screens/x.ui ui#0: tpl.extra"
        ),
        "{report:#?}"
    );
    assert!(has("layout-node-missing", "tpl.row"), "{report:#?}");
    assert!(has("copies-differ", "a.erp and b.erp"), "{report:#?}");
    assert!(report.hard.iter().all(|h| h.level == Level::Hard));
    assert!(
        report
            .hard
            .iter()
            .any(|h| h.code == "layout-node-missing" && h.at.contains("tpl.row")),
        "{report:#?}"
    );

    let mut stale = Pkg::parse(&stock_scene).unwrap();
    let entry = &mut idf::scenes_mut(&mut stale).unwrap()[0];
    let root = entry.o_mut().unwrap().at_mut("scene").unwrap();
    idf::children_mut(root)
        .unwrap()
        .push(el("row_3", "eNull", vec![], None));
    entry.o_mut().unwrap().set("name", u64v(7));
    let stale = stale.to_bytes().unwrap();
    let edited = [
        archive("a.erp", &stale, &stock_ui),
        archive("b.erp", &stale, &stock_ui),
    ];
    let report = archives(&edited, &stock, &cat, &engine).unwrap();
    let codes: BTreeSet<&str> = report.hard.iter().map(|h| h.code.as_str()).collect();
    assert!(
        codes.contains("nodeidlookup") && codes.contains("scene-id"),
        "{report:#?}"
    );

    let report = archives(&stock, &stock, &cat, &engine).unwrap();
    assert!(
        report.hard.is_empty() && report.warnings.is_empty(),
        "{report:#?}"
    );
}

#[test]
fn bindings_to_view_model_keys_stock_never_binds_are_flagged() {
    let spawner = |name: &str, key: &str| {
        node(
            name,
            "F1TS",
            vec![("dataContext", prop(s(""), Some(key)))],
            vec![],
            &[],
            &[],
        )
    };
    let stock = template(vec![spawner("a", "tiles")], 1);
    let query = node(
        "has",
        "SVCD",
        vec![("dbEntry", prop(s("addon_tiles_visible"), None))],
        vec![],
        &[],
        &[],
    );
    let global = node(
        "g",
        "UILB",
        vec![("text", prop(s(""), Some("#hud.title")))],
        vec![],
        &[],
        &[],
    );
    let edited = template(
        vec![
            spawner("a", "tiles"),
            spawner("b", "addon_tiles.list"),
            query,
            global,
        ],
        1,
    );
    let found = runtime_bindings(&stock, &edited, "syst").unwrap();
    let keys: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(keys, ["addon_tiles", "addon_tiles_visible"], "{found:#?}");
    assert!(
        found
            .iter()
            .all(|f| f.level == Level::Warn && f.code == "vm-key-new")
    );
}

#[test]
fn broken_hashes_are_hard_and_signals_leaving_the_file_are_not() {
    let row = node(
        "row",
        "UIGP",
        vec![],
        vec![event("navUp", vec![sig("elsewhere.node", "focus", "navUp")])],
        &[],
        &[],
    );
    let mut root = template_node(vec![row], 1);
    root.find_mut("row")
        .unwrap()
        .props_mut()
        .unwrap()
        .set("nameId", u32v(1));
    let data = pkg(obj(vec![("Templates", arr(vec![root]))]));
    let report = ui_edit(&input(&data, None), &Scenes::default(), &cat(), &engine()).unwrap();
    assert_eq!(report.hard.len(), 1, "{report:#?}");
    assert!(
        report.hard[0].to_string().contains("tpl.row: nameId"),
        "{report:#?}"
    );
}

#[test]
fn a_data_setter_needs_its_target_bound() {
    let bound = node(
        "ok",
        "UIDS",
        vec![("target", obj(vec![("value", s("")), ("dbentry", s("#a.b"))]))],
        vec![],
        &[],
        &[],
    );
    let literal = node(
        "bad",
        "UIDS",
        vec![("target", obj(vec![("value", s(""))]))],
        vec![],
        &[],
        &[],
    );
    let files = vec![("page".to_owned(), screen_with(vec![bound, literal], false))];
    let out = screens(&files, &[]).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("page.bad [UIDS]"));
}
