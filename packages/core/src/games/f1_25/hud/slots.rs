//! `hud.slot` records: an addon's own entry in the game's HUD editor, with its own selection box.
//!
//! The editor lists the components of the `default` layout in `hud_layouts.uidl` and boxes each one by
//! its scene's `tl_customisation` and `br_customisation` points. A slot is one more component there,
//! a scene that holds only those points and the `boundless_customisation` the move and resize apply
//! to, and a template that instances the scene. The game lists it, moves and resizes it and keeps its
//! placement in its own `hud_customisation.xml`. The core reads that placement for the addon's native
//! drawing (`docs/reference/std/hud-slots.md`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use swofty_addon::asset::{AssetOp, Op, Origin, Root, Source};
use swofty_addon::ops::ErpOp;
use swofty_formats::hash::djb2;
use swofty_formats::pkg::{Node, Pkg, Val};

use super::dials;
use crate::ui::idf;
use crate::ui::node::{self, NodeExt, ObjExt};
use crate::{Error, Result};

pub const LAYOUTS: &str = "eaid://ui_package/art/osd/screens/hud_layouts/hud_layouts.uidl";
pub const LAYOUTS_ERP: &str = "2025_asset_groups/ui_package/persistent.erp";
pub const LAYOUT: &str = "default";
/// The stock component a slot's component is cloned from: its filters show it on track and in the pits.
pub const DONOR_COMPONENT: &str = "mfd_complete";
/// The stock scene whose customisation points a slot's scene copies.
pub const DONOR_SCENE: &str = "eaid://ui_package/art/osd/components/dial/dial_complete.idf";
pub const SCENE_DIR: &str = "eaid://ui_package/art/osd/components/swofty/";
pub const SCENE_ERPS: [&str; 3] = [
    "2025_asset_groups/ui_package/flow_race.erp",
    "2025_asset_groups/ui_package/flow_options.erp",
    LAYOUTS_ERP,
];
/// The session layouts of `hud_layouts.uidl`; each holds the `default` components under its own key.
pub const SESSION_LAYOUTS: [&str; 5] = ["race", "qualifying", "practice", "time_trial", "elimination"];
/// The HUD scene's size in units: one unit is 100 px of a 1920x1080 frame.
pub const UNITS: (f64, f64) = (19.2, 10.8);
const POINTS: [&str; 3] = ["boundless_customisation", "tl_customisation", "br_customisation"];

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud.slot: {msg}"))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Centre,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub fn node(self) -> &'static str {
        match self {
            Anchor::TopLeft => "anchor_top_left",
            Anchor::Top => "anchor_top",
            Anchor::TopRight => "anchor_top_right",
            Anchor::Left => "anchor_left",
            Anchor::Centre => "anchor_centre",
            Anchor::Right => "anchor_right",
            Anchor::BottomLeft => "anchor_bottom_left",
            Anchor::Bottom => "anchor_bottom",
            Anchor::BottomRight => "anchor_bottom_right",
        }
    }

    pub fn fraction(self) -> (f64, f64) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Centre => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub name: String,
    pub rect: [f64; 4],
    #[serde(default)]
    pub anchor: Anchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotInput {
    pub addon: String,
    pub id: String,
    pub slot: Slot,
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9') => c,
            _ => '_',
        })
        .collect()
}

/// The component's `dynamicKey`, `nameId` and template name: `swofty_<addon>_<id>`.
pub fn key(addon: &str, id: &str) -> String {
    format!("swofty_{}_{}", clean(addon), clean(id))
}

/// The key of a slot's entry in `hud_customisation.xml` for one session layout.
pub fn element_key(layout: &str, key: &str) -> u32 {
    djb2(format!("{layout}{key}"))
}

pub fn scene_path(key: &str) -> String {
    format!("{SCENE_DIR}{key}.idf")
}

impl Slot {
    pub fn parse(v: &serde_json::Value) -> Result<Self> {
        let s: Slot = serde_json::from_value(v.clone()).map_err(err)?;
        s.check()?;
        Ok(s)
    }

    fn check(&self) -> Result<()> {
        let [x, y, w, h] = self.rect;
        if !(self.rect.iter().all(|v| v.is_finite())
            && w > 0.0
            && h > 0.0
            && x >= 0.0
            && y >= 0.0
            && x + w <= 1.0 + 1e-6
            && y + h <= 1.0 + 1e-6)
        {
            return Err(err(format!(
                "rect {:?} is not [x, y, w, h] inside the screen as fractions",
                self.rect
            )));
        }
        if self.name.trim().is_empty() {
            return Err(err("name is empty"));
        }
        Ok(())
    }

    /// The box's half size in HUD units; the scene's origin is the box centre.
    pub fn half_units(&self) -> (f64, f64) {
        (self.rect[2] * UNITS.0 / 2.0, self.rect[3] * UNITS.1 / 2.0)
    }

    /// The component's position: the box centre from its anchor, in HUD units, `y` up.
    pub fn position(&self) -> (f64, f64) {
        let (ax, ay) = self.anchor.fraction();
        let cx = self.rect[0] + self.rect[2] / 2.0;
        let cy = self.rect[1] + self.rect[3] / 2.0;
        ((cx - ax) * UNITS.0, (ay - cy) * UNITS.1)
    }
}

/// The slots of one `hud.slot` map of an addon's HUD file.
pub fn inputs_of(
    addon: &str,
    file: &str,
    slots: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<SlotInput>> {
    slots
        .iter()
        .map(|(id, v)| {
            let slot = Slot::parse(v).map_err(|e| err(format!("{file}: {id}: {e}")))?;
            Ok(SlotInput {
                addon: addon.to_owned(),
                id: id.clone(),
                slot,
            })
        })
        .collect()
}

fn ui64(v: u64) -> Node {
    Node::Val(Val::raw(*b"ui64", v.to_le_bytes()))
}

fn vec3(x: f64, y: f64) -> Node {
    Node::Val(idf::vec3([x, y, 0.0, 0.0]))
}

/// A slot's scene: the donor scene's root with only its customisation points, the box set to the slot.
pub fn scene(donor: &[u8], s: &SlotInput) -> Result<Vec<u8>> {
    let mut p = Pkg::parse(donor)?;
    let key = key(&s.addon, &s.id);
    let path = scene_path(&key);
    let entry = idf::scenes_mut(&mut p)?
        .first_mut()
        .ok_or_else(|| err("the donor scene has no scene"))?;
    entry.o_mut()?.set("name", ui64(idf::scene_id(&path)));
    let root = entry.o_mut()?.at_mut("scene")?;
    let kids = idf::children_mut(root)?;
    kids.retain(|c| c.as_obj().is_some_and(|_| POINTS.contains(&idf::elem_name(c))));
    if kids.len() != POINTS.len() {
        return Err(err("the donor scene lacks a customisation point"));
    }
    for k in kids.iter_mut() {
        if idf::elem_name(k) == POINTS[0] {
            idf::children_mut(k)?.clear();
            k.o_mut()?.set("position", vec3(0.0, 0.0));
        }
    }
    let (hw, hh) = s.slot.half_units();
    idf::set_box(root, [-hw, hh, 0.0], [hw, -hh, 0.0])?;
    idf::relookup(entry)?;
    Ok(p.to_bytes()?)
}

/// The template group that instances a slot's scene.
fn template_node(key: &str) -> Node {
    let mut n = node::group_node(
        key,
        vec![("allowVMBinding", node::obj(vec![("value", node::boolv(true))]))],
        &[],
        vec![],
    );
    let path = scene_path(key);
    let base = path.strip_suffix(".idf").unwrap_or(&path);
    if let Ok(slot) = n.slot_mut(node::COMPONENT) {
        slot.push(node::obj(vec![
            ("componentId", ui64(node::component_id(base))),
            ("debugName", node::s(base)),
            ("name", node::s(key)),
        ]));
    }
    node::fix_ids(&mut n, None);
    n
}

/// `hud_dynamic_templates.ui` with a template per slot.
pub fn templates(data: &[u8], slots: &[SlotInput]) -> Result<Vec<u8>> {
    let mut p = Pkg::parse(data)?;
    let list = p.root.o_mut()?.at_mut("Templates")?.a_mut()?;
    for s in slots {
        let key = key(&s.addon, &s.id);
        if list.iter().any(|t| t.is_node() && t.name() == key) {
            return Err(err(format!("the templates already have {key}")));
        }
        list.push(template_node(&key));
    }
    Ok(p.to_bytes()?)
}

fn str_of<'a>(o: &'a swofty_formats::pkg::Obj, k: &str) -> Option<&'a str> {
    o.get(k)?.as_val()?.as_str()
}

/// `hud_layouts.uidl` with a component per slot in the `default` layout.
pub fn layouts(data: &[u8], slots: &[SlotInput]) -> Result<Vec<u8>> {
    let mut p = Pkg::parse(data)?;
    let layouts = p.root.o_mut()?.at_mut("layouts")?.a_mut()?;
    let default = layouts
        .iter_mut()
        .find(|l| l.as_obj().and_then(|o| str_of(o, "id")) == Some(LAYOUT))
        .ok_or_else(|| err(format!("hud_layouts has no {LAYOUT} layout")))?;
    let comps = default.o_mut()?.at_mut("components")?.a_mut()?;
    let donor = comps
        .iter()
        .find(|c| c.as_obj().and_then(|o| str_of(o, "nameId")) == Some(DONOR_COMPONENT))
        .cloned()
        .ok_or_else(|| err(format!("the {LAYOUT} layout has no {DONOR_COMPONENT}")))?;
    for s in slots {
        let key = key(&s.addon, &s.id);
        if comps
            .iter()
            .any(|c| c.as_obj().and_then(|o| str_of(o, "nameId")) == Some(key.as_str()))
        {
            return Err(err(format!("the {LAYOUT} layout already has {key}")));
        }
        let path = scene_path(&key);
        let base = path.strip_suffix(".idf").unwrap_or(&path).to_owned();
        let (x, y) = s.slot.position();
        let mut c = donor.clone();
        let o = c.o_mut()?;
        for (k, v) in [
            ("target", s.slot.anchor.node()),
            ("eDynamicLayout.dynamicTemplate", key.as_str()),
            ("eDynamicLayout.dynamicKey", key.as_str()),
            ("nameId", key.as_str()),
        ] {
            o.at_mut(k)?.v_mut()?.set_str(v);
        }
        o.set("xref", ui64(node::component_id(&base)));
        o.set("position", vec3(x, y));
        comps.push(c);
    }
    Ok(p.to_bytes()?)
}

fn read(path: &Path, resource: &str) -> Result<Vec<u8>> {
    let file = swofty_formats::erp::ErpFile::open(path)?;
    let res = file.erp.find(resource)?;
    let loaded = file.load(res)?;
    let frag = loaded
        .fragments
        .first()
        .ok_or_else(|| err(format!("{resource} has no fragment")))?;
    Ok(frag.data()?.to_vec())
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| Error::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    let p = dir.join(name);
    std::fs::write(&p, bytes).map_err(|e| Error::Io { path: p, source: e })
}

/// The cache folder, under each owning addon's cache root, that slot resources are written to.
pub const CACHE: &str = "hud_slots";

/// The ERP ops that put every slot in the game: the scenes, the templates and the layouts, read from
/// `base` (the archive each op starts from) and written under `cache(addon)/hud_slots`.
pub fn ops(
    slots: &[SlotInput],
    base: &dyn Fn(&str) -> std::path::PathBuf,
    cache: &dyn Fn(&str) -> std::path::PathBuf,
) -> Result<(Vec<Op>, Vec<String>)> {
    let mut ops = Vec::new();
    let mut lines = Vec::new();
    if slots.is_empty() {
        return Ok((ops, lines));
    }
    let owner = slots[0].addon.clone();
    let out = cache(&owner).join(CACHE);
    let origin = Origin {
        addon: owner.clone(),
        item: Some("HUD slots".to_owned()),
    };
    let src = |name: &str| Source {
        root: Root::Cache,
        path: format!("{CACHE}/{name}"),
    };
    let donor = read(&base(dials::STOCK_ERP), DONOR_SCENE)?;
    for s in slots {
        let key = key(&s.addon, &s.id);
        let file = format!("{key}.idf");
        write(&out, &file, &scene(&donor, s)?)?;
        for erp in SCENE_ERPS {
            ops.push(Op {
                op: AssetOp::Erp(ErpOp::AddResource {
                    erp: erp.to_owned(),
                    resource: scene_path(&key),
                    kind: "UILayout".to_owned(),
                    sources: vec![src(&file)],
                }),
                origin: origin.clone(),
            });
        }
        lines.push(format!(
            "{} of {}: {:?} at {:?} from {}, keys {}",
            key,
            s.addon,
            s.slot.name,
            s.slot.rect,
            s.slot.anchor.node(),
            SESSION_LAYOUTS
                .iter()
                .map(|l| format!("{l} {}", element_key(l, &key)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for erp in dials::TEMPLATE_ERPS {
        let file = format!("templates.{}.ui", erp.rsplit('/').next().unwrap_or(erp));
        write(
            &out,
            &file,
            &templates(&read(&base(erp), dials::TEMPLATE)?, slots)?,
        )?;
        ops.push(Op {
            op: AssetOp::Erp(ErpOp::ReplaceResource {
                erp: erp.to_owned(),
                resource: dials::TEMPLATE.to_owned(),
                fragment: 0,
                source: src(&file),
            }),
            origin: origin.clone(),
        });
    }
    write(
        &out,
        "hud_layouts.uidl",
        &layouts(&read(&base(LAYOUTS_ERP), LAYOUTS)?, slots)?,
    )?;
    ops.push(Op {
        op: AssetOp::Erp(ErpOp::ReplaceResource {
            erp: LAYOUTS_ERP.to_owned(),
            resource: LAYOUTS.to_owned(),
            fragment: 0,
            source: src("hud_layouts.uidl"),
        }),
        origin,
    });
    Ok((ops, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(rect: [f64; 4], anchor: Anchor) -> Slot {
        Slot {
            name: "Team radio".into(),
            rect,
            anchor,
        }
    }

    #[test]
    fn keys_follow_the_games_element_hash() {
        assert_eq!(element_key("race", "dial_f1"), 647_083_312);
        assert_eq!(element_key("qualifying", "minimap"), 1_516_591_561);
        assert_eq!(key("acme.radio", "Card-1"), "swofty_acme_radio_card_1");
    }

    #[test]
    fn the_position_is_the_box_centre_from_the_anchor() {
        let s = slot([0.0, 0.0, 0.5, 0.5], Anchor::Centre);
        let (x, y) = s.position();
        assert!((x + 4.8).abs() < 1e-9 && (y - 2.7).abs() < 1e-9);
        let s = slot([0.75, 0.5, 0.25, 0.5], Anchor::BottomRight);
        let (x, y) = s.position();
        assert!((x + 2.4).abs() < 1e-9 && (y - 2.7).abs() < 1e-9);
        assert_eq!(s.half_units(), (2.4, 2.7));
    }

    #[test]
    fn a_rect_off_screen_is_refused() {
        let v = serde_json::json!({ "name": "x", "rect": [0.9, 0.1, 0.2, 0.1] });
        assert!(Slot::parse(&v).is_err());
        let v = serde_json::json!({ "name": "x", "rect": [0.1, 0.1, 0.2, 0.1], "anchor": "top_left" });
        assert_eq!(Slot::parse(&v).unwrap().anchor, Anchor::TopLeft);
    }

    fn layouts_pkg(extra: &[&str]) -> Pkg {
        use crate::ui::node::{arr, obj, s};
        let comp = |name: &str| {
            obj(vec![
                ("target", s("anchor_centre")),
                ("eDynamicLayout.dynamicTemplate", s(name)),
                ("eDynamicLayout.dynamicKey", s(name)),
                ("nameId", s(name)),
                ("xref", ui64(1)),
                ("position", vec3(0.0, 0.0)),
            ])
        };
        let mut names = vec![DONOR_COMPONENT, "lap_times"];
        names.extend_from_slice(extra);
        Pkg {
            name: [0; 4],
            root: obj(vec![
                ("id", s("layouts")),
                (
                    "layouts",
                    arr(vec![
                        obj(vec![
                            ("id", s("race")),
                            ("components", arr(vec![comp("race_objective")])),
                        ]),
                        obj(vec![
                            ("id", s(LAYOUT)),
                            ("components", arr(names.into_iter().map(comp).collect())),
                        ]),
                    ]),
                ),
            ]),
            vca: Vec::new(),
            layout: None,
        }
    }

    fn default_names(p: &Pkg) -> Vec<String> {
        let layouts = p.root.as_obj().unwrap().get("layouts").unwrap().as_arr().unwrap();
        let default = layouts
            .items
            .iter()
            .find(|l| l.as_obj().and_then(|o| str_of(o, "id")) == Some(LAYOUT))
            .unwrap();
        default
            .as_obj()
            .unwrap()
            .get("components")
            .unwrap()
            .as_arr()
            .unwrap()
            .items
            .iter()
            .map(|c| str_of(c.as_obj().unwrap(), "nameId").unwrap().to_owned())
            .collect()
    }

    #[test]
    fn a_later_addons_whole_layouts_keep_the_slot_components() {
        let base = layouts_pkg(&[]);
        let slots = [SlotInput {
            addon: "acme.radio".into(),
            id: "card".into(),
            slot: slot([0.1, 0.1, 0.2, 0.1], Anchor::Top),
        }];
        let ours = Pkg::parse(&layouts(&base.to_bytes().unwrap(), &slots).unwrap()).unwrap();
        let theirs = layouts_pkg(&["spool_gauge"]);
        let merged = crate::ui::merge::merge(crate::ui::merge::Kind::Layouts, &base, &ours, &theirs).unwrap();
        assert!(merged.conflicts.is_empty(), "{:?}", merged.conflicts);
        let names = default_names(&merged.pkg);
        assert!(names.contains(&"swofty_acme_radio_card".to_owned()), "{names:?}");
        assert!(names.contains(&"spool_gauge".to_owned()), "{names:?}");
    }
}
