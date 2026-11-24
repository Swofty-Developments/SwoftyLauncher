//! `stock_nodes` with `ui` on a yield record: elements of a game screen other than the HUD template
//! hidden while a leaf is on (`docs/reference/ops/hud.md`, "Other game screens").
//!
//! An entry names the screen file, a node of it, the scene that node's layout reaches and the
//! elements to hide, as a screen record's `stock_nodes` do for the HUD template. Each `wrap` element
//! gets a null `<prefix>_w_<element>` around it in the scene, in its place, so the stock node that
//! shows and hides it keeps its binding; the group `<prefix>_sn<i>` under the node binds each `hide`
//! element and each wrapper with `visible` NOT the leaf. An entry that does not fit the screen as the
//! other addons leave it is left out with a build line, and the screen stays as it is.

use std::collections::BTreeSet;

use swofty_formats::erp::Packer;
use swofty_formats::pkg::Pkg;

use super::model::{Bind, StockNodes};
use super::rewrite::Rewrite;
use super::stock::Stock;
use super::{Built, Input, ScreenBase, dials, err, screens, stock_edit};
use crate::Result;
use crate::ui::node;

const UI_PACKAGE: &str = "eaid://ui_package/";

/// The resource name of an entry's `ui`: `screens/persistent.ui` or the full `eaid://` path.
pub fn ui_resource(ui: &str) -> String {
    match ui.starts_with("eaid://") {
        true => ui.to_owned(),
        false => format!("{UI_PACKAGE}{}", ui.trim_start_matches('/')),
    }
}

/// Fails unless the entry names a game screen the composer does not write already, a scene, a
/// template path and something to hide.
pub fn check_entry(s: &StockNodes) -> Result<()> {
    let Some(ui) = s.ui.as_deref() else {
        return Err(err(format!(
            "stock_nodes {}: a yield record's entry names its screen with ui",
            s.template
        )));
    };
    let res = ui_resource(ui);
    if !res.starts_with(UI_PACKAGE) || !res.ends_with(".ui") || res.contains("..") {
        return Err(err(format!(
            "stock_nodes {}: ui {ui} is a screen file of the UI package, such as screens/broadcast_persistent.ui",
            s.template
        )));
    }
    if [dials::TEMPLATE, dials::SPECTATOR, screens::REPLAY_UI].contains(&res.as_str()) {
        return Err(err(format!(
            "stock_nodes {}: the composer writes {ui} itself; hide its parts with a screen record's stock_nodes or a yield part",
            s.template
        )));
    }
    match s.scene.as_deref() {
        Some(sc) if sc.starts_with("eaid://") && sc.ends_with(".idf") => {}
        _ => {
            return Err(err(format!(
                "stock_nodes {}: an entry with ui names the scene its elements are in with scene, the eaid:// path of a stock .idf",
                s.template
            )));
        }
    }
    if s.template.split('.').any(str::is_empty) {
        return Err(err(format!(
            "stock_nodes: template {} is a dotted node path of {ui}",
            s.template
        )));
    }
    if s.hide.is_empty() && s.wrap.is_empty() {
        return Err(err(format!("stock_nodes {}: hides nothing", s.template)));
    }
    if let Some(w) = s.wrap.iter().find(|w| s.hide.contains(w)) {
        return Err(err(format!(
            "stock_nodes {}: {w} is both hidden and wrapped",
            s.template
        )));
    }
    if let Some(b) = &s.while_ {
        b.check()?;
    }
    Ok(())
}

/// The screen entries among `inputs`: the record's prefix, the entry's index and the entry with its
/// leaf (the entry's own `while`, else the yield's).
fn entries<'a>(inputs: &'a [(String, &'a Input)]) -> Vec<(&'a str, usize, StockNodes)> {
    let mut out = Vec::new();
    for (pfx, input) in inputs {
        let Some(y) = &input.record.yields else { continue };
        for (i, s) in input.record.stock_nodes.iter().enumerate() {
            if s.ui.is_none() {
                continue;
            }
            let mut e = s.clone();
            e.while_ = Some(e.while_.clone().unwrap_or_else(|| y.while_.clone()));
            out.push((pfx.as_str(), i, e));
        }
    }
    out.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    out
}

/// The screens and scenes the screen entries among `inputs` write.
pub fn written(inputs: &[Input]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for i in inputs.iter().filter(|i| i.record.yields.is_some()) {
        for s in &i.record.stock_nodes {
            let Some(ui) = &s.ui else { continue };
            out.insert(ui_resource(ui));
            if !s.wrap.is_empty()
                && let Some(sc) = &s.scene
            {
                out.insert(sc.clone());
            }
        }
    }
    out.into_iter().collect()
}

fn leaf(b: &Option<Bind>) -> &str {
    b.as_ref()
        .and_then(|b| b.view_model.as_deref().or(b.path.as_deref()))
        .unwrap_or_default()
}

fn short(r: &str) -> &str {
    r.rsplit('/').next().unwrap_or(r)
}

/// Adds one entry's wrappers and group, or nothing when it does not fit: every step that can fail
/// comes before the writes.
fn one(
    rw: &mut Rewrite,
    stock: &dyn Stock,
    base: ScreenBase,
    pfx: &str,
    i: usize,
    s: &StockNodes,
) -> Result<String> {
    let ui = ui_resource(s.ui.as_deref().unwrap_or_default());
    let scene = s.scene.clone().unwrap_or_default();
    let carriers = stock.carriers(&[ui.as_str(), scene.as_str()])?;
    let erp_of = |r: &str| -> Result<String> {
        carriers
            .get(r)
            .and_then(|v| v.first())
            .cloned()
            .ok_or_else(|| err(format!("no game archive carries {r}")))
    };
    let (ui_erp, scene_erp) = (erp_of(&ui)?, erp_of(&scene)?);
    let mut pkg = Pkg::parse(&rw.read(&ui, &ui_erp, base(&ui))?[0])?;
    node::find_path(&pkg.root, &s.template)?;
    let scene_bytes = rw.read(&scene, &scene_erp, None)?.swap_remove(0);
    let scene_pkg = Pkg::parse(&scene_bytes)?;
    let (wrapped, wraps) = match s.wrap.is_empty() {
        true => (None, Vec::new()),
        false => {
            let (b, names) = stock_edit::wrap_scene(&scene_bytes, s, pfx, None)?;
            (Some(b), names)
        }
    };
    let before = pkg.root.clone();
    let n = stock_edit::attach_stock_nodes(
        &mut pkg.root,
        &before,
        stock_edit::scene_root(&scene_pkg)?,
        s,
        &format!("{pfx}_sn{i}"),
        &wraps,
    )?;
    rw.write(&ui, pkg.to_bytes()?)?;
    if let Some(b) = wrapped {
        rw.write(&scene, b)?;
    }
    Ok(format!(
        "{pfx}: {} {} hides {n} elements of {}{} while {} is on",
        short(&ui),
        s.template,
        short(&scene),
        match wraps.len() {
            0 => String::new(),
            k => format!(" ({k} through a new wrapper null)"),
        },
        leaf(&s.while_)
    ))
}

/// Adds the screen entries' wrappers and groups to `archives`: in place where the composer already
/// writes the resource, else in new archives `hud_screen_nodes.erp` … over every stock archive that
/// carries it. `base` edits a screen before the first entry on it (the launcher's own change to it).
pub fn apply(
    stock: &dyn Stock,
    inputs: &[(String, &Input)],
    base: ScreenBase,
    archives: &mut Vec<Built>,
    lines: &mut Vec<String>,
    packer: &Packer,
) -> Result<()> {
    let list = entries(inputs);
    if list.is_empty() {
        return Ok(());
    }
    let mut rw = Rewrite::new(stock, archives)?;
    for (pfx, i, s) in &list {
        let ui = ui_resource(s.ui.as_deref().unwrap_or_default());
        match one(&mut rw, stock, base, pfx, *i, s) {
            Ok(line) => lines.push(line),
            Err(e) => lines.push(format!(
                "{pfx}: stock_nodes {} {} left out, so the screen stays as it is: {e}",
                short(&ui),
                s.template
            )),
        }
    }
    let added = rw.finish(
        archives,
        "hud_screen_nodes",
        "the yield records' screen nodes",
        lines,
        packer,
    )?;
    archives.extend(added);
    Ok(())
}
