pub mod asset;
pub mod builder;
pub mod game;
pub mod helper;
pub mod launcher;
pub mod manifest;
pub mod ops;
pub mod option;
pub mod run_helper;
pub mod view;

pub const GAME_ID: &str = game::F1_25.id;
pub const CORE_ID: &str = "swofty.core";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("{0}")]
    Resolve(String),
    #[error("no section {0}")]
    NoSection(String),
    #[error("no command {0}")]
    NoCommand(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
