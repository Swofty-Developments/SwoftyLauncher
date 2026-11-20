use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::asset::{AssetOp, Features};
use crate::launcher::ProfileInfo;
use crate::view::Event;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    Hello(Hello),
    Profile(ProfileInfo),
    View {
        section: String,
    },
    Event {
        section: String,
        event: Event,
    },
    Import {
        paths: Vec<PathBuf>,
    },
    Build {
        options: BTreeMap<String, Value>,
        features: Features,
    },
    Command {
        path: Vec<String>,
        args: Vec<String>,
    },
    Summary,
    Tick {
        section: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub api: u32,
    pub addon: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub profile: ProfileInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_calls: Vec<String>,
    /// Optional view/protocol features; absent on older hosts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub api: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Built {
    #[serde(default)]
    pub ops: Vec<AssetOp>,
    // The item each op comes from, by position in ops, such as one mod of the
    // addon's library. Core builds one runtime overlay archive per item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Option<String>>,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    // Field names for add-row, keyed `<table>.<row>` as in an op file's `fields`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<String>>,
    // The absolute paths of every file and folder the build read; a launcher that lists build.reads
    // answers the next build with this reply while none of them changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<Vec<PathBuf>>,
}
