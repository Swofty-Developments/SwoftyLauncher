//! The rows of the ADDONS page of the in-car MFD: a heading per addon, then its rows.

use super::mfd;
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
