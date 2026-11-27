use std::time::{Instant, SystemTime};

use swofty_core::diagnostics::quarantine::{self, Attach, AttachWatch};
use swofty_core::install::{Game, PROXY};
use swofty_core::loader_check;
use swofty_core::store::Store;

use crate::commands::run::wrapper::loader::{loader_missed, note_refused};
use crate::commands::run::wrapper::play::SessionDir;
use crate::console::output::say;

/// Whether the loader's version.dll attached to the game, watched while it
/// runs and settled when it ends.
pub(super) struct LoaderWatch<'a> {
    store: &'a Store,
    g: &'a Game,
    session: &'a SessionDir,
    launched: SystemTime,
    on: bool,
    watch: Option<AttachWatch>,
    placed: Option<String>,
    early: Option<Vec<loader_check::Seen>>,
}

impl<'a> LoaderWatch<'a> {
    pub(super) fn new(
        store: &'a Store,
        g: &'a Game,
        session: &'a SessionDir,
        launched: SystemTime,
        on: bool,
    ) -> Self {
        let mut watch = Self {
            store,
            g,
            session,
            launched,
            on,
            watch: None,
            placed: None,
            early: None,
        };
        watch.restart();
        watch.placed = quarantine::placed_dlls(&store.root)
            .unwrap_or_default()
            .into_iter()
            .find_map(|(rel, sha)| rel.eq_ignore_ascii_case(PROXY).then_some(sha));
        watch
    }

    pub(super) fn restart(&mut self) {
        self.watch = self.on.then(|| {
            AttachWatch::new(
                self.session.loader.clone(),
                self.session.before.clone(),
                Instant::now(),
            )
            .with_fallback(self.session.fallback.clone())
        });
    }

    pub(super) fn tick(&mut self) {
        let probe = match self.watch.as_mut() {
            Some(w) if w.state() == Attach::Waiting => w.probe_due(Instant::now()),
            _ => return,
        };
        if probe {
            self.early = Some(self.collect().games);
        }
        if let Some(state) = self.watch.as_mut().map(|w| w.poll(Instant::now())) {
            self.verdict(state);
        }
    }

    pub(super) fn ended(&mut self) {
        let state = match self.watch.as_mut() {
            Some(w) if w.state() == Attach::Waiting => w.ended(Instant::now()),
            _ => return,
        };
        self.verdict(state);
    }

    pub(super) fn verdict(&mut self, state: Attach) {
        match state {
            Attach::Missed => self.missed(),
            Attach::Attached => note_refused(&self.session.fallback),
            Attach::Waiting => {}
        }
    }

    pub(super) fn collect(&self) -> loader_check::Findings {
        loader_check::collect(
            &self.g.dir,
            self.placed.as_deref(),
            self.launched,
            self.g.user_reg.as_deref(),
        )
    }

    pub(super) fn missed(&mut self) {
        let game = self.g.dir.as_path();
        let mut found = self.collect();
        if found.games.is_empty()
            && let Some(early) = self.early.take()
        {
            found.games = early;
            say(
                "loader",
                &format!(
                    "  the game had closed by now; the game lines below are from a look {} s after it started",
                    quarantine::PROBE_AFTER.as_secs()
                ),
            );
        }
        let running = found.attached_text();
        say(
            "loader",
            match running {
                Some(_) => {
                    "version.dll attached to F1 25, which the session's watch missed; what the session found:"
                }
                None => "version.dll did not attach to F1 25; what the session found:",
            },
        );
        for line in found.lines(game) {
            say("loader", &format!("  {line}"));
        }
        if let Some(text) = running {
            say("info", text);
            return;
        }
        if found.rotated {
            say(
                "info",
                "The mod loader ran in F1 25: it moved its log aside after the game started.",
            );
            return;
        }
        // The antivirus explanation is a Windows one; under Proton a loader
        // that does not attach is a prefix or override problem instead.
        if cfg!(windows) {
            loader_missed(self.store, game, found.cause(game));
        }
    }
}
