//! The Career list's row scene: o_button_value, the options page's button row, with the stock scene hung under
//! one xref so its background, focus animations and highlights stay the game's, and a text element for each
//! column of the row. The row's own label and value stay in the stock scene, empty.

use swofty_formats::pkg::{Node, Pkg, Val};

use super::idf::{self, elem_name};
use super::node::*;
use super::rows::set_scene_name;
use crate::Result;

pub const SCENE: &str = "eaid://ui_package/art/components/options/options_spawned/swofty_o_career_row.idf";
pub const STOCK: &str = "eaid://ui_package/art/components/options/options_spawned/o_button_value.idf";
pub const BASE: &str = "base";

const HALF_WIDTH: f64 = 5.5;
const Z: f64 = 0.02;
const IDENTITY: [f64; 4] = [-0.0, -0.0, 0.0, -1.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Start,
    End,
}

/// One column of a row: where it sits (its left edge, or its right edge when it ends there) and how it is set.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub name: &'static str,
    pub x: f64,
    pub width: f64,
    pub style: &'static str,
    pub align: Align,
    pub y: f64,
    /// Characters that fit in `width` at `style`, which the row's text is cut to.
    pub chars: usize,
}

pub const DRIVER: Column = Column {
    name: "driver",
    x: 0.3,
    width: 2.5,
    style: "19_regular",
    align: Align::Start,
    y: -0.05,
    chars: 19,
};
pub const TEAM: Column = Column {
    name: "team",
    x: 2.85,
    width: 3.05,
    style: "17_regular",
    align: Align::Start,
    y: -0.045,
    chars: 29,
};
pub const BADGE: Column = Column {
    name: "badge",
    x: 5.95,
    width: 1.8,
    style: "17_regular",
    align: Align::Start,
    y: -0.045,
    chars: 16,
};
pub const PLACE: Column = Column {
    name: "place",
    x: 7.8,
    width: 1.9,
    style: "17_regular",
    align: Align::Start,
    y: -0.045,
    chars: 19,
};
pub const STATUS: Column = Column {
    name: "status",
    x: 10.9,
    width: 1.2,
    style: "17_bold",
    align: Align::End,
    y: -0.045,
    chars: 10,
};

pub const COLUMNS: [Column; 5] = [DRIVER, TEAM, BADGE, PLACE, STATUS];

fn guid(name: &str) -> String {
    let (a, b) = (
        swofty_formats::hash::djb2(format!("swofty.career_row.{name}")),
        swofty_formats::hash::djb2(format!("{name}.swofty.career_row")),
    );
    format!(
        "{a:08x}-{:04x}-4{:03x}-8{:03x}-{b:08x}{:04x}",
        a >> 16,
        b & 0xfff,
        a & 0xfff,
        b >> 16
    )
}

fn text(label: &Node, c: &Column) -> Result<Node> {
    let mut t = idf::detached(label);
    idf::rename(&mut t, c.name)?;
    idf::set_param(&mut t, "guid", Val::str(guid(c.name)))?;
    idf::clear_list(&mut t, "anims")?;
    idf::set_param_like(&mut t, "style", Val::str(c.style))?;
    idf::set_param_like(&mut t, "wrapping", Val::str("none"))?;
    idf::set_param_like(
        &mut t,
        "halign",
        Val::str(match c.align {
            Align::Start => "start",
            Align::End => "right",
        }),
    )?;
    idf::set_param(&mut t, "boxWidth", idf::fp32(c.width))?;
    idf::set_param(&mut t, "eRTL.elementWidth", idf::fp32(c.width))?;
    idf::set_transform(&mut t, [c.x - HALF_WIDTH, c.y, Z], IDENTITY, [1.0, 1.0, 1.0])?;
    idf::recompute_ids(&mut t, c.name)?;
    Ok(t)
}

/// The row scene from the stock `o_button_value` scene's bytes.
pub fn row_scene(stock: &[u8]) -> Result<Vec<u8>> {
    let mut doc = Pkg::parse(stock)?;
    let entry = idf::scenes_mut(&mut doc)?
        .first_mut()
        .ok_or_else(|| err(format!("{STOCK} has no scenes")))?;
    if idf::scene_name(entry) != Some(idf::scene_id(STOCK)) {
        return Err(err(format!("not {STOCK}")));
    }
    set_scene_name(entry, idf::scene_id(SCENE))?;
    let root = entry.o_mut()?.at_mut("scene")?;
    let tl = idf::detached(idf::expect(root, "tl")?);
    let mut br = idf::detached(idf::expect(root, "br")?);
    idf::clear_list(&mut br, "anims")?;
    idf::set_transform(&mut br, [HALF_WIDTH, -0.2, 0.0], IDENTITY, [1.0, 1.0, 1.0])?;
    let mut base = idf::detached(idf::expect(root, "indicator")?);
    idf::rename(&mut base, BASE)?;
    idf::retarget(&mut base, STOCK)?;
    idf::set_param(&mut base, "guid", Val::str(guid(BASE)))?;
    idf::set_transform(&mut base, [0.0, 0.0, 0.0], IDENTITY, [1.0, 1.0, 1.0])?;
    let label = idf::expect(root, "label")?.clone();
    let mut kids = vec![tl, br, base];
    for c in COLUMNS {
        kids.push(text(&label, &c)?);
    }
    for k in &mut kids {
        let name = elem_name(k).to_owned();
        idf::recompute_ids(k, &name)?;
    }
    *idf::children_mut(root)? = kids;
    idf::relookup(entry)?;
    let out = doc.to_bytes()?;
    let back = Pkg::parse(&out)?;
    if back.to_bytes()? != out {
        return Err(err(format!("{SCENE} does not round-trip")));
    }
    let mut bad = idf::xref_mismatches(&back)?;
    bad.extend(idf::id_problems(&back)?);
    if !bad.is_empty() {
        return Err(err(format!("{SCENE}: {}", bad.join("; "))));
    }
    Ok(out)
}

fn lookup(scene: &[u8]) -> Result<std::collections::BTreeSet<u32>> {
    let p = Pkg::parse(scene)?;
    Ok(idf::scenes(&p)?
        .first()
        .ok_or_else(|| err("a scene file without scenes"))?
        .o()?
        .at("nodeidlookup")?
        .a()?
        .iter()
        .filter_map(|v| v.as_val().and_then(Val::as_u32))
        .collect())
}

fn steps(n: &Node) -> Vec<Vec<u32>> {
    n.slot(LAYOUT)
        .map(|groups| {
            groups
                .iter()
                .filter_map(Node::as_arr)
                .map(|g| {
                    g.items
                        .iter()
                        .filter_map(|b| b.as_obj()?.get("nodeId")?.as_val()?.as_u32())
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn below(
    n: &Node,
    scene: &std::collections::BTreeSet<u32>,
    stock: &std::collections::BTreeSet<u32>,
    out: &mut Vec<String>,
    path: &str,
) {
    let base = swofty_formats::hash::djb2(BASE);
    for p in steps(n) {
        let ok = match p.as_slice() {
            [] => true,
            [b, rest @ ..] if *b == base && !rest.is_empty() => rest.iter().all(|s| stock.contains(s)),
            single => single.iter().all(|s| scene.contains(s)),
        };
        if !ok {
            out.push(format!("{path} attaches to an element its row scene lacks"));
        }
    }
    if let Ok(kids) = n.slot(CHILDREN) {
        for c in kids.iter().filter(|c| c.is_node()) {
            below(c, scene, stock, out, &format!("{path}.{}", c.name()));
        }
    }
}

/// What in the list screen's rows (the nodes named `row_*`, below the row node itself) attaches to an element the
/// row scene `scene` lacks: a path through [`BASE`] must name elements of the stock row scene `stock`, any other
/// path elements of `scene`.
pub fn layout_problems(screen_ui: &[u8], scene: &[u8], stock: &[u8]) -> Result<Vec<String>> {
    let (scene, stock) = (lookup(scene)?, lookup(stock)?);
    let p = Pkg::parse(screen_ui)?;
    let screen = p
        .root
        .o()?
        .at("Screens")?
        .a()?
        .first()
        .ok_or_else(|| err("a screen file without screens"))?;
    let mut out = Vec::new();
    for row in screen
        .slot(CHILDREN)?
        .iter()
        .filter(|c| c.is_node() && c.name().starts_with("row_"))
    {
        for c in row.slot(CHILDREN)?.iter().filter(|c| c.is_node()) {
            below(
                c,
                &scene,
                &stock,
                &mut out,
                &format!("{}.{}", row.name(), c.name()),
            );
        }
    }
    Ok(out)
}
