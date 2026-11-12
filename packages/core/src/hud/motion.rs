//! Timed motion on a group: a slide (position keys played as a clip by a `UIXA` node) or a fade
//! (an alpha override by a pair of `UIMO` nodes), started and reversed by an `MVCD` node on a bool
//! leaf of the addon's view model.
//!
//! The group keeps its own null; its children move into a holder null the motion targets, so a
//! slide moves them in the group's local space and HUD customisation's move, resize and opacity
//! still apply. A fade multiplies the holder's alpha (`BlendType` Multiplicative), so the
//! player's opacity setting holds. The editor copy is built without motion.
//!
//! Model: the stock HUD template's `show_mfd_background` / `hide_mfd_background` `UIMO` pair
//! (Alpha, Multiplicative, Cascade) started by `activateOverride`, its `show_penalty_group`
//! `MVCD` on a bool with events `1` and `0`, and the `UIXA` players `goToFraction` and
//! `playToFraction` reach (`playToFraction` names its curve by the djb2 hash of the curve name).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use swofty_formats::animclip::Channel;
use swofty_formats::hash::djb2;
use swofty_formats::pkg::Node;

use super::geom;
use super::model::{Bind, Element, Kind};
use super::scene::Plan;
use super::template::{self, Usage};
use crate::ui::node::{self, Attr, NodeExt};
use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

/// The curve names the stock UI templates play: every `AnimationCurve`, `curveName`, `curve`,
/// `curveOnName` and `curveOffName` value in the game's UI archives. The game knows a curve by the
/// djb2 hash of its name; the curves themselves are the game's own.
pub const STOCK_CURVES: &[&str] = &[
    "broadcast_dotd_close",
    "broadcast_fade_off_track_stats",
    "broadcast_fade_on_track_stats",
    "broadcast_large_fade_off",
    "broadcast_large_fade_on",
    "close_screen_content",
    "close_screen_fade",
    "default_curve_1",
    "fe_pfo_fade_up_curve",
    "full_screen_fade",
    "generic_fade_off",
    "generic_fade_on",
    "generic_fade_on_slow",
    "loading_screen_full_screen_fade",
    "negotiation_slider",
    "open_screen_fade",
    "rwds_xp_sum_bar_diff_change",
    "rwds_xp_sum_bar_fill_change",
    "smart_text_fade",
    "tree_zoom_in",
    "vc_team_bar_slide_in",
];

/// The template properties that name a curve.
pub const CURVE_PROPS: &[&str] = &[
    "AnimationCurve",
    "curveName",
    "curve",
    "curveOnName",
    "curveOffName",
];

pub fn curve_known(name: &str) -> bool {
    STOCK_CURVES.contains(&name)
}

/// Every curve name the nodes under `root` play, by the properties in [`CURVE_PROPS`].
pub fn curves_in(root: &Node) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut paths = Vec::new();
    node::node_paths_of(root, "", &mut paths);
    for (_, n) in paths {
        let Ok(props) = n.props() else { continue };
        for key in CURVE_PROPS {
            let v = props
                .get(key)
                .and_then(|p| p.as_obj().and_then(|o| o.get("value")).or(Some(p)))
                .and_then(Node::as_val)
                .and_then(|v| v.as_str());
            if let Some(v) = v
                && !v.is_empty()
            {
                out.insert(v.to_owned());
            }
        }
    }
    out
}

/// One key: at a time, an alpha or a position offset in HUD units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyValue {
    Alpha(f64),
    Offset([f64; 2]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Slide,
    Fade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub id: String,
    pub keys: Vec<(f64, KeyValue)>,
    pub curve: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_on: Option<Bind>,
}

impl Motion {
    pub fn track(&self) -> Result<Track> {
        let mut track = None;
        for (_, k) in &self.keys {
            let t = match k {
                KeyValue::Alpha(_) => Track::Fade,
                KeyValue::Offset(_) => Track::Slide,
            };
            if track.is_some_and(|have| have != t) {
                return Err(err(format!(
                    "motion {}: its keys mix alpha values and position offsets",
                    self.id
                )));
            }
            track = Some(t);
        }
        track.ok_or_else(|| err(format!("motion {}: no keys", self.id)))
    }

    pub fn check(&self, what: &str) -> Result<()> {
        let ok_id = self.id.len() <= 16
            && self.id.starts_with(|c: char| c.is_ascii_lowercase())
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !ok_id {
            return Err(err(format!(
                "{what}: a motion id is lower case letters, digits and _, 16 at most, not {:?}",
                self.id
            )));
        }
        if self.keys.len() < 2 {
            return Err(err(format!("{what}: motion {} needs two keys or more", self.id)));
        }
        if self.keys[0].0 != 0.0 {
            return Err(err(format!("{what}: motion {} starts at time 0", self.id)));
        }
        for w in self.keys.windows(2) {
            if w[1].0 <= w[0].0 {
                return Err(err(format!(
                    "{what}: motion {}: key times go up, {} does not follow {}",
                    self.id, w[1].0, w[0].0
                )));
            }
        }
        if self.track()? == Track::Fade {
            if self.keys.len() != 2 {
                return Err(err(format!(
                    "{what}: motion {} fades alpha, which takes exactly two keys (from, to); the curve times it",
                    self.id
                )));
            }
            for (_, k) in &self.keys {
                if let KeyValue::Alpha(a) = k
                    && !(0.0..=1.0).contains(a)
                {
                    return Err(err(format!(
                        "{what}: motion {}: alpha {a} is outside 0..1",
                        self.id
                    )));
                }
            }
        }
        if !curve_known(&self.curve) {
            return Err(err(format!(
                "{what}: motion {}: {} is not a curve the stock UI templates play ({})",
                self.id,
                self.curve,
                STOCK_CURVES.join(", ")
            )));
        }
        let on = self.play_on.as_ref().ok_or_else(|| {
            err(format!(
                "{what}: motion {} needs play_on, the bool leaf that starts it",
                self.id
            ))
        })?;
        on.check()?;
        if on.view_model.is_none() || on.negate || on.default.is_some() {
            return Err(err(format!(
                "{what}: motion {}: play_on names a view_model bool leaf, without not or default",
                self.id
            )));
        }
        Ok(())
    }

    /// The fade's alpha from and to.
    pub fn alphas(&self) -> (f64, f64) {
        let a = |k: &KeyValue| match k {
            KeyValue::Alpha(a) => *a,
            KeyValue::Offset(_) => 1.0,
        };
        (
            self.keys.first().map(|(_, k)| a(k)).unwrap_or(1.0),
            self.keys.last().map(|(_, k)| a(k)).unwrap_or(1.0),
        )
    }

    /// The slide's clip channels: the holder's position keys in scene units, with the one-key
    /// rotation and scale channels every clip carries.
    pub fn channels(&self) -> Vec<Channel> {
        let times: Vec<f32> = self.keys.iter().map(|(t, _)| *t as f32).collect();
        let keys: Vec<[f32; 3]> = self
            .keys
            .iter()
            .map(|(_, k)| match k {
                KeyValue::Offset([u, v]) => {
                    let p = geom::scene_point(*u, *v);
                    [p[0] as f32, p[1] as f32, 0.0]
                }
                KeyValue::Alpha(_) => [0.0; 3],
            })
            .collect();
        vec![
            Channel::position(times, keys),
            Channel::rotation(vec![0.0], vec![[0.0, 0.0, 0.0, -1.0]]),
            Channel::scale(vec![0.0], vec![[1.0; 3]]),
        ]
    }
}

/// Checks every motion in `list`: on a group only, never on a stack's child (the stacker lays the
/// child out itself), and well formed.
pub fn check_elements(list: &[Element], parent: Option<Kind>) -> Result<()> {
    for e in list {
        if let Some(m) = &e.motion {
            let what = format!("element {}", e.id);
            if e.kind != Some(Kind::Group) {
                return Err(err(format!(
                    "{what}: a motion goes on a group, not a {:?}",
                    e.kind
                )));
            }
            if parent == Some(Kind::Stack) {
                return Err(err(format!(
                    "{what}: a stack lays its children out itself, so a child of a stack takes no motion; wrap the moving group in a group"
                )));
            }
            m.check(&what)?;
        }
        check_elements(&e.children, e.kind)?;
    }
    Ok(())
}

/// The name of the holder null a motion targets, in the scene's anim string table order
/// (`zh_` sorts after every stock name).
pub fn holder_name(element: &str, m: &Motion) -> String {
    format!("zh_{element}_{}", m.id)
}

/// What one motion put in a scene, for the template.
#[derive(Debug, Clone)]
pub struct MotionItem {
    pub id: String,
    pub mid: String,
    pub element: String,
    pub track: Track,
    pub curve: String,
    pub from: f64,
    pub to: f64,
    pub db: String,
}

impl MotionItem {
    pub fn player(&self) -> String {
        format!("m_{}_{}", self.id, self.mid)
    }

    pub fn trigger(&self) -> String {
        format!("mv_{}_{}", self.id, self.mid)
    }
}

/// Records a live group's motion in its plan: the clip when it slides, the item the template
/// nodes come from, and the leaf's type.
pub fn plan(plan: &mut Plan, e: &Element, m: &Motion, holder: &str) -> Result<()> {
    let track = m.track()?;
    let on = m
        .play_on
        .as_ref()
        .ok_or_else(|| err(format!("motion {}: no play_on", m.id)))?;
    let r = template::resolve(on, Usage::Visible, &format!("element {} motion {}", e.id, m.id))?;
    plan.known.push((r.db.clone(), r.tag));
    if track == Track::Slide {
        plan.clips.push(super::scene::ClipSpec {
            node: holder.to_owned(),
            channels: m.channels(),
        });
    }
    let (from, to) = m.alphas();
    plan.motions.push(MotionItem {
        id: e.id.clone(),
        mid: m.id.clone(),
        element: holder.to_owned(),
        track,
        curve: m.curve.clone(),
        from,
        to,
        db: r.db,
    });
    Ok(())
}

fn alpha_node(name: &str, element: &str, curve: &str, from: f64, to: f64) -> Node {
    node::node(
        name,
        "UIMO",
        vec![
            ("Cascade", node::prop(node::boolv(true), None)),
            ("ParameterName", node::prop(node::s("Alpha"), None)),
            ("BlendType", node::prop(node::s("Multiplicative"), None)),
            ("Value", node::prop(node::f32v(to as f32), None)),
            ("StartingValue", node::prop(node::f32v(from as f32), None)),
            ("AnimationCurve", node::prop(node::s(curve), None)),
        ],
        vec![],
        &[element],
        &["NBEL", "NBMG", "NBOS", "NBXA"],
    )
}

/// The player nodes of a plan's motions: a `UIXA` over the holder for a slide, a `UIMO` pair
/// (`_on` plays from to to, `_off` back) for a fade.
pub fn player_nodes(plan: &Plan) -> Vec<Node> {
    let mut out = Vec::new();
    for m in &plan.motions {
        let name = m.player();
        match m.track {
            Track::Slide => out.push(node::node(
                &name,
                "UIXA",
                vec![],
                vec![],
                &[&m.element],
                node::FRACTION_BEHAVIOURS,
            )),
            Track::Fade => {
                out.push(alpha_node(
                    &format!("{name}_on"),
                    &m.element,
                    &m.curve,
                    m.from,
                    m.to,
                ));
                out.push(alpha_node(
                    &format!("{name}_off"),
                    &m.element,
                    &m.curve,
                    m.to,
                    m.from,
                ));
            }
        }
    }
    out
}

/// The trigger nodes of a plan's motions: an `MVCD` on the leaf whose event `1` starts the motion
/// and `0` reverses it; `base` is the path of the node the players sit under.
pub fn trigger_nodes(plan: &Plan, base: &str) -> Vec<Node> {
    let mut out = Vec::new();
    for m in &plan.motions {
        let player = format!("{base}.{}", m.player());
        let events = match m.track {
            Track::Slide => {
                let to = |ev: &str, value: f32| {
                    node::event(
                        ev,
                        vec![node::sig_with(
                            &player,
                            "playToFraction",
                            ev,
                            &[
                                ("curveName", Attr::U32(djb2(&m.curve))),
                                ("value", Attr::F32(value)),
                            ],
                        )],
                    )
                };
                vec![to("1", 1.0), to("0", 0.0)]
            }
            Track::Fade => {
                let act = |ev: &str, which: &str| {
                    node::event(
                        ev,
                        vec![node::sig_with(
                            &format!("{player}_{which}"),
                            "activateOverride",
                            ev,
                            &[("overrideInstant", Attr::Bool(false))],
                        )],
                    )
                };
                vec![act("1", "on"), act("0", "off")]
            }
        };
        out.push(node::mvcd_node(&m.trigger(), &m.db, events));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use swofty_formats::animclip::{build_clip, read_clip};

    fn clip_template() -> Vec<u8> {
        use swofty_formats::pkg::{Arr, Obj, Pkg, Val};
        let raw = |ty: &[u8; 4], n: usize| Node::Val(Val::raw(*ty, vec![0u8; n]));
        let channel = Node::Obj(Obj::new(vec![
            (Val::str("name_index"), raw(b"si32", 4)),
            (Val::str("mins"), raw(b"ui64", 8)),
            (Val::str("misc"), raw(b"ui64", 8)),
            (Val::str("name"), Node::Val(Val::str("x"))),
            (Val::str("keyptr"), raw(b"ui32", 4)),
            (Val::str("timesptr"), raw(b"ui32", 4)),
        ]));
        let packet = Node::Obj(Obj::new(vec![
            (Val::str("maxtime"), raw(b"fp32", 4)),
            (Val::str("mintime"), raw(b"fp32", 4)),
            (Val::str("interpolator"), raw(b"ui32", 4)),
            (Val::str("channels"), Node::Arr(Arr::new(vec![channel]))),
        ]));
        Pkg {
            name: *b"ANIM",
            root: Node::Obj(Obj::new(vec![
                (Val::str("packets"), Node::Arr(Arr::new(vec![packet]))),
                (Val::str("string_table"), Node::Val(Val::str("t"))),
            ])),
            vca: vec![],
            layout: None,
        }
        .to_bytes()
        .unwrap()
    }

    fn motion(v: serde_json::Value) -> Motion {
        serde_json::from_value(v).unwrap()
    }

    fn slide() -> Motion {
        motion(serde_json::json!({
            "id": "slide", "keys": [[0, [0, 0]], [0.75, [0, -38.94]]], "curve": "generic_fade_on",
            "play_on": { "view_model": "view_model_data.hud_t_r00_swap" }
        }))
    }

    fn fade() -> Motion {
        motion(serde_json::json!({
            "id": "in", "keys": [[0, 0], [0.15, 1]], "curve": "generic_fade_on_slow",
            "play_on": { "view_model": "view_model_data.hud_t_arrow" }
        }))
    }

    #[test]
    fn keys_tell_a_slide_from_a_fade() {
        assert_eq!(slide().track().unwrap(), Track::Slide);
        assert_eq!(fade().track().unwrap(), Track::Fade);
        assert_eq!(fade().alphas(), (0.0, 1.0));
        slide().check("t").unwrap();
        fade().check("t").unwrap();
    }

    #[test]
    fn a_slide_round_trips_through_a_clip() {
        let channels = slide().channels();
        assert_eq!(channels[0].times, vec![0.0, 0.75]);
        assert_eq!(channels[0].max_time, 0.75);
        let dy = (geom::K * 38.94) as f32;
        assert_eq!(channels[0].keys, vec![0.0, 0.0, 0.0, 0.0, dy, 0.0]);
        let template = clip_template();
        let (temp, syst) = build_clip(&template, "zh_t_rows_slide", 9, "tbl", &channels).unwrap();
        let clip = read_clip(&temp, &syst).unwrap();
        assert_eq!(clip.node, "zh_t_rows_slide");
        assert_eq!(clip.channels, channels);
    }

    #[test]
    fn refusals() {
        let bad = |f: fn(&mut serde_json::Value)| {
            let mut v = serde_json::json!({
                "id": "slide", "keys": [[0, [0, 0]], [0.75, [0, -38.94]]], "curve": "generic_fade_on",
                "play_on": { "view_model": "view_model_data.hud_t_x" }
            });
            f(&mut v);
            let m: Motion = serde_json::from_value(v).unwrap();
            m.check("t").unwrap_err().to_string()
        };
        assert!(bad(|v| v["curve"] = "rwds_rival_state_open".into()).contains("not a curve"));
        assert!(bad(|v| v["keys"] = serde_json::json!([[0, [0, 0]]])).contains("two keys"));
        assert!(bad(|v| v["keys"] = serde_json::json!([[0, [0, 0]], [0.5, 1]])).contains("mix"));
        assert!(bad(|v| v["keys"] = serde_json::json!([[0.1, [0, 0]], [0.5, [1, 1]]])).contains("time 0"));
        assert!(bad(|v| v["keys"] = serde_json::json!([[0, [0, 0]], [0, [1, 1]]])).contains("go up"));
        assert!(
            bad(|v| v["keys"] = serde_json::json!([[0, 0], [0.1, 0.5], [0.2, 1]])).contains("exactly two")
        );
        assert!(bad(|v| v["keys"] = serde_json::json!([[0, 0], [0.1, 2]])).contains("outside"));
        assert!(bad(|v| v["id"] = "Slide".into()).contains("motion id"));
        assert!(
            bad(|v| v.as_object_mut().unwrap().remove("play_on").map(|_| ()).unwrap()).contains("play_on")
        );
        assert!(
            bad(|v| v["play_on"] = serde_json::json!({ "game": "drs_pit_state" })).contains("view_model")
        );
        assert!(bad(|v| v["play_on"]["not"] = true.into()).contains("without not"));
    }

    #[test]
    fn a_motion_goes_on_a_group_outside_a_stack() {
        let parse = |v: serde_json::Value| -> Vec<Element> { serde_json::from_value(v).unwrap() };
        let m = serde_json::json!({ "id": "s", "keys": [[0, [0, 0]], [0.5, [0, 4]]], "curve": "generic_fade_on",
            "play_on": { "view_model": "view_model_data.hud_t_go" } });
        let ok = parse(serde_json::json!([{ "id": "g", "kind": "group", "motion": m, "children": [] }]));
        check_elements(&ok, None).unwrap();
        let text = parse(serde_json::json!([{ "id": "t", "kind": "text", "motion": m }]));
        assert!(
            check_elements(&text, None)
                .unwrap_err()
                .to_string()
                .contains("on a group")
        );
        let stacked = parse(serde_json::json!([{ "id": "s", "kind": "stack", "children": [
            { "id": "g", "kind": "group", "motion": m, "children": [] } ] }]));
        assert!(
            check_elements(&stacked, None)
                .unwrap_err()
                .to_string()
                .contains("stack")
        );
        let wrapped = parse(serde_json::json!([{ "id": "s", "kind": "stack", "children": [
            { "id": "w", "kind": "group", "children": [ { "id": "g", "kind": "group", "motion": m, "children": [] } ] } ] }]));
        check_elements(&wrapped, None).unwrap();
    }

    #[test]
    fn players_and_triggers_take_the_stock_shapes() {
        let mut p = Plan::default();
        p.motions.push(MotionItem {
            id: "rows".into(),
            mid: "slide".into(),
            element: "zh_t_rows_slide".into(),
            track: Track::Slide,
            curve: "generic_fade_on".into(),
            from: 1.0,
            to: 1.0,
            db: "#view_model_data.hud_t_swap".into(),
        });
        p.motions.push(MotionItem {
            id: "up".into(),
            mid: "in".into(),
            element: "zh_t_up_in".into(),
            track: Track::Fade,
            curve: "generic_fade_on_slow".into(),
            from: 0.0,
            to: 1.0,
            db: "#view_model_data.hud_t_arrow".into(),
        });
        let players = player_nodes(&p);
        let types: Vec<(String, String)> = players
            .iter()
            .map(|n| (n.name().to_owned(), n.type_code()))
            .collect();
        assert_eq!(
            types,
            [
                ("m_rows_slide".to_owned(), "UIXA".to_owned()),
                ("m_up_in_on".to_owned(), "UIMO".to_owned()),
                ("m_up_in_off".to_owned(), "UIMO".to_owned()),
            ]
        );
        let on = node::canonical(&players[1]);
        assert!(on.contains("Alpha") && on.contains("Multiplicative") && on.contains("generic_fade_on_slow"));
        let triggers = trigger_nodes(&p, "hud.x_ctl");
        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[0].type_code(), "MVCD");
        let t = node::canonical(&triggers[0]);
        assert!(t.contains("hud.x_ctl.m_rows_slide") && t.contains("playToFraction"));
        let curve = triggers[0].slot(node::EVENTS).unwrap()[0]
            .as_obj()
            .and_then(|e| e.get("signals"))
            .and_then(Node::as_arr)
            .and_then(|a| a.items.first())
            .and_then(Node::as_obj)
            .and_then(|sg| sg.get("attributes"))
            .and_then(Node::as_obj)
            .and_then(|at| at.get("curveName"))
            .and_then(Node::as_obj)
            .and_then(|c| c.get("value"))
            .and_then(Node::as_val)
            .and_then(|v| v.as_u32());
        assert_eq!(curve, Some(djb2("generic_fade_on")));
        let f = node::canonical(&triggers[1]);
        assert!(f.contains("hud.x_ctl.m_up_in_on") && f.contains("hud.x_ctl.m_up_in_off"));
        assert!(f.contains("activateOverride"));
    }
}
