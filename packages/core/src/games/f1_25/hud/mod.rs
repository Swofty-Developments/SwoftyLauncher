//! The HUD composer: every enabled addon's `hud.element` records, composed per dial into one set of
//! ready overlay archives (`docs/reference/ops/hud.md`).
//!
//! One scene growth, one template pass and one string table per dial, whatever the number of
//! addons, with a unique `<addon>_` prefix on every element and template node. Whole-file overlays
//! of the dial scenes and the template would overwrite each other; this does not.
//!
//! Parts of the pipeline are ported from Project Halo's `halo_native.py` and `build_native.py`; see
//! the headers of `scene.rs`, `template.rs` and `editor.rs`. Uses work from Project Halo by
//! nyannoying (https://github.com/nyannoying1337/f1-25-project-halo).

pub mod check;
pub mod conflict;
pub mod dials;
pub mod editor;
pub mod geom;
pub mod integrate;
pub mod model;
pub mod motion;
pub mod scene;
pub mod screen_build;
pub mod screens;
pub mod slots;
pub mod stock_edit;
pub mod template;
pub mod textures;
pub mod units;
pub mod walk;
pub mod yields;

use std::path::{Path, PathBuf};

use swofty_addon::asset::{Root, Source};
use swofty_addon::ops::ErpOp;

use model::HudElement;

use crate::{Error, Result};

mod composer;
mod inputs;
mod sprites;
mod stock;

use composer::first;
pub use composer::{archive, clip_resources, compose, compose_with, cover_carriers};
pub use inputs::{inputs_of, prefix_for, read, shared_records};
use sprites::art_bounds;
pub use stock::{GameStock, Layered, Stock};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

/// An edit another part of the launcher makes to the HUD template, applied to the template before the
/// composer adds its nodes (the ADDONS MFD page).
pub type TemplateBase<'a> = &'a dyn Fn(&[u8]) -> Result<Vec<u8>>;

/// One record to compose.
#[derive(Debug, Clone)]
pub struct Input {
    pub addon: String,
    pub key: String,
    pub dir: PathBuf,
    pub record: HudElement,
}

/// A ready archive and the game archives it is overlaid on.
#[derive(Debug, Clone)]
pub struct Built {
    pub file: String,
    pub erps: Vec<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub archives: Vec<Built>,
    pub lines: Vec<String>,
}

impl Output {
    /// The overlay ops that put the archives written to `dir` (a folder of the cache root) in the game.
    pub fn ops(&self, dir: &str) -> Vec<ErpOp> {
        self.archives
            .iter()
            .map(|a| ErpOp::Overlay {
                erps: a.erps.clone(),
                source: Source {
                    root: Root::Cache,
                    path: format!("{dir}/{}", a.file),
                },
            })
            .collect()
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        for a in &self.archives {
            let p = dir.join(&a.file);
            std::fs::write(&p, &a.bytes).map_err(|e| Error::Io { path: p, source: e })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
