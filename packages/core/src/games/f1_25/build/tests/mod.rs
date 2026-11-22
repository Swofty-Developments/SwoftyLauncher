use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::archives::{kept_menus_line, stray_asset_groups};
use super::failure::{game_refused_text, loader_missing};
use super::files::unshipped;
use super::registry::{added_tables, early_stage_line, registering_addons};
use super::saved::built_files;
use super::*;
use crate::activation::handling::{self, HandlingMap};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_formats::gvdb::Gvdb;

use super::archives::{
    carries_handling, merged_later, missing_archives, slice_merges, stock_holders, stray_asset_group_line,
};
use super::buildhash::BuildHash;
use super::loader::{check_menu_originals, loader_line};
use super::registry::replaced_operations;
use crate::activation::place::{SLOTS, slot_file};
use crate::addons::builtin::{CORE_ID, loader_dir, loader_files};
use crate::addons::settings::OnAddons;
use crate::asset::AssetOp;
use crate::asset::{AssetBatch, Op, Origin};
use crate::asset::{FileKind, GroupOp, OpFile};
use crate::asset::{FileOp, Root, Source};
use crate::content::runtime_assets::db_edits;
use crate::dbedit::driver_manager;
use crate::erp::Resized;
use crate::game_folder::baseline::Identity;
use crate::game_folder::baseline::{Baseline, Objects, hex};
use crate::launcher::BuildCx;
use crate::launcher::{Addon, Builtin};
use crate::manifest::AddonManifest;
use crate::online::multiplayer::fingerprint;
use crate::profile::Profile;

mod addons;
mod archives;
mod cars;
mod database;
mod digests;
mod failure;
mod picture;
mod registry;
mod reuse;
mod row_ids;
mod setup;
mod stale;
mod tracks;

pub(super) struct Rows(pub(super) AddonManifest);

impl Addon for Rows {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        let reply: swofty_addon::helper::Built = serde_json::from_str(
            r#"{ "ops": [{ "op": "db.add-row", "table": "entrants.teams",
                           "from": "entrants.teams.team[_metadata_=mclaren_26]",
                           "set": { "_metadata_": "team12_26" } }],
                 "fields": { "entrants.teams.team": ["_metadata_", "stat_id"] } }"#,
        )
        .expect("valid reply");
        let origin = Origin {
            addon: cx.cx.addon.into(),
            item: None,
        };
        Ok(AssetBatch {
            ops: reply
                .ops
                .into_iter()
                .map(|op| Op {
                    op,
                    origin: origin.clone(),
                })
                .collect(),
            fields: reply.fields,
            ..AssetBatch::default()
        })
    }
}

pub(super) fn archive(names: &[&str], byte: u8) -> Vec<u8> {
    archive_of("UILayout", names, byte)
}

pub(super) fn archive_of(kind: &str, names: &[&str], byte: u8) -> Vec<u8> {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    Erp {
        header,
        version: 3,
        resources: names
            .iter()
            .map(|n| Resource {
                name: (*n).into(),
                kind: swofty_formats::erp::kind(kind).unwrap(),
                unk: vec![0; 4],
                fragments: vec![Fragment::new(*b"temp", 1, 0x10, STORED, vec![byte])],
                hash: [0; 16],
            })
            .collect(),
    }
    .to_bytes()
    .unwrap()
}
