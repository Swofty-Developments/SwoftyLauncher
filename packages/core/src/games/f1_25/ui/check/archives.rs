//! The ui files an archive holds, read from its erp, and the checks across them: scene bindings that must
//! resolve to a scene the archives hold, and a report for each changed file.

use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::pkg::Node;

use super::findings::{Finding, Report};
use super::flows::str_of;
use super::stock::{layout_groups, node_scene};
use super::{UiInput, scene_edit, ui_edit};
use crate::ui::catalog::{Catalog, Engine, Scene, Scenes};
use crate::ui::node::*;

#[derive(Debug, Clone, Default)]
pub struct ArchiveUi {
    pub archive: String,
    pub resources: BTreeMap<String, (String, Vec<Vec<u8>>)>,
}

impl ArchiveUi {
    pub fn read(archive: &str, path: &std::path::Path) -> crate::Result<Self> {
        let all = swofty_formats::erp::Erp::read_matching(path, |n| {
            n.ends_with(".ui") || (n.ends_with(".idf") && !n.contains('?'))
        })?;
        let mut resources = BTreeMap::new();
        for r in all
            .into_iter()
            .filter(|r| matches!(r.kind().as_str(), "UIDF" | "UILayout"))
        {
            let frags = r
                .fragments
                .iter()
                .map(|f| f.data())
                .collect::<swofty_formats::Result<Vec<_>>>()?;
            resources.insert(r.name.clone(), (r.kind(), frags));
        }
        Ok(Self {
            archive: archive.to_owned(),
            resources,
        })
    }

    /// The scenes of the layouts this archive holds, each distinct layout parsed once across the archives
    /// `memo` has seen.
    fn scenes<'a>(&'a self, memo: &mut LayoutMemo<'a>) -> Scenes {
        let mut s = Scenes::default();
        for (kind, frags) in self.resources.values() {
            if kind == "UILayout"
                && let Some(f) = frags.first()
            {
                let parsed = memo
                    .entry(f.as_slice())
                    .or_insert_with(|| Scenes::parse(f).unwrap_or_default());
                s.scenes.extend(parsed.iter().cloned());
            }
        }
        s
    }
}

type LayoutMemo<'a> = std::collections::HashMap<&'a [u8], Vec<(u64, std::sync::Arc<Scene>)>>;

pub(super) type Binding = (String, Option<u64>, Vec<u32>, Vec<String>);

pub(super) type CopyOf<'a> = (&'a String, &'a Vec<Vec<u8>>);

pub(super) fn bindings(data: &[u8]) -> crate::Result<Vec<Binding>> {
    fn walk(n: &Node, path: &str, ctx: Option<u64>, out: &mut Vec<Binding>) {
        let here = if path.is_empty() {
            n.name().to_owned()
        } else {
            format!("{path}.{}", n.name())
        };
        let own = node_scene(n);
        if let Some(first) = layout_groups(n).into_iter().next() {
            let ids: Vec<u32> = first
                .iter()
                .filter_map(|b| b.get("nodeId"))
                .filter_map(Node::as_val)
                .filter_map(|v| v.as_u32())
                .collect();
            let names: Vec<String> = first
                .iter()
                .filter_map(|b| str_of(b, "debugName"))
                .map(str::to_owned)
                .collect();
            if !path.is_empty() && !ids.is_empty() && ids.len() == first.len() {
                out.push((here.clone(), ctx, ids, names));
            }
        }
        for c in n.slot(CHILDREN).into_iter().flatten().filter(|c| c.is_node()) {
            walk(c, &here, own.or(ctx), out);
        }
    }
    let p = super::parsed(data)?;
    let mut out = Vec::new();
    for key in ["Screens", "Templates"] {
        for t in p
            .root
            .o()?
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            if t.is_node() {
                walk(t, "", node_scene(t), &mut out);
            }
        }
    }
    Ok(out)
}

pub(super) fn roots_scenes(data: &[u8]) -> crate::Result<Vec<(String, u64)>> {
    let p = super::parsed(data)?;
    let mut out = Vec::new();
    for key in ["Screens", "Templates"] {
        for t in p
            .root
            .o()?
            .get(key)
            .and_then(Node::as_arr)
            .map(|a| a.items.as_slice())
            .unwrap_or_default()
        {
            if let Some(id) = t.is_node().then(|| node_scene(t)).flatten() {
                out.push((t.name().to_owned(), id));
            }
        }
    }
    Ok(out)
}

pub(super) fn in_lookup(scenes: &Scenes, ids: &[u32]) -> Option<u32> {
    ids.iter()
        .copied()
        .find(|id| !scenes.scenes.values().any(|s| s.lookup.binary_search(id).is_ok()))
}

pub fn scene_bindings(
    archive: &str,
    input: &UiInput,
    edited: &Scenes,
    before: &Scenes,
    cat: &Catalog,
) -> crate::Result<Vec<Finding>> {
    let UiInput {
        resource,
        frag,
        data,
        stock,
    } = *input;
    let format = "format: the game resolves each layout step's nodeId through the nodeidlookup of the scene it reaches";
    let mut out = Vec::new();
    let at = |p: &str| format!("{archive}: {resource} {frag}: {p}");
    let stock_roots: BTreeSet<(String, u64)> = match stock {
        Some(s) => roots_scenes(s)?.into_iter().collect(),
        None => BTreeSet::new(),
    };
    for (root, id) in roots_scenes(data)? {
        if stock_roots.contains(&(root.clone(), id)) {
            continue;
        }
        if !edited.scenes.contains_key(&id) && !cat.has_scene(id) {
            out.push(Finding::hard(
                "component-missing",
                &at(&root),
                format!("its scene {id:#018x} is in neither this archive nor any stock archive"),
                format,
            ));
        } else if !edited.scenes.contains_key(&id) {
            out.push(Finding::warn("component-elsewhere", "", &format!("{id:016x}"), &at(&root), format!("its scene {id:#018x} is not in this archive; it loads only if another archive has brought it in")));
        }
    }
    let old = match stock {
        Some(s) => bindings(s)?,
        None => Vec::new(),
    };
    let found: BTreeSet<(String, Vec<u32>)> = old
        .iter()
        .filter(|(_, _, ids, _)| in_lookup(before, ids).is_none())
        .map(|(p, _, ids, _)| (p.clone(), ids.clone()))
        .collect();
    let resolved: BTreeSet<(String, Vec<u32>)> = old
        .iter()
        .filter(|(_, ctx, ids, _)| resolves(before, *ctx, ids))
        .map(|(p, _, ids, _)| (p.clone(), ids.clone()))
        .collect();
    let known: BTreeSet<(String, Vec<u32>)> = old.into_iter().map(|(p, _, ids, _)| (p, ids)).collect();
    for (path, ctx, ids, names) in bindings(data)? {
        let key = (path.clone(), ids.clone());
        if let Some(missing) = in_lookup(edited, &ids) {
            let name = names
                .get(ids.iter().position(|i| *i == missing).unwrap_or(0))
                .cloned()
                .unwrap_or_default();
            if found.contains(&key) {
                out.push(Finding::hard(
                    "layout-node-missing",
                    &at(&path),
                    format!("layout step {name} (nodeId {missing}) was in the stock scenes of this archive and is in no edited scene's nodeidlookup"),
                    format,
                ));
            } else if !known.contains(&key) {
                out.push(Finding::warn(
                    "layout-node-unknown",
                    "",
                    &name,
                    &at(&path),
                    format!("layout step {name} (nodeId {missing}) is in no scene's nodeidlookup in this archive; it binds only if another archive's scene or native code provides it"),
                ));
            }
            continue;
        }
        if resolved.contains(&key) && !resolves(edited, ctx, &ids) {
            out.push(Finding::hard(
                "layout-unresolved",
                &at(&path),
                format!(
                    "layout {} resolved against the stock scenes and does not resolve against the edited ones",
                    names.join(">")
                ),
                format,
            ));
        }
    }
    Ok(out)
}

pub(super) fn resolves(scenes: &Scenes, ctx: Option<u64>, ids: &[u32]) -> bool {
    let Some(sid) = ctx else { return false };
    scenes.resolve_ids(ids, sid)
}

pub fn archives(
    edited: &[ArchiveUi],
    stock: &[ArchiveUi],
    cat: &Catalog,
    engine: &Engine,
) -> crate::Result<Report> {
    let mut report = Report::default();
    let empty = ArchiveUi::default();
    let mut layouts = LayoutMemo::new();
    for a in edited {
        let s = stock.iter().find(|s| s.archive == a.archive).unwrap_or(&empty);
        let changed: Vec<(&String, &String, &Vec<Vec<u8>>)> = a
            .resources
            .iter()
            .filter(|(name, (_, frags))| s.resources.get(*name).is_none_or(|(_, f)| f != frags))
            .map(|(name, (kind, frags))| (name, kind, frags))
            .collect();
        if changed.is_empty() {
            continue;
        }
        let (scenes, before) = (a.scenes(&mut layouts), s.scenes(&mut layouts));
        for (name, kind, frags) in changed {
            let base = s.resources.get(name).map(|(_, f)| f);
            for (i, data) in frags.iter().enumerate() {
                let old = base.and_then(|f| f.get(i)).map(Vec::as_slice);
                if old == Some(data.as_slice()) || !swofty_formats::pkg::is_pkg(data) {
                    continue;
                }
                let frag = format!("{}#{i}", if kind == "UIDF" { "ui" } else { "idf" });
                let mut r = if kind == "UIDF" {
                    let input = UiInput {
                        resource: name,
                        frag: &frag,
                        data,
                        stock: old,
                    };
                    let mut r = ui_edit(&input, &scenes, cat, engine)?;
                    for f in scene_bindings(&a.archive, &input, &scenes, &before, cat)? {
                        r.add(f);
                    }
                    r
                } else {
                    scene_edit(name, data, cat)?
                };
                for f in r.hard.iter_mut().chain(r.warnings.iter_mut()) {
                    if !f.at.starts_with(&a.archive) {
                        f.at = format!("{}: {name}: {}", a.archive, f.at);
                    }
                }
                report.extend(r);
            }
        }
    }
    let mut copies: BTreeMap<&String, Vec<CopyOf>> = BTreeMap::new();
    for a in edited {
        for (name, (_, frags)) in &a.resources {
            copies.entry(name).or_default().push((&a.archive, frags));
        }
    }
    for (name, list) in copies {
        if let Some((a, first)) = list.first()
            && let Some((b, _)) = list.iter().find(|(_, f)| f != first)
        {
            report.add(Finding::warn(
                "copies-differ",
                "",
                name,
                name,
                format!("{a} and {b} carry different copies; the game uses the one from the archive that loads first"),
            ));
        }
    }
    Ok(report)
}

pub fn archive_files(pairs: &[(&str, &std::path::Path, &std::path::Path)]) -> crate::Result<Report> {
    let mut edited = Vec::new();
    let mut stock = Vec::new();
    for (name, before, after) in pairs {
        stock.push(ArchiveUi::read(name, before)?);
        edited.push(ArchiveUi::read(name, after)?);
    }
    archives(
        &edited,
        &stock,
        crate::ui::catalog::stock(),
        crate::ui::catalog::engine(),
    )
}
