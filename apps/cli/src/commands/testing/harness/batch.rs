use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use swofty_core::session::{LOADER_LOG, LogMark, SessionRecord};
use swofty_core::testsuite::agent::Outcome;
use swofty_core::testsuite::bench;
use swofty_core::testsuite::checks::{self, Evidence};
use swofty_core::testsuite::fixture;
use swofty_core::testsuite::plan::Batch;
use swofty_core::testsuite::report::{BatchReport, Status, TestReport};
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::suite::{self, Kind};

use crate::commands::testing::harness::agent_steps::Agent;
use crate::commands::testing::harness::sandboxes::{BatchResults, Sandbox, Staged};
use crate::commands::testing::harness::{Harness, Live, ms, read, until_polled};
use crate::commands::testing::launcher::Running;
use crate::commands::testing::preflight::{batch_fixture, sandboxed};
use crate::commands::testing::say;

/// A batch's reports while it runs, and which of its tests run.
struct BatchRun<'b> {
    b: &'b Batch,
    index: usize,
    br: BatchReport,
    reports: Vec<TestReport>,
    runnable: Vec<usize>,
}

impl<'b> BatchRun<'b> {
    fn new(b: &'b Batch, index: usize) -> Self {
        let (br, reports) = Harness::blank(b, index);
        let runnable = (0..b.tests.len())
            .filter(|&i| reports[i].reason.is_none())
            .collect();
        BatchRun {
            b,
            index,
            br,
            reports,
            runnable,
        }
    }

    fn fail_all(&mut self, why: &str) {
        for &i in &self.runnable {
            self.reports[i].status = Status::Error;
            self.reports[i].reason = Some(why.to_owned());
        }
    }

    fn did_not_start(&mut self, error: String) {
        self.br.error = Some(error);
        self.fail_all("the batch did not start");
    }

    /// Each test that did not error passes or fails on its log checks; the agent's own checks already ran
    /// when an agent ran them.
    fn grade(&mut self, ev: &Evidence, agent_checked: bool) {
        for &i in &self.runnable {
            let r = &mut self.reports[i];
            if r.status == Status::Error {
                continue;
            }
            for c in &self.b.tests[i].checks {
                if matches!(c, suite::Check::Agent { .. }) && agent_checked {
                    continue;
                }
                if let Err(f) = checks::evaluate(c, ev) {
                    r.failures.push(f);
                }
            }
            r.status = match r.failures.is_empty() {
                true => Status::Pass,
                false => Status::Fail,
            };
        }
    }

    fn done(self) -> (BatchReport, Vec<TestReport>) {
        (self.br, self.reports)
    }
}

/// What a batch sets up before the game starts: its profile, the agent's test run folder and the save
/// sandbox, closed in reverse when the batch ends.
struct Opened {
    sandboxed: bool,
    profile: String,
    tests_need_agent: bool,
    agent: Option<Agent>,
    sandbox: Option<Sandbox>,
}

impl Harness<'_> {
    pub(crate) fn batch(&mut self, index: usize, b: &Batch) -> (BatchReport, Vec<TestReport>) {
        let mut batch = BatchRun::new(b, index);
        if batch.runnable.is_empty() {
            return batch.done();
        }
        let dir = self.out.join("batches").join(index.to_string());
        if let Err(e) = std::fs::create_dir_all(&dir) {
            batch.did_not_start(format!("{}: {e}", dir.display()));
            return batch.done();
        }
        let Some(mut opened) = self.open(&mut batch, &dir) else {
            return batch.done();
        };
        let env = opened.agent.as_ref().map(|a| a.run.env()).unwrap_or_default();
        let mut staged = match Staged::write(self.game, b) {
            Ok(s) => s,
            Err(e) => {
                batch.did_not_start(format!("could not write the benchmark XML: {e}"));
                self.close(opened, &mut batch);
                return batch.done();
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
        say(&format!("batch {index}: starting F1 25 ({})", batch.br.env));
        let started = Instant::now();
        let mut run = match self.launcher.launch(&opened.profile, &args, &env, &dir) {
            Ok(r) => r,
            Err(e) => {
                batch.did_not_start(format!("could not start the game: {e:#}"));
                self.close(opened, &mut batch);
                return batch.done();
            }
        };
        batch.br.started_game = true;
        match self.reach_ready(&mut batch, &mut opened, &mut live, &mut *run, started) {
            Err(why) => {
                batch.br.error = Some(why.clone());
                batch.fail_all(&format!("the batch did not start: {why}"));
            }
            Ok(()) => {
                batch.br.ready_ms = Some(ms(started));
                say(&format!(
                    "batch {index}: ready at {}",
                    checks::ready_text(b.env.kind)
                ));
                self.run_tests(&mut batch, opened.agent.as_mut(), &mut live, &mut *run);
            }
        }
        self.stop_game(&mut batch.br, opened.agent.as_mut(), &mut *run);
        batch.br.ms = Some(ms(started));
        keep_agent_results(&mut batch.br, opened.agent.as_mut());
        let unstaged = staged.as_mut().map(|s| s.remove());
        drop(staged);
        let ev = evidence(&mut batch.br, index, &dir, &mut *run, &mut live);
        batch.grade(&ev, opened.agent.is_some());
        self.close(opened, &mut batch);
        let (clean, notes) = self.clean(unstaged);
        batch.br.clean = Some(clean);
        batch.br.notes.extend(notes);
        batch.done()
    }

    /// The batch's profile, the agent's test run folder when the batch needs the agent, and the save sandbox
    /// when it is sandboxed. `None` when one of them fails, with the batch's tests failed and what was already
    /// set up closed.
    fn open(&self, batch: &mut BatchRun, dir: &Path) -> Option<Opened> {
        let b = batch.b;
        let sandboxed = sandboxed(b);
        let mut opened = Opened {
            sandboxed,
            profile: match sandboxed {
                true => format!("{}-{}", self.profile, batch.index),
                false => self.profile.to_owned(),
            },
            tests_need_agent: sandboxed || batch.runnable.iter().any(|&i| b.tests[i].needs_agent()),
            agent: None,
            sandbox: None,
        };
        if let Err(e) = self.profile(b, &opened.profile) {
            batch.did_not_start(format!("the test profile: {e:#}"));
            self.close(opened, batch);
            return None;
        }
        if opened.tests_need_agent || b.env.kind == Kind::Boot {
            match self.open_run(b, batch.index, dir, sandboxed) {
                Ok(a) => opened.agent = Some(a),
                Err(e) => {
                    batch.did_not_start(format!("the test run folder: {e}"));
                    self.close(opened, batch);
                    return None;
                }
            }
        }
        if let (true, Some(a)) = (sandboxed, opened.agent.as_ref()) {
            match batch_fixture(b, &self.bases, &fixture::store(&self.store.root))
                .and_then(|f| self.open_sandbox(f, b, &opened.profile, &a.run))
            {
                Ok(s) => {
                    batch.br.notes.push(format!(
                        "save sandbox {} with fixture {}",
                        a.run.saves().display(),
                        s.fixture
                    ));
                    opened.sandbox = Some(s);
                }
                Err(e) => {
                    batch.did_not_start(format!("the save sandbox: {e}"));
                    self.close(opened, batch);
                    return None;
                }
            }
        }
        Some(opened)
    }

    /// Closes what `open` set up, in reverse: the save sandbox, the test run folder, then the batch's profile.
    fn close(&self, opened: Opened, batch: &mut BatchRun) {
        let Opened {
            sandboxed,
            profile,
            agent,
            sandbox,
            ..
        } = opened;
        if let Some(s) = sandbox {
            self.close_sandbox(
                s,
                agent.as_ref().map(|a| &a.run),
                batch.b,
                &profile,
                BatchResults {
                    br: &mut batch.br,
                    reports: &mut batch.reports,
                    runnable: &batch.runnable,
                },
            );
        }
        self.close_run(agent, &mut batch.br);
        self.drop_profile(sandboxed, &profile, &mut batch.br);
    }

    /// Waits for the game to reach the batch's ready line, then has the agent say hello and take the game to
    /// where the batch's environment starts. A boot batch whose tests do not need the agent goes on without
    /// one that does not answer.
    fn reach_ready(
        &self,
        batch: &mut BatchRun,
        opened: &mut Opened,
        live: &mut Live,
        run: &mut dyn Running,
        started: Instant,
    ) -> Result<(), String> {
        self.wait_for_ready(batch.b.env.kind, opened.agent.as_mut(), live, run, started)?;
        let Some(a) = opened.agent.as_mut() else {
            return Ok(());
        };
        let left = self.times.start.saturating_sub(started.elapsed());
        match a.hello(left, self.times.poll, run) {
            Err(why) if !opened.tests_need_agent => {
                batch
                    .br
                    .notes
                    .push(format!("no test agent for this boot batch: {why}"));
                self.close_run(opened.agent.take(), &mut batch.br);
                return Ok(());
            }
            other => other?,
        }
        let env = &batch.b.env;
        self.env_home(env, a, run);
        self.env_routes(env, a, run)?;
        self.env_career(
            env,
            opened.sandbox.as_ref().and_then(|s| s.mode.as_deref()),
            a,
            run,
        )?;
        self.env_session(env, a, run)
    }

    /// Polls the live loader.log until it has the ready line for `kind`, getting the agent past the title
    /// screen on the way, or until the game exits, the core refuses the test run or the start time runs out.
    fn wait_for_ready(
        &self,
        kind: Kind,
        mut agent: Option<&mut Agent>,
        live: &mut Live,
        run: &mut dyn Running,
        started: Instant,
    ) -> Result<(), String> {
        let mut told_title = false;
        let mut pressed: Option<Instant> = None;
        let mut title: Option<u64> = None;
        let mut by_pad = false;
        loop {
            if let Some(a) = agent.as_deref_mut()
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
            if let Some(a) = agent.as_deref_mut().filter(|a| a.hello && at_title) {
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
        }
    }

    /// Runs each test's steps, its function and its agent checks, then for a benchmark batch waits for the
    /// race to end, which every test of the batch times.
    fn run_tests(
        &self,
        batch: &mut BatchRun,
        mut agent: Option<&mut Agent>,
        live: &mut Live,
        run: &mut dyn Running,
    ) {
        let b = batch.b;
        let race_start = Instant::now();
        for &i in &batch.runnable {
            let t0 = Instant::now();
            let t = &b.tests[i];
            let r = &mut batch.reports[i];
            self.steps(t, r, live, run, agent.as_deref_mut());
            if r.failures.is_empty()
                && let (Some(f), Some(a)) = (&t.function, agent.as_deref_mut())
            {
                self.function(t, f, r, a, run);
            }
            if let Some(a) = agent.as_deref_mut() {
                for c in &t.checks {
                    if let suite::Check::Agent { text } = c
                        && let Err(f) = self.agent_check(text, a, run)
                    {
                        r.failures.push(f);
                    }
                }
            }
            r.ms = Some(ms(t0));
        }
        if b.env.kind == Kind::Benchmark {
            let limit = batch
                .runnable
                .iter()
                .filter_map(|&i| b.tests[i].timeout_s)
                .max()
                .map_or(self.times.race, Duration::from_secs);
            if until_polled(limit, self.times.poll, || run.finished()) {
                for &i in &batch.runnable {
                    batch.reports[i].ms = Some(ms(race_start));
                }
            } else {
                let why = format!("the benchmark race did not end within {} s", limit.as_secs());
                batch.br.error = Some(why.clone());
                batch.fail_all(&why);
            }
        }
    }

    /// Has an agent that said hello quit the game, then stops whatever is still running.
    fn stop_game(&self, br: &mut BatchReport, agent: Option<&mut Agent>, run: &mut dyn Running) {
        if let Some(a) = agent.filter(|a| a.hello) {
            a.client.send("quit", Value::Null, None).ok();
            until_polled(self.times.quit, self.times.poll, || run.finished());
        }
        if let Err(why) = self.finish(run) {
            br.notes.push(why);
        }
    }
}

fn keep_agent_results(br: &mut BatchReport, agent: Option<&mut Agent>) {
    if let Some(a) = agent {
        a.client.poll();
        if let Err(e) = a.keep() {
            br.notes
                .push(format!("could not keep the test agent's results: {e}"));
        }
    }
}

/// Copies the session's logs into the batch folder (or the live loader.log when there is no session) and
/// gathers what the log checks read, noting the session, how it ended and any crash on the batch.
fn evidence(
    br: &mut BatchReport,
    index: usize,
    dir: &Path,
    run: &mut dyn Running,
    live: &mut Live,
) -> Evidence {
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
    Evidence {
        log: final_log,
        perf,
        crash,
        log_path: Some(rel("loader.log")),
        perf_path: Some(rel("perf.log")),
    }
}
