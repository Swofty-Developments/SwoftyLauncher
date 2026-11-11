//! The `hud.element` record (`packages/sdk/std/registries/hud.element/1.0.json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Dial {
    #[serde(rename = "f1_26")]
    F126,
    #[serde(rename = "f1_25")]
    F125,
    #[serde(rename = "f2")]
    F2,
}

impl Dial {
    pub const ALL: [Dial; 3] = [Dial::F126, Dial::F125, Dial::F2];

    pub fn key(self) -> &'static str {
        match self {
            Dial::F126 => "f1_26",
            Dial::F125 => "f1_25",
            Dial::F2 => "f2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockMode {
    Keep,
    Hide,
    Park,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Hud,
    Tower,
}

impl Units {
    pub fn k(self) -> f64 {
        match self {
            Units::Hud => super::geom::K,
            Units::Tower => super::geom::KT,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppu: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ty {
    Bool,
    Text,
    I32,
    Si32,
    F32,
}

/// A value a template node binds: the addon's own leaf, a value of the dial's own view model
/// (`game`), a `#session` path, or one car of the session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bind {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_car: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<Ty>,
    #[serde(default, rename = "not", skip_serializing_if = "std::ops::Not::not")]
    pub negate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl Bind {
    pub fn leaf(name: &str) -> Self {
        Self {
            view_model: Some(name.to_owned()),
            ..Self::default()
        }
    }

    pub fn game(name: &str) -> Self {
        Self {
            game: Some(name.to_owned()),
            ..Self::default()
        }
    }

    pub fn check(&self) -> Result<()> {
        let n = [
            self.view_model.is_some(),
            self.game.is_some(),
            self.session.is_some(),
            self.session_car.is_some(),
            self.screen.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if n != 1 {
            return Err(err(
                "a binding names exactly one of view_model, game, session, session_car and screen",
            ));
        }
        let slots = self.slot.is_some() as u8 + self.slot_var.is_some() as u8;
        if self.session_car.is_some() != (slots == 1) {
            return Err(err("session_car and one of slot and slot_var go together"));
        }
        Ok(())
    }

    pub fn default_bool(&self) -> bool {
        self.default
            .as_ref()
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(self.negate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Sprite,
    Text,
    Group,
    Levels,
    Slats,
    Gauge,
    Sweep,
    Stack,
    Centre,
    Unit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repeat {
    pub count: u32,
    pub var: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub kind: Option<Kind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rot: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Bind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Bind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, rename = "case", skip_serializing_if = "Option::is_none")]
    pub text_case: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Bind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<Bind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprites: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<[[f64; 2]; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_sprite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_sprite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arc: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub game: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub states: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Editor {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preview: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_colour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_stock_in_editor: Option<bool>,
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f64; 4]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockNodes {
    pub template: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
    #[serde(default, rename = "while", skip_serializing_if = "Option::is_none")]
    pub while_on: Option<Bind>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestyleScale {
    pub elements: Vec<String>,
    pub by: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestyleText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restyle {
    pub scene: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fill: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub twins: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_parents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<RestyleScale>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub text: BTreeMap<String, RestyleText>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Also {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<Bind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub dial: Dial,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<Bind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<Element>,
}

/// One `hud.element` record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudElement {
    #[serde(default = "no_dial")]
    pub dial: Dial,
    /// The advanced target: a stock scene (`.idf`) the elements grow, and the template node that
    /// instances it, instead of one of the dials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_ui: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<Bind>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub also: BTreeMap<String, Also>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stock_nodes: Vec<StockNodes>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restyle: Vec<Restyle>,
    #[serde(default, skip_serializing_if = "is_default_units")]
    pub units: Units,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub spectator: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<StockMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stock_parts: Vec<String>,
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sprites: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<Editor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, Variant>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn yes() -> bool {
    true
}

fn no_dial() -> Dial {
    Dial::F126
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_default_units(u: &Units) -> bool {
    *u == Units::Hud
}

impl HudElement {
    pub fn parse(value: &serde_json::Value) -> Result<Self> {
        let mut v = value.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("$comment");
            let targets = ["dial", "scene", "screen"]
                .iter()
                .filter(|k| o.contains_key(**k))
                .count();
            match targets {
                0 => {
                    return Err(err(
                        "record: missing field `dial` (or `scene` and `parent_ui`, or `screen`)",
                    ));
                }
                1 => {}
                _ => {
                    return Err(err(
                        "record: a record has a dial, a scene target or a screen, not two",
                    ));
                }
            }
        }
        let mut rec: Self = serde_json::from_value(v).map_err(|e| err(format!("record: {e}")))?;
        rec.check()?;
        rec.to_hud_units();
        Ok(rec)
    }

    /// Rewrites a `units: tower` record in HUD units, so every later step reads one unit.
    pub fn to_hud_units(&mut self) {
        if self.units == Units::Hud {
            return;
        }
        let f = self.units.k() / super::geom::K;
        self.units = Units::Hud;
        self.rect = self.rect.map(|r| r.map(|v| v * f));
        if let Some(ed) = &mut self.editor {
            ed.rect = ed.rect.map(|r| r.map(|v| v * f));
        }
        for sp in self.sprites.values_mut() {
            sp.bounds = sp.bounds.map(|r| r.map(|v| v * f));
            sp.fit = sp.fit.map(|r| r.map(|v| v * f));
            sp.ppu = sp.ppu.map(|p| p / f);
        }
        for e in &mut self.elements {
            scale_element(e, f);
        }
        for v in self.variants.values_mut() {
            for e in &mut v.add {
                scale_element(e, f);
            }
        }
    }

    /// The scene and template parent of a record that targets a scene instead of a dial.
    pub fn scene_target(&self) -> Option<(&str, &str)> {
        Some((self.scene.as_deref()?, self.parent_ui.as_deref()?))
    }

    fn check_scene_target(&self) -> Result<()> {
        match (&self.scene, &self.parent_ui) {
            (None, None) => return Ok(()),
            (Some(_), Some(_)) => {}
            _ => return Err(err("scene and parent_ui go together")),
        }
        let scene = self.scene.as_deref().unwrap_or_default();
        if !scene.starts_with("eaid://") || !scene.ends_with(".idf") {
            return Err(err(format!("scene {scene} is the eaid:// path of a stock .idf")));
        }
        let parent = self.parent_ui.as_deref().unwrap_or_default();
        if parent.split('.').any(str::is_empty) {
            return Err(err(format!(
                "parent_ui {parent} is a dotted template path, such as hud_mfd_dynamic.mfd.attacher.car_damage_panel"
            )));
        }
        let what = |w: &str| err(format!("a scene target does not take {w}"));
        if !self.sprites.is_empty() {
            return Err(what("sprites"));
        }
        if !self.decisions.is_empty() {
            return Err(what("decisions"));
        }
        if !self.variants.is_empty() {
            return Err(what("variants"));
        }
        if self.stock.is_some() || !self.stock_parts.is_empty() {
            return Err(what("stock or stock_parts"));
        }
        if self.editor.is_some() || self.rect.is_some() {
            return Err(what("editor or box"));
        }
        fn only_text(list: &[Element]) -> Result<()> {
            for e in list {
                if !matches!(
                    e.kind,
                    Some(Kind::Text | Kind::Group | Kind::Stack | Kind::Centre)
                ) {
                    return Err(err(format!(
                        "element {}: a scene target takes text, group, stack and centre elements, not {:?}",
                        e.id, e.kind
                    )));
                }
                if e.layer.is_some() || e.tint.is_some() {
                    return Err(err(format!(
                        "element {}: a scene target does not take layer or tint",
                        e.id
                    )));
                }
                only_text(&e.children)?;
            }
            Ok(())
        }
        only_text(&self.elements)
    }

    pub fn check(&self) -> Result<()> {
        self.check_scene_target()?;
        let mut ids = std::collections::BTreeSet::new();
        for e in &self.elements {
            check_element(e, &mut ids)?;
        }
        for v in self.variants.values() {
            for e in &v.add {
                check_element(e, &mut ids)?;
            }
        }
        let known: std::collections::BTreeSet<&str> = ids.iter().map(String::as_str).collect();
        for d in &self.decisions {
            let ok = matches!(d.kind.as_str(), "svcd" | "mvcd");
            if !ok {
                return Err(err(format!("decision {}: type is svcd or mvcd", d.id)));
            }
            let shown = d.yes.iter().chain(&d.no).chain(d.states.values().flatten());
            for id in shown {
                if !known.contains(id.as_str()) {
                    return Err(err(format!("decision {} shows {id}, which is no element", d.id)));
                }
            }
            if d.kind == "svcd" && d.equals.is_none() {
                return Err(err(format!("decision {}: an svcd needs equals", d.id)));
            }
        }
        Ok(())
    }

    pub fn effective_stock(&self) -> StockMode {
        self.stock.unwrap_or(match self.dial {
            Dial::F126 => StockMode::Park,
            _ => StockMode::Hide,
        })
    }

    pub fn with_variant(&self, name: &str) -> Result<Self> {
        let v = self
            .variants
            .get(name)
            .ok_or_else(|| err(format!("no variant {name}")))?;
        let mut out = self.clone();
        out.dial = v.dial;
        out.variants.clear();
        remove_ids(&mut out.elements, &v.remove);
        out.elements.extend(v.add.iter().cloned());
        out.decisions.iter_mut().for_each(|d| {
            let gone = |id: &String| !v.remove.iter().any(|r| r == id);
            d.yes.retain(gone);
            d.no.retain(gone);
            d.states.values_mut().for_each(|s| s.retain(gone));
        });
        Ok(out)
    }
}

fn scale_element(e: &mut Element, f: f64) {
    let pt = |p: Option<[f64; 2]>| p.map(|p| [p[0] * f, p[1] * f]);
    e.at = pt(e.at);
    e.window = pt(e.window);
    e.pivot = pt(e.pivot);
    e.width = e.width.map(|v| v * f);
    e.cap_height = e.cap_height.map(|v| v * f);
    e.gap = e.gap.map(|v| v * f);
    e.size = e.size.map(|v| v * f);
    e.from = e.from.map(|v| v * f);
    e.to = e.to.map(|v| v * f);
    e.path = e.path.map(|p| p.map(|q| [q[0] * f, q[1] * f]));
    if let Some(r) = &mut e.repeat {
        r.pitch = pt(r.pitch);
    }
    for c in &mut e.children {
        scale_element(c, f);
    }
}

pub const REPEAT_DEPTH: usize = 2;
pub const REPEAT_COPIES: usize = 1024;

/// The elements with every `repeat` group unrolled: copy `i` of group `g` is the element `gNN` at
/// `at + pitch * (i - from)`, its subtree's ids start with `gNN_`, and `{VV}` / `{V}` in the subtree's
/// bindings, literals and `show` patterns read the index. With `rows`, a top-level repeat makes at
/// most that many copies (the editor preview).
pub fn expand(list: &[Element], rows: Option<u32>) -> Result<Vec<Element>> {
    let mut copies = 0usize;
    let mut out = Vec::with_capacity(list.len());
    for e in list {
        expand_into(e, "", &[], 0, rows, &mut copies, &mut out)?;
    }
    Ok(out)
}

fn expand_into(
    e: &Element,
    prefix: &str,
    vars: &[(char, u32)],
    depth: usize,
    rows: Option<u32>,
    copies: &mut usize,
    out: &mut Vec<Element>,
) -> Result<()> {
    let Some(r) = &e.repeat else {
        let mut c = e.clone();
        c.id = format!("{prefix}{}", e.id);
        c.children.clear();
        substitute_element(&mut c, vars)?;
        for ch in &e.children {
            expand_into(ch, prefix, vars, depth, rows, copies, &mut c.children)?;
        }
        out.push(c);
        return Ok(());
    };
    if depth >= REPEAT_DEPTH {
        return Err(err(format!(
            "element {}: repeats nest at most {REPEAT_DEPTH} deep",
            e.id
        )));
    }
    let var = r.var.chars().next().unwrap_or('N');
    if vars.iter().any(|(v, _)| *v == var) {
        return Err(err(format!(
            "element {}: the repeat var {var} is already in use by an enclosing repeat",
            e.id
        )));
    }
    let count = match rows {
        Some(n) if depth == 0 => n.min(r.count),
        _ => r.count,
    };
    let from = r.from.unwrap_or(0);
    let pitch = r.pitch.unwrap_or([0.0, 0.0]);
    let at = e.at.unwrap_or([0.0, 0.0]);
    for k in 0..count {
        *copies += 1;
        if *copies > REPEAT_COPIES {
            return Err(err(format!(
                "element {}: the record's repeats make more than {REPEAT_COPIES} copies",
                e.id
            )));
        }
        let i = from + k;
        let mut vs = vars.to_vec();
        vs.push((var, i));
        let mut c = e.clone();
        c.repeat = None;
        c.id = format!("{prefix}{}{i:02}", e.id);
        c.at = Some([at[0] + pitch[0] * k as f64, at[1] + pitch[1] * k as f64]);
        c.children.clear();
        substitute_element(&mut c, &vs)?;
        let inner = format!("{}_", c.id);
        for ch in &e.children {
            expand_into(ch, &inner, &vs, depth + 1, None, copies, &mut c.children)?;
        }
        out.push(c);
    }
    Ok(())
}

fn substitute_element(e: &mut Element, vars: &[(char, u32)]) -> Result<()> {
    let id = e.id.clone();
    let text = |s: &mut Option<String>| -> Result<()> {
        if let Some(t) = s {
            *t = super::geom::substitute(t, vars);
            if super::geom::has_placeholder(t) {
                return Err(err(format!(
                    "element {id}: {t} names a repeat var that no enclosing repeat declares"
                )));
            }
        }
        Ok(())
    };
    text(&mut e.literal)?;
    text(&mut e.literal_id)?;
    text(&mut e.show)?;
    for b in [&mut e.visible, &mut e.tint, &mut e.value, &mut e.texture]
        .into_iter()
        .flatten()
    {
        text(&mut b.view_model)?;
        text(&mut b.session)?;
        text(&mut b.session_car)?;
        if let Some(sv) = &b.slot_var
            && let Some((_, i)) = vars.iter().find(|(v, _)| sv.starts_with(*v) && sv.len() == 1)
        {
            b.slot = Some(*i);
            b.slot_var = None;
        }
    }
    Ok(())
}

fn remove_ids(list: &mut Vec<Element>, ids: &[String]) {
    list.retain(|e| !ids.contains(&e.id));
    for e in list {
        remove_ids(&mut e.children, ids);
    }
}

fn check_element(e: &Element, ids: &mut std::collections::BTreeSet<String>) -> Result<()> {
    if !ids.insert(e.id.clone()) {
        return Err(err(format!("element id {} is used twice", e.id)));
    }
    let kind = e
        .kind
        .ok_or_else(|| err(format!("element {} has no kind", e.id)))?;
    let need = |what: &str, ok: bool| -> Result<()> {
        if ok {
            Ok(())
        } else {
            Err(err(format!("element {} ({kind:?}) needs {what}", e.id)))
        }
    };
    if let Some(r) = &e.repeat {
        need("kind group for repeat", kind == Kind::Group)?;
        need("repeat.count 1..99", (1..=99).contains(&r.count))?;
        need(
            "repeat.var of one upper-case letter",
            r.var.len() == 1 && r.var.chars().all(|c| c.is_ascii_uppercase()),
        )?;
    }
    match kind {
        Kind::Sprite => need("sprite", e.sprite.is_some())?,
        Kind::Text => {
            need(
                "value, literal or literal_id",
                e.value.is_some() || e.literal.is_some() || e.literal_id.is_some(),
            )?;
            need(
                "one of literal and literal_id",
                !(e.literal.is_some() && e.literal_id.is_some()),
            )?;
        }
        Kind::Group => {}
        Kind::Stack => {
            need(
                "dir down, up, right or left",
                matches!(e.dir.as_deref(), None | Some("down" | "up" | "right" | "left")),
            )?;
            if let Some(c) = e.children.iter().find(|c| c.hidden == Some(true)) {
                return Err(err(format!(
                    "element {}: the stack child {} is hidden; a stacker lays out drawn bounds, so put a hidden part inside a child",
                    e.id, c.id
                )));
            }
        }
        Kind::Centre => {}
        Kind::Levels => {
            need(
                "sprite, count and show",
                e.sprite.is_some() && e.count.is_some() && e.show.is_some(),
            )?;
            need("axis y", e.axis.as_deref().is_none_or(|a| a == "y"))?;
        }
        Kind::Slats => need("sprites and show", !e.sprites.is_empty() && e.show.is_some())?,
        Kind::Gauge => need(
            "sprite, path and value",
            e.sprite.is_some() && e.path.is_some() && e.value.is_some(),
        )?,
        Kind::Sweep => need(
            "mask_sprite, fill_sprite, pivot, arc and value",
            e.mask_sprite.is_some()
                && e.fill_sprite.is_some()
                && e.pivot.is_some()
                && e.arc.len() >= 2
                && e.value.is_some(),
        )?,
        Kind::Unit => need("unit and size", e.unit.is_some() && e.size.is_some())?,
    }
    super::units::check_element(e)?;
    for b in [&e.visible, &e.tint, &e.value].into_iter().flatten() {
        b.check()?;
    }
    for c in &e.children {
        check_element(c, ids)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn halo_like() -> serde_json::Value {
        serde_json::json!({
            "dial": "f1_26",
            "sprites": {
                "base": { "png": "art/base.png", "bounds": [-100, -48, 110, 38] },
                "solid": { "png": "art/solid.png" }
            },
            "elements": [
                { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
                { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 40,
                  "cap_height": 9.5, "at": [0, -14.9], "value": { "game": "speed_str" } }
            ]
        })
    }

    #[test]
    fn parses_a_record_and_defaults_the_stock_mode_by_dial() {
        let r = HudElement::parse(&halo_like()).unwrap();
        assert_eq!(r.dial, Dial::F126);
        assert!(r.spectator);
        assert_eq!(r.effective_stock(), StockMode::Park);
        assert_eq!(r.elements[1].width, Some(40.0));
        let mut f2 = halo_like();
        f2["dial"] = "f2".into();
        assert_eq!(HudElement::parse(&f2).unwrap().effective_stock(), StockMode::Hide);
    }

    #[test]
    fn refuses_broken_records() {
        let mut v = halo_like();
        v["elements"][1]["value"] = serde_json::json!({ "game": "a", "view_model": "view_model_data.b" });
        assert!(
            HudElement::parse(&v)
                .unwrap_err()
                .to_string()
                .contains("exactly one")
        );
        let mut v = halo_like();
        v["elements"][0]["id"] = "speed".into();
        assert!(HudElement::parse(&v).unwrap_err().to_string().contains("twice"));
        let mut v = halo_like();
        v["elements"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({ "id": "g", "kind": "gauge" }));
        assert!(HudElement::parse(&v).unwrap_err().to_string().contains("needs"));
        let mut v = halo_like();
        v["decisions"] = serde_json::json!([{ "id": "d", "type": "svcd", "game": "ers_mode", "equals": 1, "yes": ["nope"] }]);
        assert!(
            HudElement::parse(&v)
                .unwrap_err()
                .to_string()
                .contains("no element")
        );
    }

    pub fn tower_like(rows: u32, cars: u32) -> serde_json::Value {
        let text = r##"{
            "dial": "f1_26",
            "sprites": { "strip": { "png": "art/strip.png", "bounds": [0, 0, 100, 20] }, "bar": { "png": "art/bar.png" } },
            "elements": [
                { "kind": "stack", "id": "stack", "dir": "down", "gap": 0, "children": [
                    { "kind": "group", "id": "top", "visible": { "view_model": "view_model_data.hud_tt_hdr" }, "children": [
                        { "kind": "text", "id": "lap", "style": "OSD_19_regular", "cap_height": 6, "literal_id": "lng_lap" } ] },
                    { "kind": "group", "id": "rows", "at": [0, 5.73], "children": [
                        { "kind": "group", "id": "r", "repeat": { "count": __ROWS__, "var": "N", "pitch": [0, 21.635] },
                          "visible": { "view_model": "view_model_data.hud_tt_r{NN}_on" },
                          "children": [
                            { "kind": "sprite", "id": "s", "sprite": "strip", "layer": 20 },
                            { "kind": "group", "id": "pa", "visible": { "view_model": "view_model_data.hud_tt_r{NN}_up", "not": true, "default": true },
                              "children": [ { "kind": "text", "id": "pos", "style": "OSD_19_regular", "cap_height": 5.9, "literal": "r{N}",
                                              "value": { "view_model": "view_model_data.hud_tt_r{NN}_pos" } } ] },
                            { "kind": "group", "id": "cn", "at": [14.2, 14.6], "children": [
                                { "kind": "group", "id": "v", "repeat": { "count": __CARS__, "var": "V" },
                                  "visible": { "view_model": "view_model_data.hud_tt_r{NN}_c{VV}" },
                                  "children": [
                                    { "kind": "text", "id": "code", "style": "OSD_24_bold", "cap_height": 7.5,
                                      "value": { "session_car": "driver.name_code", "slot_var": "V" } },
                                    { "kind": "sprite", "id": "bar", "sprite": "bar", "layer": 30,
                                      "tint": { "session_car": "nub_colour", "slot_var": "V", "default": "#00000000" } } ] } ] } ] } ] } ] }
            ],
            "editor": { "rows": 5, "preview": { "view_model_data.hud_tt_r00_on": true } }
        }"##;
        serde_json::from_str(
            &text
                .replace("__ROWS__", &rows.to_string())
                .replace("__CARS__", &cars.to_string()),
        )
        .unwrap()
    }

    fn ids(list: &[Element]) -> Vec<&str> {
        list.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn a_repeat_unrolls_with_padded_names_pitch_and_substitution() {
        let r = HudElement::parse(&tower_like(22, 24)).unwrap();
        let out = expand(&r.elements, None).unwrap();
        let rows = &out[0].children[1].children;
        assert_eq!(rows.len(), 22);
        assert_eq!(rows[0].id, "r00");
        assert_eq!(rows[21].id, "r21");
        assert!(rows.iter().all(|r| r.repeat.is_none()));
        assert_eq!(rows[0].at, Some([0.0, 0.0]));
        assert!((rows[5].at.unwrap()[1] - 5.0 * 21.635).abs() < 1e-9);
        assert_eq!(
            rows[3].visible.as_ref().unwrap().view_model.as_deref(),
            Some("view_model_data.hud_tt_r03_on")
        );
        assert_eq!(ids(&rows[3].children), ["r03_s", "r03_pa", "r03_cn"]);
        let pos = &rows[3].children[1].children[0];
        assert_eq!(pos.id, "r03_pos");
        assert_eq!(pos.literal.as_deref(), Some("r3"));
        assert_eq!(
            pos.value.as_ref().unwrap().view_model.as_deref(),
            Some("view_model_data.hud_tt_r03_pos")
        );
        let cars = &rows[3].children[2].children;
        assert_eq!(cars.len(), 24);
        assert_eq!(cars[0].id, "r03_v00");
        assert_eq!(cars[23].id, "r03_v23");
        let v17 = &cars[17];
        assert_eq!(
            v17.visible.as_ref().unwrap().view_model.as_deref(),
            Some("view_model_data.hud_tt_r03_c17")
        );
        assert_eq!(ids(&v17.children), ["r03_v17_code", "r03_v17_bar"]);
        let code = v17.children[0].value.as_ref().unwrap();
        assert_eq!(code.slot, Some(17));
        assert_eq!(code.slot_var, None);
        let bar = v17.children[1].tint.as_ref().unwrap();
        assert_eq!(bar.slot, Some(17));
        let nested: usize = rows.iter().map(|r| r.children[2].children.len()).sum();
        assert_eq!(nested, 528);
        assert_eq!(
            out[0].children[0].children[0].literal_id.as_deref(),
            Some("lng_lap")
        );
    }

    #[test]
    fn the_preview_expands_rows_copies_only() {
        let r = HudElement::parse(&tower_like(22, 24)).unwrap();
        let out = expand(&r.elements, Some(5)).unwrap();
        let rows = &out[0].children[1].children;
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4].children[2].children.len(), 24);
    }

    #[test]
    fn repeats_are_capped_by_depth_and_product() {
        let mut v = tower_like(40, 30);
        assert!(
            expand(&HudElement::parse(&v).unwrap().elements, None)
                .unwrap_err()
                .to_string()
                .contains("more than 1024")
        );
        v = tower_like(2, 2);
        v["elements"][0]["children"][1]["children"][0]["children"][2]["children"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({ "kind": "group", "id": "deep", "repeat": { "count": 2, "var": "D" } }));
        assert!(
            expand(&HudElement::parse(&v).unwrap().elements, None)
                .unwrap_err()
                .to_string()
                .contains("deep")
        );
        let v = tower_like(2, 2);
        let mut r = HudElement::parse(&v).unwrap();
        r.elements[0].children[1].children[0].children[0].visible = Some(Bind::leaf("view_model_data.x_{Q}"));
        assert!(expand(&r.elements, None).unwrap_err().to_string().contains("{Q}"));
    }

    #[test]
    fn a_stack_child_may_not_switch_and_a_repeat_sits_on_a_group() {
        let mut v = tower_like(2, 2);
        v["elements"][0]["children"][1]["hidden"] = true.into();
        let e = HudElement::parse(&v).unwrap_err().to_string();
        assert!(e.contains("stack child rows is hidden"), "{e}");
        let mut v = tower_like(2, 2);
        v["elements"][0]["children"][1]["children"][0]["kind"] = "sprite".into();
        v["elements"][0]["children"][1]["children"][0]["sprite"] = "bar".into();
        let e = HudElement::parse(&v).unwrap_err().to_string();
        assert!(e.contains("kind group for repeat"), "{e}");
    }

    #[test]
    fn tower_units_scale_to_hud_units_once() {
        let mut v = tower_like(2, 2);
        v["units"] = "tower".into();
        let r = HudElement::parse(&v).unwrap();
        assert_eq!(r.units, Units::Hud);
        let f = 0.03 / 0.018;
        let rows = &r.elements[0].children[1];
        assert!((rows.at.unwrap()[1] - 5.73 * f).abs() < 1e-9);
        let rep = rows.children[0].repeat.as_ref().unwrap();
        assert!((rep.pitch.unwrap()[1] - 21.635 * f).abs() < 1e-9);
        assert!((r.sprites["strip"].bounds.unwrap()[2] - 100.0 * f).abs() < 1e-9);
        let plain = HudElement::parse(&tower_like(2, 2)).unwrap();
        assert_eq!(plain.sprites["strip"].bounds.unwrap()[2], 100.0);
    }

    #[test]
    fn a_variant_patches_the_base() {
        let mut v = halo_like();
        v["variants"] = serde_json::json!({ "f2": { "dial": "f2", "remove": ["speed"] } });
        let r = HudElement::parse(&v).unwrap();
        let f2 = r.with_variant("f2").unwrap();
        assert_eq!(f2.dial, Dial::F2);
        assert_eq!(f2.elements.len(), 1);
        assert!(r.with_variant("nope").is_err());
    }
}
