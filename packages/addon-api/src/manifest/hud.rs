use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hud {
    /// Rows on the ADDONS page of the race MFD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfd: Option<HudMfd>,
    /// NO/YES rows on the pit page of the race MFD, below the front wing repair row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pit_rows: Option<Vec<HudPitRow>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudMfd {
    /// The group title the page shows above the addon's rows; the addon name when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Option ids, in page order; each is a toggle or choice option of this addon.
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudPitRow {
    /// The id the addon's native code names the row by.
    pub id: String,
    /// The text the row shows, as written.
    pub label: String,
}

pub const MFD_ROWS_MAX: usize = 8;

pub const MFD_TEXT_MAX: usize = 32;

pub const PIT_ROWS_MAX: usize = 2;

pub const PIT_ROW_ID_MAX: usize = 32;

pub const PIT_ROW_LABEL_MAX: usize = 24;
