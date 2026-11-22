//! The rows of the ADDONS page of the in-car MFD: a heading per addon, then its rows.

use super::{mfd, pit_page};
use crate::addons::settings::SessionSettings;

pub struct MfdRow {
    pub addon: String,
    pub title: String,
    pub option: String,
    pub label: String,
    pub text: String,
}

pub fn mfd_rows(settings: &SessionSettings) -> Vec<MfdRow> {
    let mut out = Vec::new();
    for a in settings.addons.iter().filter(|a| a.enabled) {
        let Some(m) = &a.mfd else { continue };
        for id in &m.rows {
            if let Some(o) = a.options.iter().find(|o| &o.id == id) {
                out.push(MfdRow {
                    addon: a.id.clone(),
                    title: m.title.clone(),
                    option: o.id.clone(),
                    label: o.label.clone(),
                    text: o
                        .states
                        .get(o.current)
                        .map(|s| s.text.clone())
                        .unwrap_or_default(),
                });
            }
        }
    }
    out.truncate(swofty_addon::manifest::MFD_ROWS_MAX);
    out
}

pub struct PitRow {
    pub addon: String,
    pub id: String,
    pub label: String,
    pub name: String,
}

/// The pit page rows of the enabled addons, in session order, each named `swofty_pit_<k>`.
pub fn pit_rows(settings: &SessionSettings) -> Vec<PitRow> {
    settings
        .addons
        .iter()
        .filter(|a| a.enabled)
        .flat_map(|a| a.pit_rows.iter().map(move |r| (a, r)))
        .take(swofty_addon::manifest::PIT_ROWS_MAX)
        .enumerate()
        .map(|(k, (a, r))| PitRow {
            addon: a.id.clone(),
            id: r.id.clone(),
            label: r.label.clone(),
            name: pit_page::row_name(k),
        })
        .collect()
}

/// The rows of the ADDONS MFD page: a heading per addon, then its rows.
pub fn mfd_page_rows(mfd_rows: &[MfdRow]) -> Vec<mfd::Row> {
    let mut rows: Vec<mfd::Row> = Vec::new();
    for (k, r) in mfd_rows.iter().enumerate() {
        if k == 0 || mfd_rows[k - 1].addon != r.addon {
            rows.push(mfd::Row {
                label: r.title.clone(),
                value: None,
            });
        }
        rows.push(mfd::Row {
            label: r.label.clone(),
            value: Some(mfd::value_path(k)),
        });
    }
    rows
}
