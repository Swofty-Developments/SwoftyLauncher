//! Content addons deliver beyond database rows: track packages, package edits, textures, shaders,
//! post-process passes and the assets the core loads at runtime.

pub mod pkg_edit;
pub mod postfx;
pub mod runtime_assets;
pub mod shaders;
pub mod texture_check;
pub mod track_package;
