//! A career setup's pages: its rows page under the setup's title, and its home with a CONFIRM tile and a
//! category tile.

use super::options::{literal_tooltips, settings_state_matrix};
use super::page::{build_page, vm_page};
use super::page_rows::addon_rows;
use super::text::label_case;
use super::tiles::{TileText, tile_with};
use super::{Entry, HOME_LAYOUT, Kind, Opt, Sources, TILES};
use crate::Result;
use crate::manifest::CareerSetup;
use crate::ui::node::*;

pub const SETUP_CONFIRM: &str = "confirm";

pub const SETUP_OPTIONS: &str = "open_options";

pub fn setup_title<'a>(e: &Entry<'a>, setup: &'a CareerSetup) -> &'a str {
    setup.title.as_deref().map(str::trim).unwrap_or(e.name)
}

pub fn setup_rows_screen(e: &Entry, setup: &CareerSetup) -> String {
    let full = e.screen.trim_start_matches("swofty_addon_");
    format!("swofty_addon_{}_{full}", setup.route)
}

pub(super) fn setup_entry<'a>(e: &Entry<'a>, setup: &'a CareerSetup) -> Entry<'a> {
    Entry {
        kind: e.kind,
        index: e.index,
        screen: setup_rows_screen(e, setup),
        name: setup_title(e, setup),
        author: e.author,
        version: e.version,
        description: e.description,
        id: e.id,
        track: e.track,
        maps: e.maps.clone(),
        folder: e.folder,
        options: setup
            .options
            .iter()
            .filter_map(|id| e.all.iter().find(|o| &o.o.id == id))
            .map(|o| Opt {
                k: o.k,
                local: o.local,
                o: o.o,
            })
            .collect(),
        all: Vec::new(),
        setup: Some(setup),
    }
}

/// A career setup's rows page: the setup's options, as ADDON SETTINGS shows them, under the setup's title.
pub fn build_setup_page(src: &Sources, e: &Entry, setup: &CareerSetup) -> Result<(String, Vec<u8>)> {
    let e = setup_entry(e, setup);
    if e.options.len() != setup.options.len() {
        return Err(err(format!(
            "career setup {}: {} names options its addon does not have",
            setup.route, e.id
        )));
    }
    let rows = addon_rows(&e);
    let mut scr = build_page(src, &e, &rows, true)?;
    vm_page(&mut scr, &e)?;
    settings_state_matrix(src, &mut scr)?;
    literal_tooltips(&mut scr)?;
    Ok((e.screen.clone(), ui_bytes(vec![scr])?))
}

/// A career setup's home: a CONFIRM tile in the first column and one category tile, which opens the rows page.
pub fn build_setup_home(src: &Sources, e: &Entry, setup: &CareerSetup) -> Result<(String, Vec<u8>)> {
    let home = setup.screen();
    let title = setup_title(e, setup).to_uppercase();
    let scr = src.screen(&home, HOME_LAYOUT)?;
    let fr = src.frame(&home)?;
    let mut kids = vec![src.title(&home, "settings_home_screen", &title)?];
    for n in ["filter", "side_panel", "scroll_bar", "scroll_bar_panel"] {
        kids.push(hidden(&format!("hide_{n}"), &[n]));
    }
    let left = format!("{home}.col_0");
    let right = format!("{home}.col_1");
    let confirm_at = format!("{left}.{SETUP_CONFIRM}");
    let options_at = format!("{right}.options");
    let press = |key: &str| {
        event(
            "pressed",
            vec![
                sig(&format!("{home}.audio_theme_trigger"), "confirm", "pressed"),
                sig_with(
                    &format!("{home}.Action"),
                    "route",
                    "pressed",
                    &[("key", key.into())],
                ),
            ],
        )
    };
    let mut confirm = instance(
        src.t("settings_home_confirm")?,
        &left,
        SETUP_CONFIRM,
        &[
            ("title", "CONFIRM"),
            ("subtitle", ""),
            ("info_title", ""),
            ("info_value", ""),
            ("info_body", ""),
        ],
        true,
    )?;
    confirm.a_mut()?[LAYOUT] = layout_path(&[(TILES.slot)(0)]);
    add_events(
        &mut confirm,
        vec![
            press(SETUP_CONFIRM),
            event("navRight", vec![sig(&options_at, "focus", "navRight")]),
        ],
    )?;
    let labels: Vec<String> = setup_entry(e, setup)
        .options
        .iter()
        .map(|o| label_case(&o.o.label))
        .collect();
    let text = TileText {
        title: &title,
        by: "",
        description: &labels.join(", "),
    };
    let mut category = tile_with(
        src,
        &TILES,
        &right,
        "options",
        0,
        (TILES.icon)(Kind::Addon, e.id, e.track),
        &text,
    )?;
    add_events(
        &mut category,
        vec![
            press(SETUP_OPTIONS),
            event("navLeft", vec![sig(&confirm_at, "focus", "navLeft")]),
        ],
    )?;
    for (name, tile) in [("col_0", confirm), ("col_1", category)] {
        let mut block = instance(src.t("settings_home_block")?, &home, name, &[], true)?;
        block.slot_mut(CHILDREN)?.push(tile);
        kids.push(block);
    }
    let mut scr = finish(scr, fr, kids, "col_1.options")?;
    settings_state_matrix(src, &mut scr)?;
    literal_tooltips(&mut scr)?;
    Ok((home, ui_bytes(vec![scr])?))
}
