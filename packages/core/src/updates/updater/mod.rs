use std::time::Duration;

mod apply;
mod catalog;
mod plan;
mod probe;
mod track;

pub use apply::{Ran, Step, did_not_take, install, run};
pub use catalog::{Candidate, Catalog, LIST_RECHECK, check_importer};
pub use plan::{Gate, Phase, Plan, describe, follow_up, gate, plan, summary};
pub use probe::{Part, Probe, probe, read_probe, tag_from_location};
pub use track::{Track, check};

pub const CHECK_INTERVAL: Duration = Duration::from_secs(60);
pub const RELEASE_RECHECK: Duration = Duration::from_secs(5 * 60);
pub const UPDATED_ENV: &str = "SWOFTY_UPDATED_TO";

#[cfg(test)]
mod tests;
