//! Screens, flows and the routes between them: which screens bind a view model, what each flow opens, and
//! whether every route's screens and signals exist, with what a patch kept of the stock signals.

use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::pkg::{Node, Pkg};

use crate::ui::node::*;

pub(super) const SCREEN_CMDS: &[&str] = &["OpenScreen", "CloseScreen", "TransitionScreen"];

pub(super) const LIST_CLASSES: &[&str] = &["UITG", "UITB", "UILT", "UIL2", "UIEL"];

pub(super) fn str_of<'a>(o: &'a swofty_formats::pkg::Obj, key: &str) -> Option<&'a str> {
    o.get(key)?.as_val()?.as_str()
}

pub(super) fn walk(n: &Node, path: &str, out: &mut Vec<String>) {
    let name = n.name();
    let here = if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}.{name}")
    };
    let kind = n.type_code();
    let Ok(props) = n.props() else { return };
    if LIST_CLASSES.contains(&kind.as_str()) {
        let db = props
            .get("data")
            .and_then(Node::as_obj)
            .and_then(|d| str_of(d, "dbentry"));
        match db {
            Some(d) if !d.is_empty() => {}
            _ => out.push(format!(
                "{here} [{kind}]: list controls need data bound to an array (data.dbentry); the game fails to open the screen"
            )),
        }
    }
    if kind == "UIDS" {
        let bound = props
            .get("target")
            .and_then(Node::as_obj)
            .and_then(|d| str_of(d, "dbentry"));
        if bound.is_none_or(str::is_empty) {
            out.push(format!(
                "{here} [{kind}]: a data setter needs its target bound (target.dbentry); stock templates that leave it literal are never instantiated and the game fails the whole screen"
            ));
        }
    }
    if props.get("allowVMBinding").is_some() && !path.is_empty() {
        out.push(format!("{here} [{kind}]: only a screen may set allowVMBinding"));
    }
    if let Ok(lay) = n.slot(LAYOUT) {
        for grp in lay {
            let entries: Vec<&Node> = match grp {
                Node::Arr(a) => a.items.iter().collect(),
                other => vec![other],
            };
            for b in entries.into_iter().filter_map(Node::as_obj) {
                let by_node = b.get("nodeId").is_some();
                let by_comp = b.get("componentId").is_some() && b.get("isComponentId").is_some();
                if !(by_node || by_comp) {
                    out.push(format!(
                        "{here} [{kind}]: a layout entry has neither a nodeId nor a componentId"
                    ));
                }
            }
        }
    }
    if let Ok(kids) = n.slot(CHILDREN) {
        for c in kids.iter().filter(|c| c.is_node()) {
            walk(c, &here, out);
        }
    }
}

pub(super) fn binds_view_model(scr: &Node) -> bool {
    scr.props()
        .ok()
        .and_then(|p| p.get("allowVMBinding"))
        .and_then(Node::as_obj)
        .and_then(|o| o.get("value"))
        .and_then(Node::as_val)
        .is_some_and(|v| matches!(&v.value, swofty_formats::pkg::Value::Raw(b) if b.first() == Some(&1)))
}

pub fn screens(files: &[(String, Vec<u8>)], view_models: &[String]) -> crate::Result<Vec<String>> {
    let mut out = Vec::new();
    for (name, data) in files {
        let p = super::parsed(data)?;
        for scr in p.root.o()?.at("Screens")?.a()? {
            walk(scr, "", &mut out);
            let vm = view_models.contains(name);
            if binds_view_model(scr) && !vm {
                out.push(format!(
                    "{name}: sets allowVMBinding without a view model; the page would stay blank and Back would not work"
                ));
            }
            if vm && !binds_view_model(scr) {
                out.push(format!(
                    "{name}: has a view model but does not set allowVMBinding"
                ));
            }
        }
    }
    Ok(out)
}

pub(super) struct Flow {
    pub(super) erp: Option<String>,
    pub(super) screens: BTreeSet<String>,
}

pub(super) fn flows(data: &[u8]) -> crate::Result<BTreeMap<String, Flow>> {
    let p = super::parsed(data)?;
    let mut out = BTreeMap::new();
    for f in p.root.o()?.at("Flows")?.a()? {
        let erp = f
            .props()?
            .get("asset_group")
            .and_then(Node::as_obj)
            .and_then(|o| str_of(o, "value"))
            .map(|v| {
                let base = v.rsplit('/').next().unwrap_or(v);
                base.rsplit_once('.').map_or(base, |(b, _)| b).to_owned()
            });
        let mut screens = BTreeSet::new();
        for scene in f.slot(CHILDREN)? {
            for e in scene.slot(CHILDREN).map(|v| v.as_slice()).unwrap_or_default() {
                if let Some(s) = e
                    .props()
                    .ok()
                    .and_then(|p| p.get("screen"))
                    .and_then(Node::as_obj)
                    .and_then(|o| str_of(o, "value"))
                {
                    screens.insert(s.to_owned());
                }
            }
        }
        out.insert(f.name().to_owned(), Flow { erp, screens });
    }
    Ok(out)
}

pub(super) fn attrs(n: &Node) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for a in n
        .as_obj()
        .and_then(|o| o.get("Attributes"))
        .and_then(Node::as_arr)
        .map(|a| a.items.as_slice())
        .unwrap_or_default()
    {
        if let Some(o) = a.as_obj()
            && let (Some(k), Some(v)) = (str_of(o, "Id"), str_of(o, "Value"))
        {
            out.insert(k.to_owned(), v.to_owned());
        }
    }
    out
}

pub fn routes(
    routes: &[u8],
    flows_ui: &[u8],
    changed: &[&str],
    registered: &BTreeMap<String, String>,
) -> crate::Result<Vec<String>> {
    route_files(&[routes], flows_ui, changed, registered, &BTreeSet::new())
}

/// The flow whose asset group is the archive `erp` (`flow_frontend`), as flows.ui lists it.
pub fn flow_loading(flows_ui: &[u8], erp: &str) -> crate::Result<Option<String>> {
    Ok(flows(flows_ui)?
        .into_iter()
        .find(|(_, f)| f.erp.as_deref() == Some(erp))
        .map(|(name, _)| name))
}

/// [`routes`] over several route files, which the game reads as one set of route groups. `stock` names game
/// screens that routes may open without a flow entry, as the game's own routes open them.
pub fn route_files(
    routes: &[&[u8]],
    flows_ui: &[u8],
    changed: &[&str],
    registered: &BTreeMap<String, String>,
    stock: &BTreeSet<String>,
) -> crate::Result<Vec<String>> {
    let fl = flows(flows_ui)?;
    let parsed = routes
        .iter()
        .map(|r| super::parsed(r))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut groups: BTreeMap<String, BTreeMap<String, &Node>> = BTreeMap::new();
    for p in &parsed {
        for g in p.root.o()?.at("RouteGroupings")?.a()? {
            let go = g.o()?;
            let gid = str_of(go, "Id").unwrap_or_default().to_owned();
            for r in go.at("Routes")?.a()? {
                let rid = r.o().ok().and_then(|o| str_of(o, "Id")).unwrap_or_default();
                groups.entry(gid.clone()).or_default().insert(rid.to_owned(), r);
            }
        }
    }
    let known = |t: &str| {
        registered.contains_key(t) || stock.contains(t) || fl.values().any(|f| f.screens.contains(t))
    };
    let mut out = Vec::new();
    for (name, flow) in registered {
        let ok = fl
            .values()
            .any(|f| f.erp.as_deref() == Some(flow) && f.screens.contains(name));
        if !ok {
            out.push(format!(
                "screen {name}: no flow that loads {flow} lists it, so routes to it cannot open it"
            ));
        }
    }
    for (gid, rs) in &groups {
        for (rid, r) in rs {
            let id = format!("{gid}.{rid}");
            if !changed.iter().any(|c| *c == gid || *c == id) {
                continue;
            }
            let mut targets = BTreeSet::new();
            if let Some(d) = attrs(r).get("Destination").filter(|d| !d.is_empty()) {
                targets.insert(d.clone());
            }
            for c in r.o()?.at("Commands")?.a()? {
                let kind = c.o().ok().and_then(|o| str_of(o, "Type")).unwrap_or_default();
                let at = attrs(c);
                if kind == "Migrate" {
                    let f = at.get("Flow").map(String::as_str).unwrap_or_default();
                    if !fl.contains_key(f) {
                        out.push(format!("{id}: Migrate to unknown flow {f:?}"));
                    }
                }
                if kind == "TriggerSubRoute" || SCREEN_CMDS.contains(&kind) {
                    for k in ["Target", "To"] {
                        if let Some(t) = at.get(k).filter(|t| !t.is_empty()) {
                            targets.insert(t.clone());
                        }
                    }
                }
                if kind == "TriggerSubRoute" {
                    let t = at.get("Target").map(String::as_str).unwrap_or_default();
                    let sub = at.get("SubRoute").map(String::as_str).unwrap_or_default();
                    if !groups.get(t).is_some_and(|g| g.contains_key(sub)) {
                        out.push(format!("{id}: sub-route {t}.{sub} does not exist"));
                    }
                }
            }
            for t in targets {
                if !known(&t) {
                    out.push(format!(
                        "{id}: target screen {t:?} is not defined in any flow (fatal in game)"
                    ));
                }
            }
        }
    }
    Ok(out)
}

pub(super) type Signal = (String, String, String);

pub(super) fn index<'a>(n: &'a Node, path: &str, out: &mut BTreeMap<String, &'a Node>) {
    let here = if path.is_empty() {
        n.name().to_owned()
    } else {
        format!("{path}.{}", n.name())
    };
    for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
        index(c, &here, out);
    }
    out.insert(here, n);
}

pub(super) fn tree(p: &Pkg) -> crate::Result<BTreeMap<String, &Node>> {
    let mut out = BTreeMap::new();
    for scr in p.root.o()?.at("Screens")?.a()? {
        index(scr, "", &mut out);
    }
    Ok(out)
}

pub(super) fn signals(n: &Node) -> Vec<Signal> {
    let mut out = Vec::new();
    for e in [COMMANDS, EVENTS]
        .into_iter()
        .flat_map(|i| n.slot(i).into_iter().flatten())
    {
        let sigs = e.as_obj().and_then(|o| o.get("signals")).and_then(Node::as_arr);
        for s in sigs.into_iter().flat_map(|a| &a.items).filter_map(Node::as_obj) {
            out.push((
                str_of(s, "to_object_name").unwrap_or_default().to_owned(),
                str_of(s, "to_pin_name").unwrap_or_default().to_owned(),
                s.get("attributes").map(canonical).unwrap_or_default(),
            ));
        }
    }
    out
}

pub(super) fn kids(n: &Node) -> Vec<&str> {
    n.slot(CHILDREN)
        .map(|k| k.iter().filter(|c| c.is_node()).map(|c| c.name()).collect())
        .unwrap_or_default()
}

pub fn stock_kept(stock: &[u8], patched: &[u8], prefix: &str) -> crate::Result<Vec<String>> {
    let (a, b) = (super::parsed(stock)?, super::parsed(patched)?);
    let (old, new) = (tree(&a)?, tree(&b)?);
    let sent: BTreeSet<Signal> = old.values().flat_map(|n| signals(n)).collect();
    let mut out = Vec::new();
    for (path, n) in &old {
        let Some(m) = new.get(path) else {
            out.push(format!("{path}: removed"));
            continue;
        };
        let slot = |x: &Node, i: usize| x.a().ok().and_then(|a| a.get(i)).map(canonical);
        if (0..8)
            .filter(|i| ![CHILDREN, COMMANDS, EVENTS].contains(i))
            .any(|i| slot(n, i) != slot(m, i))
        {
            out.push(format!("{path}: its properties, layout or behaviours changed"));
        }
        let (k0, k1) = (kids(n), kids(m));
        if !k1.starts_with(&k0) || k1[k0.len()..].iter().any(|k| !k.starts_with(prefix)) {
            out.push(format!("{path}: its stock children changed"));
        }
    }
    for (path, m) in &new {
        for sig in signals(m) {
            if old.contains_key(&sig.0) && !sent.contains(&sig) {
                out.push(format!(
                    "{path}: sends {}.{} {}, which no stock node sends",
                    sig.0, sig.1, sig.2
                ));
            }
        }
    }
    Ok(out)
}
