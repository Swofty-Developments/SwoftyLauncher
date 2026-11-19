use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::digests::Digests;
use crate::addons::builtin::{loader_dir, loader_files};
use crate::asset::{AssetOp, Op};
use crate::erp::reads;
use crate::game_folder::baseline::hex;
use crate::install::Game;
use crate::manifest::AddonManifest;
use crate::store::Store;
use crate::{Result, io_err};

/// Sources at most this big are stamped by their bytes: build hooks rewrite small generated files on every
/// build, which moves their modified time without changing them.
const CONTENT_STAMP_MAX: u64 = 1 << 20;

/// The SHA-256 of a file small enough to be keyed by its bytes rather than its modified time; None for a
/// larger file or a folder.
pub(super) fn small_file_digest(path: &Path, meta: &std::fs::Metadata) -> Result<Option<String>> {
    if !meta.is_file() || meta.len() > CONTENT_STAMP_MAX {
        return Ok(None);
    }
    let data = std::fs::read(path).map_err(io_err(path))?;
    Ok(Some(hex(&Sha256::digest(&data))))
}

/// What a source contributes to the build hash: a small file's digest, else its size and modified time. A
/// folder is its files, which the caller stamps one by one.
fn source_stamp(path: &Path, meta: &std::fs::Metadata, digests: &mut Digests) -> Result<Vec<u8>> {
    if meta.is_dir() {
        return Ok(b"dir".to_vec());
    }
    Ok(match meta.is_file() && meta.len() <= CONTENT_STAMP_MAX {
        true => format!("{}|sha256 {}", meta.len(), digests.sha256(path, meta)?),
        false => format!("{}|{:?}", meta.len(), meta.modified().ok()),
    }
    .into_bytes())
}

/// The digest of everything a build reads; two builds with the same hash make the same overlay.
pub(super) struct BuildHash(Sha256, Digests);

impl BuildHash {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_digests(Digests::none())
    }

    /// A build hash that reuses the small sources' digests in `digests`.
    pub fn with_digests(digests: Digests) -> Self {
        Self(Sha256::new(), digests)
    }

    /// Keeps the small sources' digests for the next build.
    pub fn save_digests(&self) {
        self.1.save();
    }

    #[cfg(test)]
    pub fn digests(&self) -> &Digests {
        &self.1
    }

    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        self.0.update(data);
    }

    pub fn json(&mut self, value: &impl Serialize) {
        self.0.update(serde_json::to_vec(value).expect("serialisable"));
    }

    /// The file at `path` by its size and modified time, or as missing.
    pub fn stamp(&mut self, label: &str, path: &Path) {
        match std::fs::metadata(path) {
            Ok(meta) => self.update(format!("{label}|{}|{:?};", meta.len(), meta.modified().ok())),
            Err(_) => self.update(format!("{label}|missing;")),
        }
    }

    pub fn stamp_tree(&mut self, label: &str, path: &Path) -> Result<()> {
        if path.is_dir() {
            for (rel, p) in loader_files(path)? {
                self.stamp(&format!("{label}/{rel}"), &p);
            }
        } else {
            self.stamp(label, path);
        }
        Ok(())
    }

    /// The files `ops` of `addon` read, by size and modified time.
    pub fn sources(&mut self, store: &Store, addon: &str, ops: &[AssetOp]) -> Result<()> {
        crate::diagnostics::phases::time_part("build.hash_sources", || self.sources_of(store, addon, ops))
    }

    fn sources_of(&mut self, store: &Store, addon: &str, ops: &[AssetOp]) -> Result<()> {
        // Many resource edits read the same archive or directory. Inspect each source
        // once per batch, but feed every occurrence into the digest in its original order.
        // Keep this local: a helper can change cache/data sources before the next batch.
        let mut stamps = HashMap::<PathBuf, Vec<u8>>::new();
        for src in ops.iter().flat_map(AssetOp::sources) {
            let path = src.resolve(&super::source_dir(store, addon, src.root))?;
            if !stamps.contains_key(&path) {
                let meta = std::fs::metadata(&path).map_err(io_err(&path))?;
                let mut stamp = source_stamp(&path, &meta, &mut self.1)?;
                if meta.is_dir() {
                    for (rel, p) in loader_files(&path)? {
                        let meta = std::fs::metadata(&p).map_err(io_err(&p))?;
                        stamp.extend_from_slice(rel.as_bytes());
                        stamp.extend_from_slice(b"|");
                        stamp.extend_from_slice(&source_stamp(&p, &meta, &mut self.1)?);
                    }
                }
                stamps.insert(path.clone(), stamp);
            }
            self.update(src.path.as_bytes());
            self.update(b"|");
            self.update(&stamps[&path]);
        }
        Ok(())
    }

    /// What a build depends on without reading it through an op: the game and pipeline versions, the
    /// loader's features, addon plugins and the game archives ERP ops change.
    pub fn unread(
        &mut self,
        store: &Store,
        game: &Game,
        order: &[&AddonManifest],
        erp_ops: &[Op],
        resolve: &dyn Fn(&str) -> PathBuf,
    ) -> Result<()> {
        crate::diagnostics::phases::time_part("build.hash_unread", || {
            self.unread_of(store, game, order, erp_ops, resolve)
        })
    }

    fn unread_of(
        &mut self,
        store: &Store,
        game: &Game,
        order: &[&AddonManifest],
        erp_ops: &[Op],
        resolve: &dyn Fn(&str) -> PathBuf,
    ) -> Result<()> {
        self.update(format!("game {};", game.identity.key()));
        self.update(format!("pipeline {};", env!("SWOFTY_PIPELINE")));
        if let Some(dir) = loader_dir() {
            self.stamp("loader features", &dir.join(crate::planner::FEATURES_FILE));
        }
        for m in order.iter().filter(|m| !store.is_builtin(&m.id)) {
            let dir = store.addons_dir().join(&m.id);
            for n in &m.operations.native {
                for rel in std::iter::once(&n.dll).chain(&n.files) {
                    let rel = rel.trim_end_matches('/');
                    self.stamp_tree(&format!("{}/{rel}", m.id), &dir.join(rel))?;
                }
            }
        }
        let read: BTreeSet<&str> = erp_ops
            .iter()
            .filter_map(|op| match &op.op {
                AssetOp::Erp(e) => Some(reads(e)),
                _ => None,
            })
            .flatten()
            .collect();
        for rel in read {
            self.stamp(rel, &resolve(rel));
        }
        Ok(())
    }

    pub fn finalize(self) -> sha2::digest::Output<Sha256> {
        self.0.finalize()
    }

    /// The build hash an overlay folder is named by.
    pub fn finish(self) -> String {
        hex(&self.finalize())[..16].to_owned()
    }
}
