use std::time::{Duration, Instant};

use swofty_core::game_folder::journal::Journal;
use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::testsuite::bench;
use swofty_core::testsuite::checks::{self, Failure};
use swofty_core::testsuite::report::{StepReport, TestReport};
use swofty_core::testsuite::suite::{self, Test};

use crate::commands::testing::harness::agent_steps::Agent;
use crate::commands::testing::harness::{Harness, Live, ms};
use crate::commands::testing::launcher::Running;

impl Harness<'_> {
    pub(crate) fn steps(
        &self,
        t: &Test,
        r: &mut TestReport,
        live: &mut Live,
        run: &mut dyn Running,
        mut agent: Option<&mut Agent>,
    ) {
        let test_limit = t.timeout_s.map_or(self.times.test, Duration::from_secs);
        let started = Instant::now();
        for s in &t.steps {
            let limit = s
                .timeout_s
                .map_or(self.times.step, Duration::from_secs)
                .min(test_limit.saturating_sub(started.elapsed()));
            if let Some(suite::Wait::Ms(n)) = s.host {
                let t0 = Instant::now();
                let span = Duration::from_millis(n).min(test_limit.saturating_sub(started.elapsed()));
                while t0.elapsed() < span && !run.finished() {
                    std::thread::sleep(self.times.poll.min(span - t0.elapsed()));
                }
                r.steps.push(StepReport {
                    step: s.text.clone(),
                    status: "ok".to_owned(),
                    ms: Some(ms(t0)),
                });
                continue;
            }
            let by_agent =
                s.host.is_none() || matches!(s.host, Some(suite::Wait::Event(_))) && agent.is_some();
            if by_agent {
                let Some(a) = agent.as_deref_mut() else {
                    r.steps.push(StepReport {
                        step: s.text.clone(),
                        status: format!("skipped: {}", suite::NEEDS_AGENT),
                        ms: None,
                    });
                    continue;
                };
                let t0 = Instant::now();
                let done = self.agent_step(s, limit, a, run);
                r.steps.push(StepReport {
                    step: s.text.clone(),
                    status: match &done {
                        Ok(text) => text.clone(),
                        Err(f) => f.actual.clone(),
                    },
                    ms: Some(ms(t0)),
                });
                if let Err(f) = done {
                    r.failures.push(f);
                    return;
                }
                continue;
            }
            let Some(wait) = &s.host else { continue };
            let t0 = Instant::now();
            let seen = loop {
                if checks::waited(live.text(), wait) {
                    break true;
                }
                if t0.elapsed() >= limit || run.finished() {
                    break checks::waited(live.text(), wait);
                }
                std::thread::sleep(self.times.poll);
            };
            r.steps.push(StepReport {
                step: s.text.clone(),
                status: if seen { "ok" } else { "timed out" }.to_owned(),
                ms: Some(ms(t0)),
            });
            if !seen {
                let why = if run.finished() {
                    "none before the game exited".to_owned()
                } else {
                    format!("none within {} s", limit.as_secs())
                };
                r.failures.push(Failure {
                    check: format!("wait {}", checks::wait_text(wait)),
                    expected: checks::wait_text(wait),
                    actual: why,
                    evidence: None,
                });
                return;
            }
        }
    }

    /// Close the game unless it has closed, and wait for the session to put
    /// the game files back, asking again now and then in case the game
    /// started late.
    pub(crate) fn finish(&self, run: &mut dyn Running) -> std::result::Result<(), String> {
        let start = Instant::now();
        let mut asked: Option<Instant> = None;
        while !run.finished() {
            if start.elapsed() >= self.times.finish {
                run.kill();
                return Err(format!(
                    "the session did not end within {} s of being asked to; its launcher was stopped, so run swofty \
                     restore if the game folder is not stock",
                    self.times.finish.as_secs()
                ));
            }
            if asked.is_none_or(|a| a.elapsed() >= self.times.restop) {
                run.stop();
                asked = Some(Instant::now());
            }
            std::thread::sleep(self.times.poll);
        }
        Ok(())
    }

    /// The game folder is back as it was: nothing staged is left, the
    /// session lock is free and no session journal is open.
    pub(crate) fn clean(&self, unstaged: Option<std::io::Result<()>>) -> (bool, Vec<String>) {
        let mut notes = Vec::new();
        if let Some(Err(e)) = unstaged {
            notes.push(format!("could not remove the benchmark XML: {e}"));
        }
        let xml = self.game.join(bench::DIR).join(bench::FILE);
        if xml.exists() {
            notes.push(format!("{} is still in the game folder", xml.display()));
        }
        let data = &self.store.root;
        if SessionLock::held_elsewhere(data) {
            notes.push("the session is still putting the game files back (its session lock is held)".into());
        }
        match Journal::open(data) {
            Ok(open) => {
                let n = open.iter().filter(|j| j.session).count();
                if n > 0 {
                    notes.push(format!(
                        "{n} session journal(s) are still open: the game files were not all put back; run swofty restore"
                    ));
                }
            }
            Err(e) => notes.push(format!("could not read the install journals: {e}")),
        }
        if swofty_core::offline::journal_path(data).exists() {
            notes.push("the offline play staging is still in the game folder; run swofty restore".into());
        }
        (notes.is_empty(), notes)
    }
}
