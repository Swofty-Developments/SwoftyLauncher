use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use swofty_core::testsuite::agent::{self, Client, Outcome};
use swofty_core::testsuite::career;
use swofty_core::testsuite::checks::Failure;
use swofty_core::testsuite::plan::Batch;
use swofty_core::testsuite::report::{BatchReport, StepReport, TestReport};
use swofty_core::testsuite::sandbox::{RunDir, RunOptions};
use swofty_core::testsuite::session;
use swofty_core::testsuite::suite::{self, Env, Kind, Test};

use crate::commands::testing::harness::{Harness, Times, ms};
use crate::commands::testing::launcher::Running;
use crate::commands::testing::say;

/// A batch's test run folder and its line to the test agent.
pub(crate) struct Agent {
    pub(crate) run: RunDir,
    pub(crate) client: Client,
    pub(crate) hello: bool,
    /// Values the steps read, by their `as` name, and sampled series.
    pub(crate) values: BTreeMap<String, Value>,
    pub(crate) series: BTreeMap<String, Vec<f64>>,
    /// Where results and frames are kept: the batch folder.
    pub(crate) keep_dir: PathBuf,
}

impl Agent {
    /// Read what the agent has written since the last call.
    pub(crate) fn keep_up(&mut self) -> std::result::Result<(), String> {
        self.client.poll();
        self.hello |= self.client.hello.is_some();
        match &self.client.refused {
            Some(why) => Err(format!("the test agent refused the run: {why}")),
            None => Ok(()),
        }
    }

    pub(crate) fn hello(
        &mut self,
        limit: Duration,
        poll: Duration,
        run: &mut dyn Running,
    ) -> std::result::Result<(), String> {
        let hello = self.client.wait_hello(limit, poll, || !run.finished(), || {})?;
        self.hello = true;
        say(&format!(
            "test agent: hello (pid {}, protocol {})",
            hello["pid"], hello["v"]
        ));
        Ok(())
    }

    pub(crate) fn call(
        &mut self,
        cmd: &str,
        args: Value,
        limit: Duration,
        times: &Times,
        run: &mut dyn Running,
    ) -> std::result::Result<agent::Reply, String> {
        let r = self
            .client
            .call(
                cmd,
                args,
                Some(limit),
                times.grace,
                times.poll.min(Duration::from_millis(50)),
                || !run.finished(),
            )
            .map_err(|e| e.to_string())?;
        if cmd == "capture"
            && r.outcome == Outcome::Ok
            && let Some(f) = r.value.get("file").and_then(Value::as_str)
            && let Some(name) = Path::new(f).file_name()
        {
            let frames = self.keep_dir.join("frames");
            std::fs::create_dir_all(&frames).ok();
            std::fs::copy(self.run.agent().join(f), frames.join(name)).ok();
        }
        Ok(r)
    }

    /// Keep what the agent wrote in the batch folder: a run folder without
    /// the sandbox is removed after the batch.
    pub(crate) fn keep(&self) -> std::io::Result<()> {
        let mut text = self.client.lines.join("\n");
        text.push('\n');
        std::fs::write(self.keep_dir.join("agent.jsonl"), text)
    }
}

fn agent_failure(check: String, expected: String, actual: String) -> Failure {
    Failure {
        check,
        expected,
        actual,
        evidence: Some("agent.jsonl".into()),
    }
}

impl Harness<'_> {
    /// The batch's test run folder (`<data>/test-runs/<run>-<batch>/`):
    /// run.json with a fresh token, the test DLLs in `agent/dll/` and, for a
    /// career, the save sandbox's `saves/remote/`.
    pub(crate) fn open_run(
        &self,
        b: &Batch,
        index: usize,
        keep_dir: &Path,
        sandbox: bool,
    ) -> std::io::Result<Agent> {
        let mut dlls: Vec<(PathBuf, String)> = Vec::new();
        for t in &b.tests {
            if let Some(dll) = t.dll() {
                let name = format!("{}.dll", t.suite);
                if !dlls.iter().any(|(_, n)| *n == name) {
                    dlls.push((dll, name));
                }
            } else if let Some(f) = &t.function {
                return Err(std::io::Error::other(format!(
                    "{} runs fn {f}, and there is no native/tests.dll next to {}",
                    t.full_id(),
                    t.file.display()
                )));
            }
        }
        let options = RunOptions {
            sandbox,
            exe_sha256: self.exe_sha.clone(),
            dlls,
        };
        let run = RunDir::create(&self.store.root, &format!("{}-{index}", self.run_id), options)?;
        let client = Client::new(&run.agent());
        Ok(Agent {
            run,
            client,
            hello: false,
            values: BTreeMap::new(),
            series: BTreeMap::new(),
            keep_dir: keep_dir.to_path_buf(),
        })
    }

    /// After the batch: retire the run's token, which stops the agent, and
    /// remove a run folder that holds no saves to capture.
    pub(crate) fn close_run(&self, agent: Option<Agent>, br: &mut BatchReport) {
        let Some(a) = agent else { return };
        let done = match a.run.sandboxed() {
            true => a.run.retire(),
            false => a.run.remove(),
        };
        if let Err(e) = done {
            br.notes.push(format!(
                "could not retire the test run in {}: {e}",
                a.run.root.display()
            ));
        }
    }

    /// A career environment: the agent continues the fixture's career to
    /// the place the test names and logs `[test] career ready at <at>`.
    pub(crate) fn env_career(
        &self,
        env: &Env,
        mode: Option<&str>,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<(), String> {
        if env.kind != Kind::Career {
            return Ok(());
        }
        let (cmd, args) = career::agent_command(env, mode);
        let slot = args["slot"]
            .as_str()
            .map_or_else(|| args["slot"].to_string(), str::to_owned);
        say(&format!("career load {slot} at {}", career::at(env)));
        let reply = a.call(&cmd, args, self.times.career, &self.times, run)?;
        match reply.outcome {
            Outcome::Ok => {
                say(&format!("career ready at {} ({})", career::at(env), reply.value));
                Ok(())
            }
            _ => Err(reply.text()),
        }
    }

    /// A session environment: the agent starts the Time Trial or Grand Prix
    /// session and the CLI waits for it to say the load has started. A
    /// refusal ends the batch with the agent's reason.
    pub(crate) fn env_session(
        &self,
        env: &Env,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<(), String> {
        if env.kind != Kind::Session {
            return Ok(());
        }
        let (cmd, args) = session::agent_command(env);
        say(&format!("{cmd} {}", args));
        let reply = a.call(&cmd, args, self.times.career, &self.times, run)?;
        match reply.outcome {
            Outcome::Ok => {
                say(&format!("session started ({})", reply.value));
                Ok(())
            }
            _ => Err(format!("{cmd}: {}", reply.text())),
        }
    }

    /// A boot environment ends on the main menu: the agent answers the
    /// core's notice and waits for the home screen. An agent that can't
    /// leaves the batch where the front end put it.
    pub(crate) fn env_home(&self, env: &Env, a: &mut Agent, run: &mut dyn Running) {
        if env.kind != Kind::Boot {
            return;
        }
        match a.call("home", Value::Null, self.times.step * 4, &self.times, run) {
            Ok(r) if r.outcome == Outcome::Ok => say(&format!("home screen up ({})", r.value)),
            Ok(r) => say(&format!("home screen: {}", r.text())),
            Err(e) => say(&format!("home screen: {e}")),
        }
    }

    /// A route environment: fire its routes from the home screen in order.
    pub(crate) fn env_routes(
        &self,
        env: &Env,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<(), String> {
        if env.kind != Kind::Route {
            return Ok(());
        }
        let routes: Vec<String> = env
            .params
            .get("routes")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        for r in routes {
            let started = Instant::now();
            let reply = loop {
                let reply = a.call("route", json!({ "route": r }), self.times.step, &self.times, run)?;
                let not_yet =
                    reply.outcome == Outcome::Unavailable && reply.text().contains("no home view model");
                if !not_yet || started.elapsed() >= self.times.step * 6 {
                    break reply;
                }
                std::thread::sleep(self.times.poll.max(Duration::from_millis(500)));
            };
            if reply.outcome != Outcome::Ok {
                return Err(format!("route {r}: {}", reply.text()));
            }
        }
        Ok(())
    }

    pub(crate) fn agent_step(
        &self,
        s: &suite::Step,
        limit: Duration,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<String, Failure> {
        let fail =
            |actual: String| agent_failure(s.action.clone(), format!("{} to succeed", s.action), actual);
        let v: Value = serde_json::from_str(&s.text).unwrap_or_default();
        if s.action == "sample" {
            let o = &v["sample"];
            let path = o["read"].as_str().unwrap_or_default().to_owned();
            let name = o["as"].as_str().unwrap_or_default().to_owned();
            let every = Duration::from_millis(o["every_ms"].as_u64().unwrap_or(500).max(10));
            let span =
                Duration::from_secs(o["for_s"].as_u64().unwrap_or(10)).min(limit.max(Duration::from_secs(1)));
            let start = Instant::now();
            let mut got = Vec::new();
            while start.elapsed() < span {
                let r = a
                    .call("read", json!({ "path": path }), self.times.step, &self.times, run)
                    .map_err(fail)?;
                match (r.outcome, agent::number(&r.value)) {
                    (Outcome::Ok, Some(n)) => got.push(n),
                    (Outcome::Ok, None) => {
                        return Err(fail(format!("read {path} is not a number: {}", r.value)));
                    }
                    _ => return Err(fail(format!("read {path}: {}", r.text()))),
                }
                std::thread::sleep(every);
            }
            let n = got.len();
            a.series.insert(name.clone(), got);
            return Ok(format!("ok: {n} samples as {name}"));
        }
        let cmds = agent::step_commands(s).map_err(|e| fail(e.0))?;
        let mut text = Vec::new();
        for (cmd, args) in cmds {
            let r = a.call(&cmd, args, limit, &self.times, run).map_err(fail)?;
            if r.outcome != Outcome::Ok {
                return Err(fail(r.text()));
            }
            if s.action == "read"
                && let Some(name) = v.get("as").and_then(Value::as_str)
            {
                a.values.insert(name.to_owned(), r.value.clone());
            }
            text.push(r.text());
        }
        Ok(text.join("; "))
    }

    /// A `fn` test: the agent runs the test DLL's coroutine and answers with
    /// its expectations.
    pub(crate) fn function(
        &self,
        t: &Test,
        f: &str,
        r: &mut TestReport,
        a: &mut Agent,
        run: &mut dyn Running,
    ) {
        let limit = t.timeout_s.map_or(self.times.test, Duration::from_secs);
        let t0 = Instant::now();
        let reply = a.call("run_test", json!({ "fn": f }), limit, &self.times, run);
        let status = match &reply {
            Ok(rep) => rep.text(),
            Err(e) => e.clone(),
        };
        r.steps.push(StepReport {
            step: format!("fn {f}"),
            status: status.clone(),
            ms: Some(ms(t0)),
        });
        let rep = match reply {
            Ok(rep) => rep,
            Err(e) => {
                r.failures
                    .push(agent_failure(format!("fn {f}"), "the test to finish".into(), e));
                return;
            }
        };
        let expects = rep
            .value
            .get("expects")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for e in expects.iter().filter(|e| e["ok"] == json!(false)) {
            r.failures.push(agent_failure(
                format!("fn {f}"),
                e["what"].as_str().unwrap_or("an expectation").to_owned(),
                "false".into(),
            ));
        }
        if rep.outcome != Outcome::Ok && r.failures.is_empty() {
            r.failures.push(agent_failure(
                format!("fn {f}"),
                "the test to pass".into(),
                rep.text(),
            ));
        }
    }

    pub(crate) fn operand(
        &self,
        v: &Value,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<Value, String> {
        let Some(name) = v.as_str() else {
            return Ok(v.clone());
        };
        if let Some(stored) = a.values.get(name) {
            return Ok(stored.clone());
        }
        let r = a.call("read", json!({ "path": name }), self.times.step, &self.times, run)?;
        match r.outcome {
            Outcome::Ok => Ok(r.value),
            _ => Err(format!("read {name}: {}", r.text())),
        }
    }

    /// `value` and `series` checks, on what the steps read or a read now.
    pub(crate) fn agent_check(
        &self,
        text: &str,
        a: &mut Agent,
        run: &mut dyn Running,
    ) -> std::result::Result<(), Failure> {
        let c: Value = serde_json::from_str(text).unwrap_or_default();
        let fail = |expected: String, actual: String| agent_failure(text.to_owned(), expected, actual);
        if let Some(name) = c.get("series").and_then(Value::as_str) {
            let Some(s) = a.series.get(name).filter(|s| !s.is_empty()) else {
                return Err(fail(format!("a sampled series {name}"), "no samples".into()));
            };
            let change = s[s.len() - 1] - s[0];
            if let Some(n) = c.get("rises_by").and_then(Value::as_f64)
                && change < n
            {
                return Err(fail(
                    format!("{name} to rise by {n}"),
                    format!("a change of {change}"),
                ));
            }
            if let Some(n) = c.get("falls_by").and_then(Value::as_f64)
                && -change < n
            {
                return Err(fail(
                    format!("{name} to fall by {n}"),
                    format!("a change of {change}"),
                ));
            }
            return Ok(());
        }
        let Some(subject) = c.get("value") else {
            return Err(fail("a value or series check".into(), "neither".into()));
        };
        let got = self
            .operand(subject, a, run)
            .map_err(|e| fail("a value".into(), e))?;
        let num = agent::number(&got);
        let want_num =
            |key: &str, a: &mut Agent, run: &mut dyn Running| -> std::result::Result<Option<f64>, Failure> {
                match c.get(key) {
                    None => Ok(None),
                    Some(v) => {
                        let v = self.operand(v, a, run).map_err(|e| fail(key.into(), e))?;
                        agent::number(&v)
                            .map(Some)
                            .ok_or_else(|| fail(format!("{key} a number"), v.to_string()))
                    }
                }
            };
        let shown = num.map_or_else(|| got.to_string(), |n| n.to_string());
        if let Some(range) = c.get("between").and_then(Value::as_array)
            && range.len() == 2
        {
            let (lo, hi) = (
                range[0].as_f64().unwrap_or(f64::MIN),
                range[1].as_f64().unwrap_or(f64::MAX),
            );
            if !num.is_some_and(|n| n >= lo && n <= hi) {
                return Err(fail(format!("between {lo} and {hi}"), shown));
            }
        }
        if let Some(n) = want_num("greater_than", a, run)?
            && !num.is_some_and(|x| x > n)
        {
            return Err(fail(format!("greater than {n}"), shown));
        }
        if let Some(n) = want_num("less_than", a, run)?
            && !num.is_some_and(|x| x < n)
        {
            return Err(fail(format!("less than {n}"), shown));
        }
        if let Some(want) = c.get("equals") {
            let want = self.operand(want, a, run).map_err(|e| fail("equals".into(), e))?;
            let same = match (agent::number(&want), num) {
                (Some(x), Some(y)) => x == y,
                _ => strip(&want) == strip(&got),
            };
            if !same {
                return Err(fail(format!("equal to {want}"), shown));
            }
        }
        Ok(())
    }
}

fn strip(v: &Value) -> Value {
    v.get("value").cloned().unwrap_or_else(|| v.clone())
}
