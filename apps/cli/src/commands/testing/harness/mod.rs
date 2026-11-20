mod agent_steps;
mod batch;
mod sandboxes;
mod steps;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use swofty_core::addons::builtin::CORE_ID;
use swofty_core::session::LogMark;
use swofty_core::store::Store;
use swofty_core::testsuite::plan::{Batch, Plan};
use swofty_core::testsuite::report::{BatchReport, Report, Status, TestReport};

use crate::commands::testing::launcher::Launcher;

pub(crate) use sandboxes::BatchResults;
#[cfg(test)]
pub(crate) use sandboxes::career_mode;

pub(crate) struct Times {
    pub start: Duration,
    pub step: Duration,
    pub test: Duration,
    pub race: Duration,
    pub finish: Duration,
    pub restop: Duration,
    pub poll: Duration,
    /// How long past a command's own time limit the CLI waits for its answer.
    pub grace: Duration,
    /// How long the game gets to close after the agent's quit.
    pub quit: Duration,
    /// How often the title screen gets a pad press when the agent can't
    /// pass it without input.
    pub title_press: Duration,
    /// How long a career environment's `career load` may take.
    pub career: Duration,
}

impl Default for Times {
    fn default() -> Self {
        Times {
            start: Duration::from_secs(900),
            step: Duration::from_secs(30),
            test: Duration::from_secs(300),
            race: Duration::from_secs(900),
            finish: Duration::from_secs(600),
            restop: Duration::from_secs(10),
            poll: Duration::from_millis(250),
            grace: Duration::from_secs(10),
            quit: Duration::from_secs(30),
            title_press: Duration::from_secs(3),
            career: Duration::from_secs(900),
        }
    }
}

pub(crate) struct Harness<'a> {
    pub store: &'a Store,
    pub game: &'a Path,
    pub out: &'a Path,
    pub profile: &'a str,
    pub launcher: &'a mut dyn Launcher,
    pub times: Times,
    pub run_id: &'a str,
    /// Each suite's test file folder, for fixture paths.
    pub bases: BTreeMap<String, PathBuf>,
    /// The game exe's sha256 when a batch needs the save sandbox.
    pub exe_sha: Option<String>,
    /// The player's own save folder (Steam's, or the EA copy's profile
    /// folder): a sandboxed batch compares its listing (names, sizes, times;
    /// never contents) before and after.
    pub live_saves: Option<PathBuf>,
}

pub(crate) fn ms(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

pub(crate) fn read(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub(crate) fn until_polled(limit: Duration, poll: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(poll);
    }
}

/// The live loader.log once this session's game has replaced the last one.
pub(crate) struct Live {
    pub(crate) path: PathBuf,
    pub(crate) mark: Option<LogMark>,
    pub(crate) last: String,
}

impl Live {
    pub(crate) fn text(&mut self) -> &str {
        if LogMark::replaced(&self.path, self.mark.as_ref())
            && let Some(t) = read(&self.path)
            && t.len() >= self.last.len()
        {
            self.last = t;
        }
        &self.last
    }
}

impl Harness<'_> {
    pub(crate) fn blank(b: &Batch, index: usize) -> (BatchReport, Vec<TestReport>) {
        let label = b.label();
        let batch = BatchReport {
            index,
            env: label.clone(),
            key: b.key.clone(),
            addons: b.addons.clone(),
            started_game: false,
            ready_ms: None,
            ms: None,
            session: None,
            end: None,
            error: None,
            clean: None,
            notes: Vec::new(),
        };
        let tests = b
            .tests
            .iter()
            .map(|t| TestReport {
                suite: t.suite.clone(),
                id: t.id.clone(),
                env: label.clone(),
                batch: index,
                status: Status::Skipped,
                ms: None,
                reason: t.skip_reason().map(|r| r.to_owned()),
                failures: Vec::new(),
                steps: Vec::new(),
            })
            .collect();
        (batch, tests)
    }

    pub(crate) fn skip_all(plan: &Plan, report: &mut Report) {
        for (i, b) in plan.batches.iter().enumerate() {
            let (batch, tests) = Self::blank(b, i + 1);
            report.batches.push(batch);
            report.tests.extend(tests);
        }
    }

    pub(crate) fn not_run(plan: &Plan, report: &mut Report, why: &str) {
        Self::skip_all(plan, report);
        for t in report.tests.iter_mut().filter(|t| t.reason.is_none()) {
            t.status = Status::NotRun;
            t.reason = Some(why.to_owned());
        }
    }

    pub fn run(&mut self, plan: &Plan, report: &mut Report) {
        for (i, b) in plan.batches.iter().enumerate() {
            let (batch, tests) = self.batch(i + 1, b);
            report.batches.push(batch);
            report.tests.extend(tests);
        }
    }

    pub(crate) fn profile(&self, b: &Batch, name: &str) -> Result<()> {
        let installed = self.store.addons()?;
        let registry = self.store.registry()?;
        let mut p = self.store.profile(name)?;
        let keep = |id: &str| {
            id == CORE_ID || b.addons.iter().any(|a| a == id) || registry.get(id).is_some_and(|e| e.locked)
        };
        let listed: Vec<String> = p.addons.iter().map(|a| a.id.clone()).collect();
        for id in listed {
            p.set_enabled(&id, keep(&id))?;
        }
        for id in &b.addons {
            if !installed.contains_key(id) {
                bail!("{id} is not installed: install it with swofty addons install <folder>");
            }
            p.set_enabled(id, true)?;
        }
        p.options.clear();
        for (id, options) in &b.settings {
            let Some(m) = installed.get(id) else {
                bail!("the settings name {id}, which is not installed");
            };
            for (option, value) in options {
                p.set_option(m, option, value.clone())?;
            }
        }
        self.store.save_profile(&p)?;
        Ok(())
    }

    /// The shell's profile as a copy of `base`: the same addons in the same order, on or off as there, with the
    /// same settings and a copy of its saves folder (launcher saves and plain store), so the build applies what
    /// `swofty --profile <base> run --apply` applies and the game loads the careers `base` plays; `extra` are
    /// switched on as well.
    pub(crate) fn profile_from(&self, base: &str, name: &str, extra: &[String]) -> Result<()> {
        if !self.store.profile_names()?.iter().any(|n| n == base) {
            bail!("there is no profile {base}: swofty profile list shows them");
        }
        let installed = self.store.addons()?;
        let mut p = self.store.profile(base)?;
        p.name = name.to_owned();
        for id in extra {
            if !installed.contains_key(id) {
                bail!("{id} is not installed: install it with swofty addons install <folder>");
            }
            p.set_enabled(id, true)?;
        }
        self.store.save_profile(&p)?;
        let (from, to) = (self.store.saves_dir(base), self.store.saves_dir(name));
        if from.is_dir() && !to.exists() {
            sandboxes::copy_tree(&from, &to)
                .map_err(|e| anyhow::anyhow!("copying {} to {}: {e}", from.display(), to.display()))?;
            crate::commands::testing::say(&format!(
                "saves: a copy of {base}'s saves folder ({})",
                to.display()
            ));
        }
        Ok(())
    }
}
