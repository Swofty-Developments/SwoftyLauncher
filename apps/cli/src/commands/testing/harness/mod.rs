mod agent_steps;
mod sandboxes;
mod steps;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde_json::{Value, json};
use swofty_core::builtin::CORE_ID;
use swofty_core::session::{LOADER_LOG, LogMark, SessionRecord};
use swofty_core::store::Store;
use swofty_core::testsuite::agent::Outcome;
use swofty_core::testsuite::bench;
use swofty_core::testsuite::checks::{self, Evidence};
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::plan::{Batch, Plan};
use swofty_core::testsuite::report::{BatchReport, Report, Status, TestReport};
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::suite::{self, Kind};

use crate::commands::testing::harness::sandboxes::Staged;
use crate::commands::testing::launcher::Launcher;
use crate::commands::testing::preflight::{batch_fixture, sandboxed};
use crate::commands::testing::say;
use crate::commands::testing::shell::until_polled;

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
    /// The player's Steam save folder: a sandboxed batch compares its
    /// listing (names, sizes, times; never contents) before and after.
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
    /// same settings, so the build applies what `swofty --profile <base> run --apply` applies; `extra` are
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
        Ok(())
    }

    pub(crate) fn batch(&mut self, index: usize, b: &Batch) -> (BatchReport, Vec<TestReport>) {
        let (mut br, mut reports) = Self::blank(b, index);
        let runnable: Vec<usize> = (0..b.tests.len())
            .filter(|&i| reports[i].reason.is_none())
            .collect();
        if runnable.is_empty() {
            return (br, reports);
        }
        let fail_all = |reports: &mut [TestReport], why: &str| {
            for &i in &runnable {
                reports[i].status = Status::Error;
                reports[i].reason = Some(why.to_owned());
            }
        };
        let dir = self.out.join("batches").join(index.to_string());
        if let Err(e) = std::fs::create_dir_all(&dir) {
            br.error = Some(format!("{}: {e}", dir.display()));
            fail_all(&mut reports, "the batch did not start");
            return (br, reports);
        }
        let sandboxed = sandboxed(b);
        let profile = match sandboxed {
            true => format!("{}-{index}", self.profile),
            false => self.profile.to_owned(),
        };
        if let Err(e) = self.profile(b, &profile) {
            br.error = Some(format!("the test profile: {e:#}"));
            fail_all(&mut reports, "the batch did not start");
            self.drop_profile(sandboxed, &profile, &mut br);
            return (br, reports);
        }
        let tests_need_agent = sandboxed || runnable.iter().any(|&i| b.tests[i].needs_agent());
        let needs_agent = tests_need_agent || b.env.kind == Kind::Boot;
        let mut agent = match needs_agent {
            false => None,
            true => match self.open_run(b, index, &dir, sandboxed) {
                Ok(a) => Some(a),
                Err(e) => {
                    br.error = Some(format!("the test run folder: {e}"));
                    fail_all(&mut reports, "the batch did not start");
                    self.drop_profile(sandboxed, &profile, &mut br);
                    return (br, reports);
                }
            },
        };
        let sandbox = match (sandboxed, agent.as_ref()) {
            (true, Some(a)) => match batch_fixture(b, &self.bases, &fixture::store(&self.store.root))
                .and_then(|f| self.open_sandbox(f, b, &profile, &a.run))
            {
                Ok(s) => {
                    br.notes.push(format!(
                        "save sandbox {} with fixture {}",
                        a.run.saves().display(),
                        s.fixture
                    ));
                    Some(s)
                }
                Err(e) => {
                    br.error = Some(format!("the save sandbox: {e}"));
                    fail_all(&mut reports, "the batch did not start");
                    self.close_run(agent.take(), &mut br);
                    self.drop_profile(sandboxed, &profile, &mut br);
                    return (br, reports);
                }
            },
            _ => None,
        };
        let env = agent.as_ref().map(|a| a.run.env()).unwrap_or_default();
        let mut staged = match Staged::write(self.game, b) {
            Ok(s) => s,
            Err(e) => {
                br.error = Some(format!("could not write the benchmark XML: {e}"));
                fail_all(&mut reports, "the batch did not start");
                if let Some(s) = sandbox {
                    self.close_sandbox(
                        s,
                        agent.as_ref().map(|a| &a.run),
                        b,
                        &profile,
                        &mut br,
                        &mut reports,
                        &runnable,
                    );
                }
                self.close_run(agent.take(), &mut br);
                self.drop_profile(sandboxed, &profile, &mut br);
                return (br, reports);
            }
        };
        let mut live = Live {
            path: self.game.join(LOADER_LOG),
            mark: LogMark::read(&self.game.join(LOADER_LOG)),
            last: String::new(),
        };
        let args = match b.env.kind {
            Kind::Benchmark => bench::args(),
            _ => Vec::new(),
        };
        say(&format!("batch {index}: starting F1 25 ({})", br.env));
        let started = Instant::now();
        let mut run = match self.launcher.launch(&profile, &args, &env, &dir) {
            Ok(r) => r,
            Err(e) => {
                br.error = Some(format!("could not start the game: {e:#}"));
                fail_all(&mut reports, "the batch did not start");
                if let Some(s) = sandbox {
                    self.close_sandbox(
                        s,
                        agent.as_ref().map(|a| &a.run),
                        b,
                        &profile,
                        &mut br,
                        &mut reports,
                        &runnable,
                    );
                }
                self.close_run(agent.take(), &mut br);
                self.drop_profile(sandboxed, &profile, &mut br);
                return (br, reports);
            }
        };
        br.started_game = true;
        let kind = b.env.kind;
        let mut told_title = false;
        let mut pressed: Option<Instant> = None;
        let mut title: Option<u64> = None;
        let mut by_pad = false;
        let ready = loop {
            if let Some(a) = agent.as_mut()
                && let Err(why) = a.keep_up()
            {
                break Err(why);
            }
            let text = live.text();
            if text.contains(checks::ready_line(kind)) {
                break Ok(());
            }
            if agent.is_some()
                && let Some(line) = text.lines().find(|l| l.contains(sandbox::FAILED_LINE))
            {
                break Err(format!("the core refused the test run and the game quit: {line}"));
            }
            let at_title = matches!(kind, Kind::Boot | Kind::Route) && text.contains(checks::TITLE);
            if let Some(a) = agent.as_mut().filter(|a| a.hello && at_title) {
                match title {
                    None => title = a.client.send("title", Value::Null, None).ok(),
                    Some(seq) => {
                        if let Some(r) = a.client.take_reply(seq) {
                            by_pad = r.outcome != Outcome::Ok;
                            if by_pad {
                                say(&format!("title: {}; pressing the virtual pad instead", r.text()));
                            }
                        }
                    }
                }
                if by_pad && pressed.is_none_or(|p| p.elapsed() >= self.times.title_press) {
                    pressed = Some(Instant::now());
                    a.client
                        .send("pad", json!({ "press": "a", "ms": 250 }), None)
                        .ok();
                }
            } else if at_title && agent.is_none() && !told_title {
                told_title = true;
                say("at the title screen: it waits for a button press before the main menu");
            }
            if run.finished() {
                break Err(format!("the game exited before {}", checks::ready_text(kind)));
            }
            if started.elapsed() >= self.times.start {
                break Err(format!(
                    "the game did not reach {} within {} s",
                    checks::ready_text(kind),
                    self.times.start.as_secs()
                ));
            }
            std::thread::sleep(self.times.poll);
        };
        let mut dropped_agent = false;
        let ready = ready.and_then(|()| match agent.as_mut() {
            None => Ok(()),
            Some(a) => {
                let left = self.times.start.saturating_sub(started.elapsed());
                match a.hello(left, self.times.poll, &mut *run) {
                    Err(why) if !tests_need_agent => {
                        br.notes.push(format!("no test agent for this boot batch: {why}"));
                        dropped_agent = true;
                        return Ok(());
                    }
                    other => other?,
                }
                self.env_home(&b.env, a, &mut *run);
                self.env_routes(&b.env, a, &mut *run)?;
                self.env_career(
                    &b.env,
                    sandbox.as_ref().and_then(|s| s.mode.as_deref()),
                    a,
                    &mut *run,
                )?;
                self.env_session(&b.env, a, &mut *run)
            }
        });
        if dropped_agent {
            self.close_run(agent.take(), &mut br);
        }
        match &ready {
            Err(why) => {
                br.error = Some(why.clone());
                fail_all(&mut reports, &format!("the batch did not start: {why}"));
            }
            Ok(()) => {
                br.ready_ms = Some(ms(started));
                say(&format!("batch {index}: ready at {}", checks::ready_text(kind)));
                let race_start = Instant::now();
                for &i in &runnable {
                    let t0 = Instant::now();
                    let t = &b.tests[i];
                    self.steps(t, &mut reports[i], &mut live, &mut *run, agent.as_mut());
                    if reports[i].failures.is_empty()
                        && let (Some(f), Some(a)) = (&t.function, agent.as_mut())
                    {
                        self.function(t, f, &mut reports[i], a, &mut *run);
                    }
                    if let Some(a) = agent.as_mut() {
                        for c in &t.checks {
                            if let suite::Check::Agent { text } = c
                                && let Err(f) = self.agent_check(text, a, &mut *run)
                            {
                                reports[i].failures.push(f);
                            }
                        }
                    }
                    reports[i].ms = Some(ms(t0));
                }
                if kind == Kind::Benchmark {
                    let limit = runnable
                        .iter()
                        .filter_map(|&i| b.tests[i].timeout_s)
                        .max()
                        .map_or(self.times.race, Duration::from_secs);
                    if until_polled(limit, self.times.poll, || run.finished()) {
                        for &i in &runnable {
                            reports[i].ms = Some(ms(race_start));
                        }
                    } else {
                        let why = format!("the benchmark race did not end within {} s", limit.as_secs());
                        br.error = Some(why.clone());
                        fail_all(&mut reports, &why);
                    }
                }
            }
        }
        if let Some(a) = agent.as_mut().filter(|a| a.hello) {
            a.client.send("quit", Value::Null, None).ok();
            until_polled(self.times.quit, self.times.poll, || run.finished());
        }
        if let Err(why) = self.finish(&mut *run) {
            br.notes.push(why);
        }
        br.ms = Some(ms(started));
        if let Some(a) = agent.as_mut() {
            a.client.poll();
            if let Err(e) = a.keep() {
                br.notes
                    .push(format!("could not keep the test agent's results: {e}"));
            }
        }
        let unstaged = staged.as_mut().map(|s| s.remove());
        drop(staged);
        let session = run.session();
        br.session = session.as_ref().map(|s| s.display().to_string());
        let record = session.as_deref().map(SessionRecord::read);
        let final_log = session
            .as_deref()
            .and_then(|s| read(&s.join("loader.log")))
            .unwrap_or_else(|| live.text().to_owned());
        let perf = session.as_deref().and_then(|s| read(&s.join("perf.log")));
        if let Some(s) = &session {
            for f in [
                "loader.log",
                "perf.log",
                "end.json",
                "exit_code",
                "game_stdout.log",
            ] {
                std::fs::copy(s.join(f), dir.join(f)).ok();
            }
        } else {
            std::fs::write(dir.join("loader.log"), &final_log).ok();
        }
        br.end = record
            .as_ref()
            .and_then(|r| r.end.as_ref())
            .map(|e| format!("{}: {}", e.end.as_str(), e.reason));
        let crash = record.as_ref().filter(|r| r.crashed()).map(|r| match &r.end {
            Some(e) => format!("the game crashed ({})", e.reason),
            None => format!("the game crashed (exit code {})", r.exit_code.unwrap_or(-1)),
        });
        br.notes.extend(crash.clone());
        let rel = |f: &str| format!("batches/{index}/{f}");
        let ev = Evidence {
            log: final_log,
            perf,
            crash,
            log_path: Some(rel("loader.log")),
            perf_path: Some(rel("perf.log")),
        };
        for &i in &runnable {
            let r = &mut reports[i];
            if r.status == Status::Error {
                continue;
            }
            for c in &b.tests[i].checks {
                if matches!(c, suite::Check::Agent { .. }) && agent.is_some() {
                    continue;
                }
                if let Err(f) = checks::evaluate(c, &ev) {
                    r.failures.push(f);
                }
            }
            r.status = match r.failures.is_empty() {
                true => Status::Pass,
                false => Status::Fail,
            };
        }
        if let Some(s) = sandbox {
            self.close_sandbox(
                s,
                agent.as_ref().map(|a| &a.run),
                b,
                &profile,
                &mut br,
                &mut reports,
                &runnable,
            );
        }
        self.close_run(agent.take(), &mut br);
        self.drop_profile(sandboxed, &profile, &mut br);
        let (clean, notes) = self.clean(unstaged);
        br.clean = Some(clean);
        br.notes.extend(notes);
        (br, reports)
    }
}
