mod calls;
mod capabilities;
mod host_calls;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use self::calls::{Built, Call, Hello, Welcome};
pub use self::capabilities::{
    ADDON_CALLS, API_VERSION, BUILD_READS, MOVE_ADDON, PROFILE_HEADS, RESTART, SAVE_DIR, TASKS,
    VIEW_CELL_IMAGE_DATA, VIEW_CHART, VIEW_CHART_OPTIONS, VIEW_SELECTION_DIALOG,
};
pub use self::host_calls::{HostCall, Invalidate};
pub use crate::run_helper::{claim_word, run_helper};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request<C> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub call: C,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Ok { id: u64, result: Value },
    Err { id: u64, error: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Reply(Reply),
    Call(Request<HostCall>),
}

#[cfg(test)]
mod tests;
