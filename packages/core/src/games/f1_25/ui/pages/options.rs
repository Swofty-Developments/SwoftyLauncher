//! An addon's options page under another screen name, with its settings state and its tooltips written out.

use swofty_formats::pkg::{Node, Val, Value};

use super::page::{build_page, retitle, vm_page};
use super::page_rows::addon_rows;
use super::{Entry, Opt, Sources};
use crate::Result;
use crate::ui::node::*;

pub(super) fn settings_state_matrix(src: &Sources, scr: &mut Node) -> Result<()> {
    let want: Value = src
        .container
        .props()?
        .at("animData")?
        .o()?
        .at("stateMatrix")?
        .o()?
        .at("value")?
        .v()?
        .value
        .clone();
    scr.props_mut()?
        .at_mut("animData")?
        .o_mut()?
        .at_mut("stateMatrix")?
        .o_mut()?
        .at_mut("value")?
        .v_mut()?
        .value = want;
    Ok(())
}

pub(super) fn literal_tooltips(scr: &mut Node) -> Result<()> {
    for t in scr.child_mut("tooltips")?.slot_mut(CHILDREN)? {
        if t.name() == "adjust" {
            set_prop_value(t.child_mut("text")?, "localise", Val::bool(false))?;
        }
    }
    Ok(())
}

/// An addon's options page under another screen name, for an addon screen with `page: options`.
pub fn build_options_page(src: &Sources, e: &Entry, screen: &str) -> Result<Vec<u8>> {
    let e = Entry {
        kind: e.kind,
        index: e.index,
        screen: screen.to_owned(),
        name: e.name,
        author: e.author,
        version: e.version,
        description: e.description,
        id: e.id,
        track: e.track,
        maps: e.maps.clone(),
        folder: e.folder,
        options: e
            .options
            .iter()
            .map(|o| Opt {
                k: o.k,
                local: o.local,
                o: o.o,
            })
            .collect(),
        all: Vec::new(),
        setup: e.setup,
    };
    let rows = addon_rows(&e);
    let mut scr = build_page(src, &e, &rows, !e.options.is_empty())?;
    retitle(&mut scr, &e)?;
    if !e.options.is_empty() {
        vm_page(&mut scr, &e)?;
    }
    settings_state_matrix(src, &mut scr)?;
    literal_tooltips(&mut scr)?;
    ui_bytes(vec![scr])
}
