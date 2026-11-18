//! The home screens: which entries each page holds, its rows of tiles, and the focus and scroll a tile press
//! keeps.

use swofty_formats::pkg::Node;

use super::listing::LOOSE_ADDONS;
use super::text::{fit, tagged};
use super::tiles::{HomeTile, drop_first_step, page_tile, set_component, tile};
use super::{
    Entry, HOME, HOME_BLOCK, HOME_LAYOUT, HOME_PAGE_TILES, HOME_ROWS, LIGHT_TAG, NEXT_PAGE, PAGE_FIRST,
    PAGES_ROW, PREV_PAGE, STATE, Sources, TITLE, TileStyle, VISIBLE_COLUMNS,
};
use crate::Result;
use crate::ui::node::*;
use crate::ui::rows;

pub fn home_screen(page: usize) -> String {
    match page {
        0 => HOME.to_owned(),
        p => format!("{HOME}_{}", p + 1),
    }
}

pub fn home_of(index: usize) -> String {
    home_screen(index / HOME_PAGE_TILES)
}

#[derive(Debug, Clone, PartialEq)]
pub struct HomePage {
    pub page: usize,
    pub pages: usize,
    pub screen: String,
    pub tiles: std::ops::Range<usize>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

pub fn home_plan(tiles: usize) -> Vec<HomePage> {
    let pages = tiles.div_ceil(HOME_PAGE_TILES).max(1);
    (0..pages)
        .map(|page| HomePage {
            page,
            pages,
            screen: home_screen(page),
            tiles: page * HOME_PAGE_TILES..((page + 1) * HOME_PAGE_TILES).min(tiles),
            prev: (page > 0).then(|| home_screen(page - 1)),
            next: (page + 1 < pages).then(|| home_screen(page + 1)),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Slot {
    Entry(usize),
    Next,
    Prev,
}

/// A home page's rows: each run of entries of one folder, then the page tiles under More.
pub(super) fn home_rows(flow: &[Entry], page: &HomePage) -> Vec<(String, Vec<Slot>)> {
    let mut rows: Vec<(String, Vec<Slot>)> = Vec::new();
    for i in page.tiles.clone() {
        let folder = flow.get(i).map_or(LOOSE_ADDONS, |e| e.folder);
        match rows.last_mut() {
            Some((name, slots)) if name == folder => slots.push(Slot::Entry(i)),
            _ => rows.push((folder.to_owned(), vec![Slot::Entry(i)])),
        }
    }
    let nav: Vec<Slot> = [
        (page.next.is_some(), Slot::Next),
        (page.prev.is_some(), Slot::Prev),
    ]
    .into_iter()
    .filter_map(|(on, s)| on.then_some(s))
    .collect();
    if !nav.is_empty() {
        rows.push((PAGES_ROW.to_owned(), nav));
    }
    rows
}

/// Signals that scroll `items`, in stack order, so `to` shows with as many before it as `visible`
/// leaves room for: the ones before those hide, the rest that scrolling ever hides show.
pub(super) fn scroll_to(items: &[String], to: usize, visible: usize, from: &str) -> Vec<Node> {
    let hidden = (to + 1).saturating_sub(visible);
    items[..items.len().saturating_sub(visible)]
        .iter()
        .enumerate()
        .map(|(i, p)| sig(p, if i < hidden { "hide" } else { "show" }, from))
        .collect()
}

pub(super) fn home_title(page: &HomePage) -> String {
    match page.pages {
        1 => "ADDON SETTINGS".to_owned(),
        n => tagged(
            "ADDON SETTINGS",
            &format!("PAGE {}/{n}", page.page + 1),
            LIGHT_TAG,
        ),
    }
}

pub(super) fn build_home(src: &Sources, style: &TileStyle, flow: &[Entry], page: &HomePage) -> Result<Node> {
    let home = page.screen.as_str();
    let scr = src.screen(home, HOME_LAYOUT)?;
    let mut fr = src.frame(home)?;
    if page.prev.is_some() {
        for ev in fr.back.slot_mut(EVENTS)? {
            let o = ev.o_mut()?;
            if o.str_at("eventName")? == "pressed" {
                o.at_mut("signals")?.a_mut()?.insert(
                    1,
                    sig(&format!("{home}.remember_{PREV_PAGE}"), "trigger", "pressed"),
                );
            }
        }
    }
    let mut kids = vec![src.title(home, "settings_home_screen", &home_title(page))?];
    for n in ["filter", "side_panel", "scroll_bar", "scroll_bar_panel"] {
        kids.push(hidden(&format!("hide_{n}"), &[n]));
    }
    let rows = home_rows(flow, page);
    let block_path = format!("{home}.{HOME_BLOCK}");
    let mut block = instance(src.t("settings_home_block")?, home, HOME_BLOCK, &[], true)?;
    block.slot_mut(CHILDREN)?.clear();
    let mut row_nodes = Vec::new();
    let mut label_nodes = Vec::new();
    let mut tiles: Vec<Vec<String>> = Vec::new();
    let mut keyed = Vec::new();
    for (r, (label, slots)) in rows.iter().enumerate() {
        let rname = format!("row_{r}");
        let rpath = format!("{block_path}.{rname}");
        let mut row = instance(src.t("settings_home_block")?, &block_path, &rname, &[], true)?;
        row.slot_mut(CHILDREN)?.clear();
        row.a_mut()?[LAYOUT] = layout_path(&[rows::STACKER]);
        set_component(&mut row, rows::O_ROW)?;
        // The folder's name over its row: a copy of the screen title that the block stacks as an
        // item of its own, as the career content select stacks its Built In and Addons labels.
        let lname = format!("label_{r}");
        let mut name = src.title(home, "settings_home_screen", &fit(label.to_uppercase()))?;
        rename_refs(
            &mut name,
            &format!("{home}.screen_title"),
            &format!("{block_path}.{lname}"),
        );
        rename(&mut name, &lname)?;
        name.a_mut()?[LAYOUT] = layout_path(&[rows::STACKER]);
        set_component(&mut name, rows::TITLE_SCENE)?;
        for c in name.slot_mut(CHILDREN)? {
            if c.is_node() {
                drop_first_step(c, TITLE)?;
            }
        }
        label_nodes.push(name);
        let mut paths = Vec::new();
        for (c, slot) in slots.iter().enumerate() {
            let (mut t, tname, route, writes) = match *slot {
                Slot::Entry(i) => {
                    let e = flow.get(i).ok_or_else(|| {
                        err(format!(
                            "{home} lists tile {i}, past the {} there are",
                            flow.len()
                        ))
                    })?;
                    let tname = format!("t{i}");
                    let t = tile(src, style, &rpath, &tname, c, e)?;
                    (
                        t,
                        tname.clone(),
                        format!("open_{}_{}", e.kind.key(), e.index),
                        tname,
                    )
                }
                Slot::Next => (
                    page_tile(src, style, &rpath, c, flow, page, *slot)?,
                    NEXT_PAGE.to_owned(),
                    NEXT_PAGE.to_owned(),
                    PAGE_FIRST.to_owned(),
                ),
                Slot::Prev => (
                    page_tile(src, style, &rpath, c, flow, page, *slot)?,
                    PREV_PAGE.to_owned(),
                    PREV_PAGE.to_owned(),
                    NEXT_PAGE.to_owned(),
                ),
            };
            let path = format!("{rpath}.{tname}");
            add_events(
                &mut t,
                vec![event(
                    "pressed",
                    vec![
                        sig(&format!("{home}.audio_theme_trigger"), "confirm", "pressed"),
                        sig(&format!("{home}.remember_{tname}"), "trigger", "pressed"),
                        sig_with(
                            &format!("{home}.Action"),
                            "route",
                            "pressed",
                            &[("key", route.into())],
                        ),
                    ],
                )],
            )?;
            row.slot_mut(CHILDREN)?.push(t);
            paths.push(path.clone());
            keyed.push(HomeTile {
                path,
                name: tname,
                row: r,
                col: c,
                writes: Some(writes),
            });
        }
        tiles.push(paths);
        row_nodes.push(row);
    }
    let row_paths: Vec<String> = (0..rows.len()).map(|r| format!("{block_path}.row_{r}")).collect();
    let label_paths: Vec<String> = (0..rows.len())
        .map(|r| format!("{block_path}.label_{r}"))
        .collect();
    for (r, row_tiles) in tiles.iter().enumerate() {
        for (c, path) in row_tiles.iter().enumerate() {
            let mut evs = Vec::new();
            if c > 0 {
                let mut sigs = vec![sig(&row_tiles[c - 1], "focus", "navLeft")];
                if row_tiles.len() > VISIBLE_COLUMNS {
                    sigs.push(sig(&row_tiles[c - 1], "show", "navLeft"));
                }
                evs.push(event("navLeft", sigs));
            }
            if c + 1 < row_tiles.len() {
                let mut sigs = vec![sig(&row_tiles[c + 1], "focus", "navRight")];
                if row_tiles.len() > VISIBLE_COLUMNS && c + 1 >= VISIBLE_COLUMNS {
                    sigs.push(sig(&row_tiles[c + 1 - VISIBLE_COLUMNS], "hide", "navRight"));
                }
                evs.push(event("navRight", sigs));
            }
            // Up and down land on the same column, or the row's last tile, with that row
            // scrolled to show it.
            if r > 0 {
                let above = &tiles[r - 1];
                let to = c.min(above.len() - 1);
                let mut sigs = scroll_to(above, to, VISIBLE_COLUMNS, "navUp");
                if rows.len() > HOME_ROWS {
                    sigs.push(sig(&label_paths[r - 1], "show", "navUp"));
                    sigs.push(sig(&row_paths[r - 1], "show", "navUp"));
                }
                sigs.push(sig(&above[to], "focus", "navUp"));
                evs.push(event("navUp", sigs));
            }
            if r + 1 < tiles.len() {
                let below = &tiles[r + 1];
                let to = c.min(below.len() - 1);
                let mut sigs = scroll_to(below, to, VISIBLE_COLUMNS, "navDown");
                if rows.len() > HOME_ROWS && r + 1 >= HOME_ROWS {
                    sigs.push(sig(&label_paths[r + 1 - HOME_ROWS], "hide", "navDown"));
                    sigs.push(sig(&row_paths[r + 1 - HOME_ROWS], "hide", "navDown"));
                }
                sigs.push(sig(&below[to], "focus", "navDown"));
                evs.push(event("navDown", sigs));
            }
            let tname = &path[path.rfind('.').map_or(0, |d| d + 1)..];
            let tile = row_nodes[r]
                .slot_mut(CHILDREN)?
                .iter_mut()
                .find(|x| x.is_node() && x.name() == tname)
                .ok_or_else(|| err(format!("tile {tname} went missing")))?;
            add_events(tile, evs)?;
        }
    }
    for (label, row) in label_nodes.into_iter().zip(row_nodes) {
        block.slot_mut(CHILDREN)?.extend([label, row]);
    }
    kids.push(block);
    let first_path = keyed
        .first()
        .map(|k| k.path.clone())
        .ok_or_else(|| err("ADDON SETTINGS has nothing to list"))?;
    if page.prev.is_some() {
        keyed.push(HomeTile {
            path: first_path.clone(),
            name: PAGE_FIRST.to_owned(),
            row: 0,
            col: 0,
            writes: None,
        });
    }
    let first = first_path[home.len() + 1..].to_owned();
    finish_home(scr, fr, kids, &keyed, &[row_paths, label_paths], &tiles, &first)
}

/// The home screen's tiles are `keyed`, in the rows and labels at `row_paths` whose tiles are `tiles`. A tile
/// press stores the tile's `writes` in the state's `last`, and an open of the screen focuses the tile
/// whose name `last` holds, with the rows and its row scrolled to show it.
pub(super) fn finish_home(
    scr: Node,
    fr: Frame,
    mut kids: Vec<Node>,
    keyed: &[HomeTile],
    row_paths: &[Vec<String>; 2],
    tiles: &[Vec<String>],
    first: &str,
) -> Result<Node> {
    let home = scr.name().to_owned();
    let last = format!("{STATE}.last");
    for k in keyed {
        let Some(writes) = &k.writes else { continue };
        kids.push(node(
            &format!("remember_{}", k.name),
            "UIDS",
            vec![
                ("source", prop(s(writes), None)),
                ("target", prop(s(""), Some(&last))),
            ],
            vec![],
            &[],
            &[],
        ));
    }
    kids.push(node(
        "restore_focus",
        "MVCD",
        vec![("dbEntry", prop(s(&last), None))],
        keyed
            .iter()
            .map(|k| {
                // The screen opens with every row and tile showing: only the rows and the
                // tile's own row scroll.
                let mut sigs: Vec<Node> = row_paths
                    .iter()
                    .flat_map(|paths| scroll_to(paths, k.row, HOME_ROWS, &k.name))
                    .collect();
                sigs.extend(scroll_to(&tiles[k.row], k.col, VISIBLE_COLUMNS, &k.name));
                sigs.push(sig(&k.path, "focus", &k.name));
                event(&k.name, sigs)
            })
            .collect(),
        &[],
        &[],
    ));
    let mut scr = finish(scr, fr, kids, first)?;
    for e in scr.child_mut("Action")?.slot_mut(EVENTS)? {
        let o = e.o_mut()?;
        let nm = o.str_at("eventName")?.to_owned();
        if matches!(nm.as_str(), "opened_screen" | "finishedTranIn") {
            o.at_mut("signals")?
                .a_mut()?
                .push(sig(&format!("{home}.restore_focus"), "query", &nm));
        }
    }
    fix_ids(&mut scr, None);
    Ok(scr)
}
