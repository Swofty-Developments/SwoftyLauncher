pub(crate) mod addons;
pub(crate) mod build;
pub(crate) mod cli;
pub(crate) mod db;
pub(crate) mod dev;
pub(crate) mod doctor;
pub(crate) mod game;
pub(crate) mod games;
pub(crate) mod hud;
pub(crate) mod laps;
pub(crate) mod logs;
pub(crate) mod offline;
pub(crate) mod postfx;
pub(crate) mod profile;
pub(crate) mod registry;
pub(crate) mod run;
pub(crate) mod saves;
pub(crate) mod settings;
pub(crate) mod setup;
pub(crate) mod shaders;
pub(crate) mod steam;
pub(crate) mod testing;
pub(crate) mod update;

use std::process::ExitCode;

use anyhow::Result;
use swofty_core::store::Store;

pub(crate) struct Invocation {
    pub(crate) store: Store,
    pub(crate) profile: String,
    pub(crate) named_profile: Option<String>,
}

pub(crate) trait Execute {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode>;
}
