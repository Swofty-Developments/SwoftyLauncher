use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
