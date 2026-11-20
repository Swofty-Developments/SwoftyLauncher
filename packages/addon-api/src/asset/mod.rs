use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::manifest::{Operations, relative_inside};
use crate::ops::{DbOp, ErpOp, SideData};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Group,
    Erp,
    Db,
    Lang,
    File,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Kind::Group => "group",
            Kind::Erp => "erp",
            Kind::Db => "db",
            Kind::Lang => "lang",
            Kind::File => "file",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssetOp {
    Group(GroupOp),
    Erp(ErpOp),
    Db(DbOp),
    Lang(LangOp),
    File(FileOp),
    Track(TrackOp),
    Item(ItemOp),
    Warn { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GroupOp {
    Register {
        group: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        erp: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        erps: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        deps: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LangOp {
    Set {
        language: String,
        key: String,
        value: String,
    },
    Table {
        language: String,
        source: Source,
    },
    Default {
        language: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FileOp {
    Put { source: Source, target: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TrackOp {
    Add { source: Source },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ItemOp {
    Show {
        id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        author: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        version: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
    },
}

impl AssetOp {
    /// Why the op is refused at validation ([`ErpOp::refusal`]).
    pub fn refusal(&self) -> Option<String> {
        match self {
            AssetOp::Erp(op) => op.refusal(),
            _ => None,
        }
    }

    pub fn kind(&self) -> Option<Kind> {
        match self {
            AssetOp::Group(_) => Some(Kind::Group),
            AssetOp::Erp(_) => Some(Kind::Erp),
            AssetOp::Db(_) => Some(Kind::Db),
            AssetOp::Lang(_) => Some(Kind::Lang),
            AssetOp::File(_) => Some(Kind::File),
            AssetOp::Track(_) | AssetOp::Item(_) | AssetOp::Warn { .. } => None,
        }
    }

    pub fn sources(&self) -> Vec<&Source> {
        match self {
            AssetOp::Erp(
                ErpOp::ReplaceResource { source, .. }
                | ErpOp::PutResource { source, .. }
                | ErpOp::AddMipmaps { source, .. }
                | ErpOp::Overlay { source, .. }
                | ErpOp::MergeSlices { source, .. }
                | ErpOp::AppendSlices { source, .. },
            )
            | AssetOp::Lang(LangOp::Table { source, .. })
            | AssetOp::File(FileOp::Put { source, .. })
            | AssetOp::Track(TrackOp::Add { source }) => vec![source],
            AssetOp::Erp(ErpOp::AddResource { sources, .. }) => sources.iter().collect(),
            AssetOp::Erp(ErpOp::CopyResource { source, .. }) => source.iter().collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Plain {
    Warn { message: String },
}

impl<'de> Deserialize<'de> for AssetOp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let mut op = Map::deserialize(d)?;
        let name = match op.get("op") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err(D::Error::missing_field("op")),
        };
        let (kind, rest) = name.split_once('.').unwrap_or(("", &name));
        op.insert("op".into(), rest.into());
        let op = Value::Object(op);
        match kind {
            "group" => GroupOp::deserialize(op).map(AssetOp::Group),
            "erp" => ErpOp::deserialize(op).map(AssetOp::Erp),
            "db" => DbOp::deserialize(op).map(AssetOp::Db),
            "lang" => LangOp::deserialize(op).map(AssetOp::Lang),
            "file" => FileOp::deserialize(op).map(AssetOp::File),
            "track" => TrackOp::deserialize(op).map(AssetOp::Track),
            "item" => ItemOp::deserialize(op).map(AssetOp::Item),
            "" => Plain::deserialize(op).map(|Plain::Warn { message }| AssetOp::Warn { message }),
            _ => return Err(D::Error::custom(format!("unknown op {name}"))),
        }
        .map_err(D::Error::custom)
    }
}

impl Serialize for AssetOp {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let (prefix, value) = match self {
            AssetOp::Group(op) => ("group", serde_json::to_value(op)),
            AssetOp::Erp(op) => ("erp", serde_json::to_value(op)),
            AssetOp::Db(op) => ("db", serde_json::to_value(op)),
            AssetOp::Lang(op) => ("lang", serde_json::to_value(op)),
            AssetOp::File(op) => ("file", serde_json::to_value(op)),
            AssetOp::Track(op) => ("track", serde_json::to_value(op)),
            AssetOp::Item(op) => ("item", serde_json::to_value(op)),
            AssetOp::Warn { message } => {
                return Plain::Warn {
                    message: message.clone(),
                }
                .serialize(s);
            }
        };
        let mut value = value.map_err(serde::ser::Error::custom)?;
        if let Some(name) = value.get_mut("op") {
            *name = format!("{prefix}.{}", name.as_str().unwrap_or_default()).into();
        }
        value.serialize(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Op {
    pub op: AssetOp,
    pub origin: Origin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Origin {
    pub addon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.item {
            Some(item) => write!(f, "{} ({item})", self.addon),
            None => f.write_str(&self.addon),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "SourceRepr", into = "SourceRepr")]
pub struct Source {
    pub root: Root,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Root {
    Addon,
    Data,
    Cache,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum SourceRepr {
    Addon(String),
    Rooted { root: Root, path: String },
}

impl From<SourceRepr> for Source {
    fn from(r: SourceRepr) -> Self {
        match r {
            SourceRepr::Addon(path) => Self::addon(path),
            SourceRepr::Rooted { root, path } => Self { root, path },
        }
    }
}

impl From<Source> for SourceRepr {
    fn from(s: Source) -> Self {
        match s.root {
            Root::Addon => Self::Addon(s.path),
            root => Self::Rooted { root, path: s.path },
        }
    }
}

impl Source {
    pub fn addon(path: impl Into<String>) -> Self {
        Self {
            root: Root::Addon,
            path: path.into(),
        }
    }

    pub fn resolve(&self, dir: &Path) -> Result<PathBuf> {
        if relative_inside(&self.path) {
            Ok(dir.join(&self.path))
        } else {
            Err(Error::Resolve(format!(
                "{} must be a relative path inside the addon",
                self.path
            )))
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AssetBatch {
    pub ops: Vec<Op>,
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<String>>,
    /// The absolute paths of every file and folder the build read, sent as a helper's build reply's
    /// `reads`; None runs the build on every Play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<Vec<std::path::PathBuf>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Features {
    pub stable: BTreeSet<Kind>,
    pub experimental: BTreeSet<Kind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Database,
    Erp,
    Assets,
}

impl FileKind {
    fn prefix(self) -> Option<&'static str> {
        match self {
            FileKind::Database => Some("db"),
            FileKind::Erp => Some("erp"),
            FileKind::Assets => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpFile {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strict: bool,
    #[serde(default, skip_serializing_if = "SideData::is_empty")]
    pub side_data: SideData,
    pub ops: Vec<AssetOp>,
}

impl OpFile {
    pub fn from_value(mut doc: Value, kind: FileKind) -> serde_json::Result<Self> {
        if let (Some(prefix), Some(doc)) = (kind.prefix(), doc.as_object_mut()) {
            if kind == FileKind::Erp {
                doc.remove("foreach");
            }
            let ops = doc.get_mut("ops").and_then(Value::as_array_mut);
            for name in ops.into_iter().flatten().filter_map(|op| op.get_mut("op")) {
                if let Some(n) = name.as_str().filter(|n| *n != "warn") {
                    *name = format!("{prefix}.{n}").into();
                }
            }
        }
        serde_json::from_value(doc)
    }
}

impl Operations {
    pub fn op_files(&self) -> impl Iterator<Item = (FileKind, &str)> {
        [
            (FileKind::Database, &self.database),
            (FileKind::Erp, &self.erp),
            (FileKind::Assets, &self.assets),
        ]
        .into_iter()
        .flat_map(|(kind, files)| files.iter().map(move |rel| (kind, rel.as_str())))
    }

    pub fn file_ops(&self) -> impl Iterator<Item = AssetOp> {
        self.files.iter().map(|f| {
            AssetOp::File(FileOp::Put {
                source: Source::addon(&f.source),
                target: f.target.clone(),
            })
        })
    }
}

#[cfg(test)]
mod tests;
