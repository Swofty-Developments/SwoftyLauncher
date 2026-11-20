//! F1 25's profile and career saves: launcher saves, the career vault, old backups, saves that stall
//! loading, saves stuck in a started race weekend, and addons that affected them.

pub mod backup;
pub mod bag;
pub mod codec;
pub mod launcher;
pub mod plain;
pub mod remap;
pub mod stall;
pub mod stuck;
pub mod vault;
pub mod warning;

pub use warning::{BODY, SavesWarning, TITLE};
