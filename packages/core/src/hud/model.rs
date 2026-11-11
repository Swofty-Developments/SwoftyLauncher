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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppu: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ty {
    Bool,
    Text,
    I32,
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
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if n != 1 {
            return Err(err(
                "a binding names exactly one of view_model, game, session and session_car",
            ));
        }
        if self.session_car.is_some() != self.slot.is_some() {
            return Err(err("session_car and slot go together"));
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Element>,
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
    pub team_colour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_stock_in_editor: Option<bool>,
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f64; 4]>,
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

impl HudElement {
    pub fn parse(value: &serde_json::Value) -> Result<Self> {
        let mut v = value.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("$comment");
            match (o.contains_key("dial"), o.contains_key("scene")) {
                (true, true) => return Err(err("record: a record has a dial or a scene target, not both")),
                (false, false) => {
                    return Err(err("record: missing field `dial` (or `scene` and `parent_ui`)"));
                }
                _ => {}
            }
        }
        let rec: Self = serde_json::from_value(v).map_err(|e| err(format!("record: {e}")))?;
        rec.check()?;
        Ok(rec)
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
                if !matches!(e.kind, Some(Kind::Text | Kind::Group)) {
                    return Err(err(format!(
                        "element {}: a scene target takes text and group elements, not {:?}",
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
    match kind {
        Kind::Sprite => need("sprite", e.sprite.is_some())?,
        Kind::Text => need("value or literal", e.value.is_some() || e.literal.is_some())?,
        Kind::Group => {}
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
    }
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
