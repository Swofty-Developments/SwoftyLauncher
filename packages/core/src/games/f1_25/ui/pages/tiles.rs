//! The tiles of the home screens: an entry's tile, its text, and the tiles that open the other home pages.

use swofty_formats::pkg::{Node, Val};

use super::home::{HomePage, Slot};
use super::text::{TILE_LINES, clip_lines, fit, fit_to, tagged, title_lines, wrap_text};
use super::{
    Entry, HOME_PAGE_TILES, Icon, Kind, NEXT_PAGE, PREV_PAGE, SETTINGS_ICON, Sources, TAG_STYLE, TileStyle,
};
use crate::Result;
use crate::ui::node::*;

pub(super) fn tile(
    src: &Sources,
    style: &TileStyle,
    parent: &str,
    name: &str,
    row: usize,
    e: &Entry,
) -> Result<Node> {
    let title = tagged(&fit_to(e.upper(), 200), e.kind.tag(), TAG_STYLE);
    let description = fit(e.description.to_owned());
    let text = TileText {
        title: &title,
        by: &fit(e.details()),
        description: &description,
    };
    tile_with(
        src,
        style,
        parent,
        name,
        row,
        (style.icon)(e.kind, e.id, e.track),
        &text,
    )
}

pub(super) struct TileText<'a> {
    pub(super) title: &'a str,
    pub(super) by: &'a str,
    pub(super) description: &'a str,
}

pub(super) fn tile_with(
    src: &Sources,
    style: &TileStyle,
    parent: &str,
    name: &str,
    row: usize,
    icon: Icon,
    text: &TileText,
) -> Result<Node> {
    let mut t = instance(
        src.t("settings_home_category")?,
        parent,
        name,
        &[("title", text.title), ("subtitle", "")],
        true,
    )?;
    t.a_mut()?[LAYOUT] = layout_path(&[(style.slot)(row)]);
    for c in t.slot_mut(CHILDREN)? {
        if c.name() == "icon" {
            let p = c.props_mut()?;
            match icon {
                Icon::Ref(r) => p.set("textureOverride", prop(s(""), Some(r))),
                Icon::Path(path) => p.set("textureOverride", prop(s(path), None)),
                Icon::Texture(name) => p.set(
                    "textureOverride",
                    prop(s(&format!("{SETTINGS_ICON}{name}.tif")), None),
                ),
            }
            p.set("visible", prop(boolv(true), None));
        }
    }
    let (by, description) = match style.wrap {
        Some((by_width, desc_width)) => {
            let by = wrap_text(text.by, by_width);
            let room = TILE_LINES
                .saturating_sub(title_lines(text.title))
                .saturating_sub(by.lines().count())
                .max(1);
            (
                by,
                clip_lines(&wrap_text(text.description, desc_width), room, desc_width),
            )
        }
        None => (text.by.to_owned(), text.description.to_owned()),
    };
    let gap = if style.by_gap && !by.is_empty() && !description.is_empty() {
        "\n"
    } else {
        ""
    };
    let mut info = instance(
        src.t("settings_edit_description_set")?,
        &format!("{parent}.{name}"),
        "info",
        &[("title", &format!("{by}{gap}")), ("body", &description)],
        true,
    )?;
    info.a_mut()?[LAYOUT] = layout_path(style.attach);
    // Like the stock SETTINGS tiles, the details show only on the focused tile.
    set_bool(&mut info, "visible", false)?;
    let info_path = format!("{parent}.{name}.info");
    add_events(
        &mut t,
        vec![
            event("focused", vec![sig(&info_path, "show", "focused")]),
            event("unfocused", vec![sig(&info_path, "hide", "unfocused")]),
        ],
    )?;
    for c in info.slot_mut(CHILDREN)? {
        match c.name() {
            "title" => rename(c, "by")?,
            "body" => rename(c, "desc")?,
            _ => {}
        }
        set_bool(c, "visible", true)?;
    }
    if style.hide_divider {
        info.slot_mut(CHILDREN)?
            .push(hidden("hide_divider", &["divider"]));
    }
    t.slot_mut(CHILDREN)?.push(info);
    Ok(t)
}

pub(super) struct HomeTile {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) row: usize,
    pub(super) col: usize,
    pub(super) writes: Option<String>,
}

/// Drops the first step of the layout of `n`'s children and their children when it is `step`: a node
/// attached to a scene element by its own component names that scene's elements alone.
pub(super) fn drop_first_step(n: &mut Node, step: &str) -> Result<()> {
    let Some(path) = n.a_mut()?[LAYOUT].a_mut()?.first_mut() else {
        return Ok(());
    };
    let steps = path.a_mut()?;
    let first = steps
        .first()
        .and_then(|s| s.as_obj())
        .and_then(|o| o.str_at("debugName").ok());
    if first == Some(step) && steps.len() > 1 {
        steps.remove(0);
    }
    Ok(())
}

/// Gives `n` the component scene at `path`, as a block's o_block or a tile's o_category.
pub(super) fn set_component(n: &mut Node, path: &str) -> Result<()> {
    let path = path.strip_suffix(".idf").unwrap_or(path);
    let name = path.rsplit('/').next().unwrap_or(path);
    n.a_mut()?[COMPONENT] = arr(vec![obj(vec![
        (
            "componentId",
            Node::Val(Val::raw(*b"ui64", component_id(path).to_le_bytes())),
        ),
        ("debugName", s(path)),
        ("name", s(name)),
    ])]);
    Ok(())
}

pub(super) fn page_tile(
    src: &Sources,
    style: &TileStyle,
    parent: &str,
    row: usize,
    flow: &[Entry],
    page: &HomePage,
    slot: Slot,
) -> Result<Node> {
    let (label, to) = match slot {
        Slot::Next => ("NEXT PAGE", page.page + 1),
        _ => ("PREVIOUS PAGE", page.page.saturating_sub(1)),
    };
    let name = if slot == Slot::Next { NEXT_PAGE } else { PREV_PAGE };
    let title = tagged(label, &format!("{}/{}", to + 1, page.pages), TAG_STYLE);
    let on = &flow[(to * HOME_PAGE_TILES).min(flow.len())..((to + 1) * HOME_PAGE_TILES).min(flow.len())];
    let names: Vec<&str> = on.iter().take(3).map(|e| e.name).collect();
    let description = match on.len().saturating_sub(names.len()) {
        0 => names.join(", "),
        more => format!("{} and {more} more", names.join(", ")),
    };
    let text = TileText {
        title: &title,
        by: &format!("Page {} of {}", to + 1, page.pages),
        description: &description,
    };
    tile_with(
        src,
        style,
        parent,
        name,
        row,
        (style.icon)(Kind::Addon, "", false),
        &text,
    )
}
