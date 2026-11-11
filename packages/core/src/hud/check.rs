//! Assertions of the stock facts the composer relies on, against the game files, and the checks of
//! a composed result (`swofty hud check`). A game update that moves one of them fails here, not in
//! the game.

use std::collections::BTreeSet;

use swofty_formats::hash::djb2;
use swofty_formats::pkg::Pkg;

use super::dials::{self, Use};
use super::editor;
use super::model::Dial;
use super::scene::Builder;
use super::template::StockTags;
use super::{Stock, first};
use crate::ui::idf;
use crate::ui::node::{self, NodeExt};

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub ok: Vec<String>,
    pub problems: Vec<String>,
}

impl Report {
    fn pass(&mut self, line: impl Into<String>) {
        self.ok.push(line.into());
    }

    fn fail(&mut self, line: impl Into<String>) {
        self.problems.push(line.into());
    }

    fn run<T>(&mut self, what: &str, r: crate::Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.fail(format!("{what}: {e}"));
                None
            }
        }
    }

    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

fn identical(stock: &dyn Stock, erps: &[&str], name: &str, r: &mut Report) -> Option<Vec<u8>> {
    let mut found: Option<Vec<u8>> = None;
    for erp in erps {
        let data = match stock.read(erp, name).and_then(|f| first(f, name)) {
            Ok(d) => d,
            Err(e) => {
                r.fail(format!("{name} in {erp}: {e}"));
                return None;
            }
        };
        match &found {
            None => found = Some(data),
            Some(f) if *f != data => {
                r.fail(format!("{name}: {erp} carries a different copy than {}", erps[0]));
                return None;
            }
            _ => {}
        }
    }
    r.pass(format!("{name}: the same in {} archives", erps.len()));
    found
}

fn short(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn check_parent(root: &swofty_formats::pkg::Node, parent: &str, r: &mut Report) {
    let at = match node::find_path(root, parent) {
        Ok(a) => a,
        Err(e) => return r.fail(format!("{parent}: {e}")),
    };
    let Ok(n) = node::node_at(root, &at) else {
        return r.fail(format!("{parent}: gone"));
    };
    let id = n
        .props()
        .ok()
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_val())
        .and_then(|v| v.as_u32());
    if id != Some(djb2(parent)) {
        return r.fail(format!("{parent}: the node's id is not the hash of its path"));
    }
    if n.type_code() != "F1HD" {
        return r.fail(format!("{parent}: {} is not an F1HD", n.type_code()));
    }
    for cmd in ["show", "hide"] {
        let hits = n
            .slot(node::COMMANDS)
            .map(|c| {
                c.iter()
                    .filter(|c| {
                        c.as_obj()
                            .and_then(|o| o.get("commandName"))
                            .and_then(|v| v.as_val())
                            .and_then(|v| v.as_str())
                            == Some(cmd)
                    })
                    .count()
            })
            .unwrap_or(0);
        if hits != 1 {
            r.fail(format!("{parent}: {hits} {cmd} commands, one expected"));
            return;
        }
    }
    r.pass(format!("{parent}: an F1HD with show and hide"));
}

/// Checks the stock facts of every dial against `stock`.
pub fn stock_facts(stock: &dyn Stock) -> Report {
    let mut r = Report::default();
    let read = |name: &str| first(stock.read(dials::STOCK_ERP, name)?, name);
    let mods = r.run("dial_mods.idf", read(dials::MODS));
    let bar = r.run("dial_om_bar_complete.idf", read(dials::BAR));
    if let Some(m) = &mods
        && let Ok(p) = Pkg::parse(m)
        && let Ok(root) = idf::scenes(&p).map(|s| s[0].clone())
    {
        let scene = root.as_obj().and_then(|o| o.get("scene"));
        for n in ["mod_1_icon_fill_mask", "mod_1_icon_fill_mask_anim"] {
            match scene.and_then(|s| idf::find(s, n)) {
                Some(_) => r.pass(format!("dial_mods.idf has {n}")),
                None => r.fail(format!("dial_mods.idf has no {n}")),
            }
        }
    }
    if let Some(b) = &bar
        && let Ok(p) = Pkg::parse(b)
    {
        let has = idf::scenes(&p)
            .ok()
            .and_then(|s| {
                s[0].as_obj()?
                    .get("scene")
                    .map(|s| idf::find(s, "ers_battery_fill_mask").is_some())
            })
            .unwrap_or(false);
        if has {
            r.pass("dial_om_bar_complete.idf has ers_battery_fill_mask");
        } else {
            r.fail("dial_om_bar_complete.idf has no ers_battery_fill_mask");
        }
    }
    let clip = r.run("clip template", read(dials::CLIP_TEMPLATE));
    if let Some(c) = clip {
        match Pkg::parse(&c) {
            Ok(_) => r.pass("the clip template parses"),
            Err(e) => r.fail(format!("the clip template: {e}")),
        }
    }
    if let Some(m) = r.run("fill material", read(dials::FILL_MATERIAL)) {
        match super::textures::reencode_material(&m) {
            Ok(b) if b == m => r.pass("the fill material re-encodes to its own bytes"),
            Ok(_) => r.fail("the fill material does not re-encode to its own bytes"),
            Err(e) => r.fail(format!("the fill material: {e}")),
        }
    }
    let om_xm = r.run("dial_om_xm.idf", read(dials::F126.scene));
    let scene_erps: Vec<&str> = dials::SCENE_ERPS.to_vec();
    for dial in Dial::ALL {
        let f = dials::facts(dial);
        let Some(scene) = identical(stock, &scene_erps, f.scene, &mut r) else {
            continue;
        };
        identical(stock, &scene_erps, f.strtab, &mut r);
        let text_from = (dial != Dial::F126).then(|| om_xm.clone()).flatten();
        if let (Some(m), Some(b)) = (&mods, &bar) {
            let built = Builder::new(f, &scene, m, b, text_from.as_deref());
            if let Some(mut b) = r.run(&format!("{}: scene", short(f.scene)), built) {
                r.pass(format!(
                    "{}: the quad {} has the mesh, extents and pivot the composer expects",
                    short(f.scene),
                    f.quad
                ));
                let parts: Vec<String> = dials::STOCK_KIDS.iter().map(|s| (*s).to_owned()).collect();
                if r.run(
                    &format!("{}: stock parts", short(f.scene)),
                    b.stock_parts_present(&parts),
                )
                .is_some()
                {
                    r.pass(format!(
                        "{}: the stock parts {} are under {}",
                        short(f.scene),
                        parts.join(", "),
                        dials::CUSTOMISATION
                    ));
                }
                match b.bottom() {
                    Ok(y) if (y - f.bottom_y).abs() < 1e-3 => {
                        r.pass(format!("{}: br_customisation sits at y {y}", short(f.scene)))
                    }
                    Ok(y) => r.fail(format!(
                        "{}: br_customisation is at y {y}, not {}",
                        short(f.scene),
                        f.bottom_y
                    )),
                    Err(e) => r.fail(format!("{}: {e}", short(f.scene))),
                }
            }
        }
        if let Some(strtab) = stock
            .read(dials::STOCK_ERP, f.strtab)
            .ok()
            .and_then(|v| first(v, "strtab").ok())
        {
            match swofty_formats::animclip::read_strings(&strtab) {
                Ok(s) if s.iter().any(|n| n.starts_with("zh_")) => r.pass(format!(
                    "{}: the anim string table already carries zh_ names (a built game folder)",
                    short(f.scene)
                )),
                Ok(s) => r.pass(format!(
                    "{}: the anim string table has {} stock names",
                    short(f.scene),
                    s.len()
                )),
                Err(e) => r.fail(format!("{}: {e}", short(f.strtab))),
            }
        }
        if dial == Dial::F125
            && let Some(u) = stock
                .read(dials::STOCK_ERP, f.uipre)
                .ok()
                .and_then(|v| first(v, "uipre").ok())
        {
            match editor::uipre(&u, f.scene, &[dials::STOCK_KIDS[0].to_owned()], &[]) {
                Ok(_) => r.pass("dial_complete.uipre: the stock setters to copy are there"),
                Err(e) => r.fail(format!("dial_complete.uipre: {e}")),
            }
        }
    }
    let tmpl_erps: Vec<&str> = dials::TEMPLATE_ERPS.to_vec();
    let spec_erps: Vec<&str> = dials::SPECTATOR_ERPS.to_vec();
    let template = identical(stock, &tmpl_erps, dials::TEMPLATE, &mut r);
    let spectator = identical(stock, &spec_erps, dials::SPECTATOR, &mut r);
    let race = template.as_deref().and_then(|b| Pkg::parse(b).ok());
    let spec = spectator.as_deref().and_then(|b| Pkg::parse(b).ok());
    if let Some(p) = &race {
        for d in Dial::ALL {
            check_parent(&p.root, dials::facts(d).race_parent, &mut r);
        }
    }
    if let Some(p) = &spec {
        for d in Dial::ALL {
            check_parent(&p.root, dials::facts(d).spectator_parent, &mut r);
        }
    }
    if let Some(p) = &race {
        let tags = StockTags::race(&p.root);
        let mut checked = BTreeSet::new();
        for g in dials::GAME_VALUES.iter().chain(dials::SESSION_VALUES) {
            if g.usage == Use::DecisionOnly && g.name == "drs_pit_state" {
                continue;
            }
            match tags.get(g.db) {
                Some(have) if have.contains(&g.tag) => {
                    checked.insert(g.name);
                }
                Some(have) => r.fail(format!(
                    "{}: the table says {}, the stock template binds {}",
                    g.name,
                    dials::tag_name(&g.tag),
                    have.iter().map(dials::tag_name).collect::<Vec<_>>().join("/")
                )),
                None if g.in_stock => r.fail(format!("{}: no stock node binds {}", g.name, g.db)),
                None => {}
            }
        }
        r.pass(format!(
            "{} game values match the stock template's tags",
            checked.len()
        ));
        for d in [Dial::F125, Dial::F2] {
            let parent = dials::facts(d).race_parent;
            let mut paths = Vec::new();
            node::node_paths_of(&p.root, "", &mut paths);
            let drs = paths
                .iter()
                .filter(|(path, n)| {
                    path.starts_with(&format!("{parent}."))
                        && n.type_code() == "MVCD"
                        && n.props()
                            .ok()
                            .and_then(|p| p.get("dbEntry"))
                            .and_then(|v| v.as_obj())
                            .and_then(|o| o.get("value"))
                            .and_then(|v| v.as_val())
                            .and_then(|v| v.as_str())
                            == Some("drs_pit_state")
                })
                .count();
            if drs == 1 {
                r.pass(format!("{parent}: one stock MVCD on drs_pit_state"));
            } else {
                r.fail(format!(
                    "{parent}: {drs} stock MVCDs on drs_pit_state, one expected"
                ));
            }
        }
    }
    r
}

fn strip(n: &mut swofty_formats::pkg::Node, ours: &BTreeSet<String>) {
    use swofty_formats::pkg::Node;
    if n.is_node() {
        if let Ok(kids) = n.slot_mut(node::CHILDREN) {
            kids.retain(|c| !(c.is_node() && ours.contains(c.name())));
            kids.iter_mut().for_each(|c| strip(c, ours));
        }
        for slot in [node::COMMANDS, node::EVENTS] {
            let Ok(list) = n.slot_mut(slot) else { continue };
            for entry in list.iter_mut() {
                let Some(sigs) = entry
                    .as_obj_mut()
                    .and_then(|o| o.get_mut("signals"))
                    .and_then(Node::as_arr_mut)
                else {
                    continue;
                };
                sigs.items.retain(|s| {
                    let to = s
                        .as_obj()
                        .and_then(|o| o.get("to_object_name"))
                        .and_then(Node::as_val)
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    !ours.contains(to.rsplit('.').next().unwrap_or(to))
                });
            }
        }
        return;
    }
    match n {
        Node::Arr(a) => a.items.iter_mut().for_each(|c| strip(c, ours)),
        Node::Obj(o) => o.items.iter_mut().for_each(|(_, v)| strip(v, ours)),
        _ => {}
    }
}

/// True when `built`, without the groups named in `ours` and the signals that target them, is
/// the stock file node for node and value for value (a file's value tables are not compared: new
/// values interleave with the stock ones).
pub fn stock_kept(stock: &Pkg, built: &Pkg, ours: &BTreeSet<String>) -> crate::Result<bool> {
    let mut b = built.clone();
    strip(&mut b.root, ours);
    let (mut x, mut y) = (Vec::new(), Vec::new());
    node::node_paths_of(&stock.root, "", &mut x);
    node::node_paths_of(&b.root, "", &mut y);
    if x.len() != y.len() {
        return Ok(false);
    }
    let slots = regex::Regex::new(r"slot: (Some\(\([0-9, ]+\)\)|None)").expect("a valid pattern");
    let plain = |n: &swofty_formats::pkg::Node| slots.replace_all(&format!("{n:?}"), "").into_owned();
    Ok(x.iter()
        .zip(&y)
        .all(|((p, a), (q, b))| p == q && plain(a) == plain(b)))
}
