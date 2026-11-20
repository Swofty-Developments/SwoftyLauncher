use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use swofty_core::install::Source;
use swofty_core::platform::steam::client as steam_client;
use swofty_core::platform::{ClientNote, ClientStart, Steam};
use swofty_core::store::Store;

use crate::console::output::say;
use crate::stores::steam::StageFile;

/// Where a client start's notes go: its lines to the session log under the
/// store's name, its stage to the launcher's status line until the start ends.
pub(crate) struct Notes<L: FnMut(&str, &str)> {
    tag: &'static str,
    stage_path: PathBuf,
    stage: Option<StageFile>,
    log: L,
    /// The program Windows refused to start with error 740.
    refused: Option<PathBuf>,
}

impl<L: FnMut(&str, &str)> Notes<L> {
    pub(crate) fn new(root: &Path, source: Source, log: L) -> Self {
        Self {
            tag: match source {
                Source::EaApp => "ea app",
                _ => "steam",
            },
            stage_path: root.join(steam_client::STAGE_FILE),
            stage: None,
            log,
            refused: None,
        }
    }

    pub(crate) fn note(&mut self, note: ClientNote) {
        match note {
            ClientNote::Log(line) => (self.log)(self.tag, &line),
            ClientNote::Stage(text) => self
                .stage
                .get_or_insert_with(|| StageFile(self.stage_path.clone()))
                .set(text),
            ClientNote::RunAsAdmin(program) => self.refused = Some(program),
        }
    }
}

/// A copy of the game needs its store's client up: Steam signed in for a
/// Steam copy, the EA app running for an EA app copy. Start it when it is
/// closed, as the store's own Play button would. A folder no store installed
/// is checked as Steam's.
pub(crate) fn ensure_client(store: &Store, source: Source, game: &Path, opted_out: bool) -> Result<()> {
    let platform = source.platform().unwrap_or(&Steam);
    let config = store.config();
    let start = ClientStart {
        game,
        allowed: config.launch.start_store && !opted_out,
        ea_command: config.launch.ea_launch_command.as_deref(),
    };
    let mut notes = Notes::new(&store.root, platform.source(), say);
    let ready = platform.ensure_client(start, &mut |n| notes.note(n));
    let refused = notes.refused.take();
    drop(notes);
    match (ready, refused) {
        (Ok(_), _) => Ok(()),
        (Err(_), Some(program)) => Err(crate::stores::ea::refused_program(store, &program)),
        (Err(e), None) => Err(anyhow!(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_start_logs_under_its_store_and_clears_its_status_line() {
        let root = std::env::temp_dir().join(format!("swofty-client-notes-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let stage = root.join(steam_client::STAGE_FILE);
        let mut lines = Vec::new();
        for (source, line) in [
            (Source::Steam, "detected: not running"),
            (Source::Unknown, "detected: not running"),
            (Source::EaApp, "detected: closed; starting the EA app"),
        ] {
            let mut notes = Notes::new(&root, source, |tag: &str, text: &str| {
                lines.push(format!("[{tag}] {text}"))
            });
            notes.note(ClientNote::Log(line.into()));
            assert!(!stage.exists());
            notes.note(ClientNote::Stage("Starting the EA app…"));
            assert_eq!(std::fs::read_to_string(&stage).unwrap(), "Starting the EA app…");
            drop(notes);
            assert!(!stage.exists());
        }
        assert_eq!(
            lines,
            [
                "[steam] detected: not running",
                "[steam] detected: not running",
                "[ea app] detected: closed; starting the EA app",
            ]
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
