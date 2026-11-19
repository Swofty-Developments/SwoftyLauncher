use std::path::Path;

use super::buildhash::BuildHash;
use crate::addons::builtin::{CORE_ID, CORE_INTRO, stage_intro};
use crate::addons::settings::{OnAddons, render_ops};
use crate::asset::{AssetOp, FileKind, OpFile};
use crate::dbedit::driver_manager;
use crate::erp::unsupported;
use crate::manifest::AddonManifest;
use crate::profile::Profile;
use crate::store::Store;
use crate::{Error, Result, io_err};

pub(super) fn render_file(
    path: &Path,
    kind: FileKind,
    m: &AddonManifest,
    p: &Profile,
    on: &OnAddons,
    hash: &mut BuildHash,
) -> Result<OpFile> {
    let doc = render_doc(path, m, p, on)?;
    hash.json(&doc);
    OpFile::from_value(doc, kind).map_err(json_at(path))
}

/// The core's startup intro ops, empty while the `startup_intro` setting is off. The
/// video is staged in the core's cache folder, which the `file.put` reads.
pub(super) fn core_intro(
    store: &Store,
    m: &AddonManifest,
    p: &Profile,
    on: &OnAddons,
    hash: &mut BuildHash,
) -> Result<OpFile> {
    let mut doc = serde_json::from_str(CORE_INTRO).expect("the built-in intro ops are JSON");
    render_ops(&mut doc, m, p, on)?;
    let file = OpFile::from_value(doc, FileKind::Assets).expect("the built-in intro ops are valid");
    hash.json(&file);
    if !file.ops.is_empty() {
        stage_intro(&store.addon_cache_dir(CORE_ID))?;
    }
    Ok(file)
}

/// One of an addon's op files as a build reads it, with its option
/// templating filled from the profile's settings and the `when`s checked against the addons on.
pub fn read_op_file(
    path: &Path,
    kind: FileKind,
    m: &AddonManifest,
    p: &Profile,
    on: &OnAddons,
) -> Result<OpFile> {
    OpFile::from_value(render_doc(path, m, p, on)?, kind).map_err(json_at(path))
}

fn render_doc(path: &Path, m: &AddonManifest, p: &Profile, on: &OnAddons) -> Result<serde_json::Value> {
    let raw = std::fs::read(path).map_err(io_err(path))?;
    let mut doc = serde_json::from_slice(&raw).map_err(json_at(path))?;
    render_ops(&mut doc, m, p, on)?;
    Ok(doc)
}

fn json_at(path: &Path) -> impl FnOnce(serde_json::Error) -> Error + '_ {
    move |source| Error::Json {
        path: path.to_owned(),
        source,
    }
}

/// A DB Editor package's ops, generated from its `dbedit.json` rather than
/// read from its `database.json`, and the Driver Manager drivers its values pick.
pub(super) fn package_ops(store: &Store, id: &str) -> Result<(OpFile, Vec<String>)> {
    let edits = crate::dbedit::package::Package::open(store, id)?.edits;
    let ops = edits.to_ops();
    let file = OpFile {
        schema_version: ops.schema_version,
        fields: ops.fields,
        strict: ops.strict,
        side_data: ops.side_data,
        ops: ops.ops.into_iter().map(AssetOp::Db).collect(),
    };
    Ok((file, driver_manager::picks(&edits)))
}

/// Drops the ops [`AssetOp::refusal`] refuses from an addon's op file `at`, each with a warning naming the addon, the
/// file and the entry.
pub(super) fn refuse_placements(id: &str, at: &str, ops: &mut Vec<AssetOp>, warnings: &mut Vec<String>) {
    refuse_by(id, at, ops, |op| op, warnings);
}

pub(super) fn refuse_by<T>(
    id: &str,
    at: &str,
    ops: &mut Vec<T>,
    op: impl Fn(&T) -> &AssetOp,
    warnings: &mut Vec<String>,
) {
    ops.retain(|o| match op(o).refusal() {
        Some(why) => {
            warnings.push(format!("{id}: {at}: refused {why}"));
            false
        }
        None => true,
    });
}

pub(super) fn unbuilt<'a>(ops: impl IntoIterator<Item = &'a AssetOp>) -> Option<&'static str> {
    ops.into_iter().find_map(|op| match op {
        AssetOp::Erp(op) => unsupported(op),
        _ => None,
    })
}
