use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::asset::{AssetOp, Features, FileOp, GroupOp, Kind, LangOp, Op, Origin, Source};
use crate::dbedit::deps::Copy;
use crate::game_db::entrant_lists::StockLists;
use crate::ops::{DbOps, ErpOp, Rewrite};

mod db;
mod lines;
mod results;
mod setup;
mod writers;

#[cfg(test)]
mod tests;

use self::lines::stat_row;
pub use self::lines::{
    kept_lists_line, moved_stat_id_line, stock_removals_line, track_removals_line, waiting_lines,
};

pub const FEATURES_FILE: &str = "swofty/addons/core/features.json";

const BUILD_DELIVERIES: [Kind; 3] = [Kind::Erp, Kind::Db, Kind::File];
const RUNTIME_DELIVERIES: [Kind; 5] = [Kind::Group, Kind::Erp, Kind::Db, Kind::Lang, Kind::File];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Build,
    Runtime,
    Waiting,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Level {
    Stable,
    Experimental,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct FeaturesFile {
    #[serde(default)]
    features: BTreeMap<String, Level>,
}

pub fn load_features(loader: &Path) -> Result<Features> {
    let path = loader.join(FEATURES_FILE);
    if !path.is_file() {
        return Ok(Features::default());
    }
    let file: FeaturesFile = crate::read_json(&path)?;
    let mut out = Features::default();
    for (name, level) in file.features {
        let Ok(kind) = serde_json::from_value::<Kind>(name.into()) else {
            continue;
        };
        match level {
            Level::Stable => out.stable.insert(kind),
            Level::Experimental => out.experimental.insert(kind),
            Level::Other => false,
        };
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    Group(String),
    Resource { erp: String, uri: String },
    Slice { erp: String, uri: String, slice: usize },
    Record(u64),
    String { language: String, key: String },
    File(String),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Group(g) => write!(f, "asset group {g}"),
            Target::Resource { erp, uri } => write!(f, "{uri} in {erp}"),
            Target::Slice { erp, uri, slice } => write!(f, "slice {slice} of {uri} in {erp}"),
            Target::Record(key) => write!(f, "database record {key:#018x}"),
            Target::String { language, key } => write!(f, "text {key} ({language})"),
            Target::File(rel) => f.write_str(rel),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub target: Target,
    pub winner: Origin,
    pub overridden: Vec<Origin>,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lost: Vec<String> = self.overridden.iter().map(Origin::to_string).collect();
        write!(
            f,
            "{}: {} overrides {}",
            self.target,
            self.winner,
            lost.join(", ")
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct Planned {
    pub runtime: Vec<Op>,
    pub waiting: Vec<Op>,
    pub conflicts: Vec<Conflict>,
    /// Names for conflicting records of rows the build added, which the catalogue doesn't know.
    pub record_names: BTreeMap<u64, String>,
}

pub struct Planner<'a> {
    features: &'a Features,
    opt_in: &'a BTreeSet<Kind>,
    build_only: &'a BTreeSet<Kind>,
    db: Vec<(Origin, DbOps)>,
    erp: Vec<Op>,
    files: Vec<Op>,
    tracks: Vec<Op>,
    items: Vec<Op>,
    planned: Planned,
    writers: BTreeMap<Target, Vec<Origin>>,
    // Origins in the order their first op came, which is load order.
    ranks: Vec<Origin>,
    removals: usize,
    fields: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    skipped: Vec<String>,
    id_left_out: BTreeMap<String, usize>,
    waits: BTreeSet<(String, String)>,
    stock_removals: BTreeMap<String, usize>,
    track_removals: BTreeMap<String, usize>,
    lists: Option<StockLists>,
    kept_lists: BTreeMap<String, Vec<String>>,
    copies: Vec<Copy>,
    lenient: BTreeMap<String, String>,
    names: BTreeMap<String, String>,
    // The tracks the build leaves out because the game's track lists are full, by code, each with its name.
    left_out_tracks: BTreeMap<String, String>,
    // The records each origin's database ops changed, by the origin's rank.
    rewrites: Vec<(usize, Rewrite)>,
    core_rows: BTreeMap<String, BTreeSet<String>>,
    touched: BTreeMap<String, Touched>,
    overlays: Option<&'a OverlayNames<'a>>,
    mipmaps: Option<&'a MipmapNames<'a>>,
    row_ids: Option<RefCell<crate::row_ids::Store>>,
}

/// The resource names in an `erp.overlay` op's archive.
pub type OverlayNames<'a> = dyn Fn(&Origin, &Source) -> Vec<String> + 'a;

/// The file names in an `erp.add-mipmaps` op's source folder; empty when the source is a single file.
pub type MipmapNames<'a> = dyn Fn(&Origin, &Source) -> Vec<String> + 'a;

#[derive(Default)]
struct Touched {
    records: HashSet<u64>,
    other: HashSet<Target>,
}

fn erp_target(op: &ErpOp) -> Option<(Target, bool)> {
    let resource = |erp: &str, uri: &str| Target::Resource {
        erp: erp.to_owned(),
        uri: uri.to_owned(),
    };
    match op {
        ErpOp::ReplaceResource { erp, resource: r, .. } | ErpOp::RemoveResource { erp, resource: r } => {
            Some((resource(erp, r), true))
        }
        ErpOp::PutResource {
            erp,
            resource: r,
            rename,
            ..
        } => Some((resource(erp, rename.as_deref().unwrap_or(r)), true)),
        ErpOp::CopyResource {
            erp,
            resource: r,
            rename,
            ..
        } => Some((resource(erp, rename.as_deref().unwrap_or(r)), false)),
        ErpOp::AddResource { erp, resource: r, .. } => Some((resource(erp, r), true)),
        ErpOp::PatchPkg { erp, resource: r, .. } | ErpOp::SetMaterial { erp, resource: r, .. } => {
            Some((resource(erp, r), false))
        }
        ErpOp::AddMipmaps { target, .. } => {
            Some((Target::File(target.trim_end_matches('/').to_owned()), true))
        }
        // Its resources come from the archive's index, through `Planner::overlays`.
        ErpOp::Overlay { .. } => None,
        // The build writes one target per slice it changes, through `Planner::replaced`.
        ErpOp::MergeSlices { .. } => None,
        // It adds slices after the array as earlier addons left it, so it replaces nothing.
        ErpOp::AppendSlices { .. } => None,
    }
}

fn target(op: &AssetOp) -> Option<(Target, bool)> {
    match op {
        AssetOp::Group(GroupOp::Register { group, .. }) => Some((Target::Group(group.clone()), false)),
        AssetOp::Erp(op) => erp_target(op),
        AssetOp::Lang(LangOp::Set { language, key, .. }) => Some((
            Target::String {
                language: language.clone(),
                key: key.clone(),
            },
            true,
        )),
        AssetOp::File(FileOp::Put { target, .. }) => Some((Target::File(target.clone()), true)),
        AssetOp::Db(_)
        | AssetOp::Lang(LangOp::Table { .. } | LangOp::Default { .. })
        | AssetOp::Track(_)
        | AssetOp::Item(_)
        | AssetOp::Warn { .. } => None,
    }
}
