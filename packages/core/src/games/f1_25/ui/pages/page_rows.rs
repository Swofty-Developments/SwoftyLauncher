//! The rows of an entry's page: an addon's options, track packages, maps and mods, and the values they start
//! at.

use super::text::{RESTART_NOTE, fit, label_case, with_note};
use super::{Entry, Opt};
use crate::Result;
use crate::addons::settings::SessionOption;
use crate::manifest::{Binds, OptionType};
use crate::ui::node::*;

pub(super) struct Row<'a> {
    pub(super) key: String,
    pub(super) kind: RowKind<'a>,
    pub(super) label: String,
    pub(super) title: String,
    pub(super) body: String,
}

pub(super) enum RowKind<'a> {
    Info(String),
    Slider(&'a Opt<'a>),
    Toggle(&'a Opt<'a>),
}

pub(super) fn addon_rows<'a>(e: &'a Entry<'a>) -> Vec<Row<'a>> {
    let mut rows = track_rows(e);
    rows.extend(option_rows(e));
    rows
}

// A track addon's track packages: their codes, to pick in any mode.
pub(super) fn track_rows<'a>(e: &Entry) -> Vec<Row<'a>> {
    let several = e.maps.len() > 1;
    e.maps
        .iter()
        .enumerate()
        .map(|(j, mp)| {
            let label = match several {
                true => format!("Track Code: {}", mp.name),
                false => "Track Code".to_owned(),
            };
            Row {
                key: format!("row_code_{j}"),
                kind: RowKind::Info(mp.code.clone()),
                title: fit(label.to_uppercase()),
                label: fit(label),
                body: "Pick this track in any mode to race on it.".into(),
            }
        })
        .collect()
}

pub(super) fn option_rows<'a>(e: &'a Entry<'a>) -> Vec<Row<'a>> {
    if e.options.is_empty() {
        let none = match e.maps.is_empty() {
            true => "This addon has no options.",
            false => "",
        };
        return vec![
            Row {
                key: "row_id".into(),
                kind: RowKind::Info(e.id.into()),
                label: "Addon ID".into(),
                title: "ADDON ID".into(),
                body: none.into(),
            },
            Row {
                key: "row_version".into(),
                kind: RowKind::Info(e.version.into()),
                label: "Version".into(),
                title: "VERSION".into(),
                body: String::new(),
            },
        ];
    }
    e.options
        .iter()
        .map(|opt| {
            let o = opt.o;
            let restart = if o.binds == Binds::Db { RESTART_NOTE } else { "" };
            Row {
                key: format!("row_{}", opt.k),
                kind: if o.kind == OptionType::Slider {
                    RowKind::Slider(opt)
                } else {
                    RowKind::Toggle(opt)
                },
                label: fit(label_case(&o.label)),
                title: fit(o.label.to_uppercase()),
                body: with_note(&o.description, restart),
            }
        })
        .collect()
}

pub(super) fn map_rows<'a>(e: &Entry) -> Vec<Row<'a>> {
    let info = |key: &str, label: &str, value: &str, title: &str, body: &str| Row {
        key: key.into(),
        kind: RowKind::Info(value.into()),
        label: label.into(),
        title: title.into(),
        body: body.into(),
    };
    vec![
        info(
            "row_code",
            "Track Code",
            e.id,
            "TRACK CODE",
            "Pick this track in any mode to race on it.",
        ),
        info("row_author", "Author", e.author, "AUTHOR", ""),
        info("row_version", "Version", e.version, "VERSION", ""),
    ]
}

pub(super) fn mod_rows<'a>(e: &Entry) -> Vec<Row<'a>> {
    let info = |key: &str, label: &str, value: &str, title: &str, body: &str| Row {
        key: key.into(),
        kind: RowKind::Info(value.into()),
        label: label.into(),
        title: title.into(),
        body: body.into(),
    };
    vec![
        info(
            "row_id",
            "Mod ID",
            e.id,
            "MOD ID",
            "Enabled by its addon. Turn it on or off from the addon's page in the launcher.",
        ),
        info("row_author", "Author", e.author, "AUTHOR", ""),
        info("row_version", "Version", e.version, "VERSION", ""),
    ]
}

pub(super) fn state_value(o: &SessionOption) -> Result<&crate::addons::settings::SessionState> {
    o.states
        .get(o.current)
        .ok_or_else(|| err(format!("option {} has no state {}", o.id, o.current)))
}

pub(super) fn number(v: &serde_json::Value) -> Result<f32> {
    v.as_f64()
        .map(|x| x as f32)
        .ok_or_else(|| err(format!("slider value {v} is not a number")))
}
