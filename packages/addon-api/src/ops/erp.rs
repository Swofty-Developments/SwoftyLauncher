use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::asset::Source;
use crate::ops::pkg::PkgEdit;

impl ErpOp {
    /// Why the op is refused: a `patch-pkg` edit that [`PkgEdit::refusal`] refuses, named with its archive and resource.
    pub fn refusal(&self) -> Option<String> {
        match self {
            ErpOp::PatchPkg {
                erp, resource, edits, ..
            } => edits
                .iter()
                .find_map(PkgEdit::refusal)
                .map(|why| format!("{erp} {resource}: {why}")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ErpOp {
    ReplaceResource {
        erp: String,
        resource: String,
        #[serde(default)]
        fragment: usize,
        source: Source,
    },
    AddResource {
        erp: String,
        resource: String,
        #[serde(rename = "type")]
        kind: String,
        sources: Vec<Source>,
    },
    CopyResource {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_erp: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<Source>,
        erp: String,
        resource: String,
        #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
        rename: Option<String>,
    },
    PutResource {
        erp: String,
        resource: String,
        source: Source,
        #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
        rename: Option<String>,
    },
    /// Removing a resource the archive does not have is a no-op.
    RemoveResource {
        erp: String,
        resource: String,
    },
    AddMipmaps {
        source: Source,
        target: String,
    },
    PatchPkg {
        erp: String,
        resource: String,
        #[serde(default)]
        fragment: usize,
        edits: Vec<PkgEdit>,
    },
    // A ready overlay archive: each resource in it goes into every one of `erps`, as a
    // put-resource from it would. On the runtime route the archive is loaded as it is.
    Overlay {
        erps: Vec<String>,
        source: Source,
    },
    // Each slice of the texture array `resource` in `source` that differs from the
    // game's stock slice goes over the array already in `erp`.
    MergeSlices {
        erp: String,
        resource: String,
        source: Source,
    },
    // Every slice of the texture array `resource` in `source` goes after the slices of
    // the array already in `erp` (the game's, or as earlier addons left it), in load
    // order, and the array's view (`resource` without `.image`) counts them.
    AppendSlices {
        erp: String,
        resource: String,
        source: Source,
    },
    /// Sets constants of a `.material` resource in place: a number for a scalar, a list for a
    /// vector, written as the material declares them.
    SetMaterial {
        erp: String,
        resource: String,
        params: BTreeMap<String, MaterialValue>,
    },
}

/// A material constant's value: a number, a list, or text from a `{{option}}` placeholder holding
/// one or more numbers separated by spaces or commas (`true` and `false` read as 1 and 0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MaterialValue {
    Scalar(f64),
    Vector(Vec<MaterialNumber>),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MaterialNumber {
    Number(f64),
    Text(String),
}

fn material_number(text: &str) -> std::result::Result<f64, String> {
    match text.trim() {
        "true" => Ok(1.0),
        "false" => Ok(0.0),
        t => t
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{t:?} is not a number")),
    }
}

impl MaterialValue {
    pub fn values(&self) -> std::result::Result<Vec<f64>, String> {
        match self {
            Self::Scalar(v) => Ok(vec![*v]),
            Self::Vector(v) => v
                .iter()
                .map(|n| match n {
                    MaterialNumber::Number(v) => Ok(*v),
                    MaterialNumber::Text(t) => material_number(t),
                })
                .collect(),
            Self::Text(t) => t
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(material_number)
                .collect(),
        }
    }
}
