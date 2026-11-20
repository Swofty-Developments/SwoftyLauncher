use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hud {
    /// Rows on the ADDONS page of the race MFD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfd: Option<HudMfd>,
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

pub const MFD_ROWS_MAX: usize = 8;

pub const MFD_TEXT_MAX: usize = 32;
