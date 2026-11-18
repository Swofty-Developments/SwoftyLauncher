use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The vehicle placements a world layer holds per kind, with the ids 1 to 32. The game stores a placement request at
/// slot `Id - 1` of its kind's 32 (Sess_VehiclePlacementRequest_Register, 0x142f60c80) with no bound, so another id
/// writes into another kind's requests or past the table.
pub const PLACEMENT_SLOTS: u32 = 32;
const PLACEMENT_KEYS: [&str; 2] = ["overrides.VehPlacement.Id", "overrides.F1Garage.id"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PkgPatch {
    pub schema_version: u32,
    pub targets: Vec<PkgTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    pub edits: Vec<PkgEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PkgTarget {
    pub erp: String,
    pub resource: String,
    #[serde(default)]
    pub fragment: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Signal {
    pub to: String,
    pub pin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventReplacement {
    pub event: String,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PkgEdit {
    CloneNode {
        src: String,
        #[serde(rename = "as")]
        name: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        set: BTreeMap<String, serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replace_event: Option<EventReplacement>,
    },
    SetText {
        node: String,
        value: String,
        #[serde(default)]
        localise: bool,
    },
    SetProps {
        node: String,
        set: BTreeMap<String, serde_json::Value>,
    },
    AddSignal {
        node: String,
        event: String,
        signal: Signal,
    },
    RemoveSignal {
        node: String,
        event: String,
        to: String,
    },
    CloneEntry {
        at: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<PkgTarget>,
        src: String,
        #[serde(rename = "as")]
        name: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        set: BTreeMap<String, serde_json::Value>,
    },
    SetValue {
        path: String,
        value: serde_json::Value,
    },
}

fn placement_id(key: &str, value: &serde_json::Value) -> Option<String> {
    let id: i128 = match value {
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    if (1..=i128::from(PLACEMENT_SLOTS)).contains(&id) {
        return None;
    }
    Some(format!(
        "{key} {id} is outside 1..={PLACEMENT_SLOTS}, the vehicle placements the game holds per kind"
    ))
}

impl PkgEdit {
    /// Why the edit is refused: it gives an entry a vehicle placement or garage id outside 1..=[`PLACEMENT_SLOTS`].
    pub fn refusal(&self) -> Option<String> {
        match self {
            PkgEdit::CloneEntry { name, set, .. } => PLACEMENT_KEYS.iter().find_map(|key| {
                let why = placement_id(key, set.get(*key)?)?;
                Some(format!("clone-entry {name}: {why}"))
            }),
            PkgEdit::SetValue { path, value } => PLACEMENT_KEYS.iter().find_map(|key| {
                if path != key && !path.strip_suffix(key)?.ends_with('.') {
                    return None;
                }
                let why = placement_id(key, value)?;
                Some(format!("set-value {path}: {why}"))
            }),
            _ => None,
        }
    }
}
