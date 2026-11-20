use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use swofty_core::platform::process::stop_process;
use swofty_core::session::{Ending, LOADER_LOG, PERF_LOG, SessionEnd};
#[cfg(unix)]
use swofty_core::testsuite::agent;
use swofty_core::testsuite::fixture;
#[cfg(unix)]
use swofty_core::testsuite::report::{Report, TestReport};
#[cfg(unix)]
use swofty_core::testsuite::sandbox;

use crate::commands::testing::harness::Times;
use crate::commands::testing::launcher::{Launcher, Running};

pub(crate) fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-cli-test-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A stand-in for `swofty run`: a shell script plays the game, writing
/// loader.log lines into the game folder, and its end does what the
/// wrapper does when the game exits (keep the logs, record the end).
pub(crate) struct Fake {
    pub(crate) scripts: Vec<&'static str>,
    pub(crate) root: PathBuf,
    pub(crate) game: PathBuf,
    pub(crate) launches: Vec<(String, Vec<String>)>,
    pub(crate) envs: Vec<Vec<(String, String)>>,
}

impl Launcher for Fake {
    fn launch(
        &mut self,
        profile: &str,
        game_args: &[String],
        env: &[(String, String)],
        dir: &Path,
    ) -> anyhow::Result<Box<dyn Running>> {
        self.launches.push((profile.to_owned(), game_args.to_vec()));
        self.envs.push(env.to_vec());
        let script = self.scripts.remove(0);
        let session = self.root.join(format!("sessions/{}", self.launches.len()));
        std::fs::create_dir_all(&session)?;
        let log = std::fs::File::create(dir.join("launcher.log"))?;
        let child = Command::new("sh")
            .arg("-c")
            .arg(script)
            .arg("sh")
            .args(game_args)
            .env("GAME", &self.game)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        Ok(Box::new(FakeRun {
            child,
            game: self.game.clone(),
            session,
            done: false,
            stops: 0,
        }))
    }
}

pub(crate) struct FakeRun {
    child: Child,
    game: PathBuf,
    session: PathBuf,
    done: bool,
    stops: u32,
}

impl Running for FakeRun {
    fn finished(&mut self) -> bool {
        if self.done {
            return true;
        }
        let Ok(Some(status)) = self.child.try_wait() else {
            return false;
        };
        std::fs::copy(self.game.join(LOADER_LOG), self.session.join("loader.log")).ok();
        std::fs::copy(self.game.join(PERF_LOG), self.session.join("perf.log")).ok();
        std::fs::remove_dir_all(self.game.join("swofty")).ok();
        let code = status.code().unwrap_or(-1);
        let (end, reason) = match code {
            3 => (Ending::Crash, "exception 0xc0000005 in F1_25.exe"),
            _ if self.stops > 0 => (Ending::Stopped, "the test run closed F1 25"),
            _ => (Ending::Quit, "the game quit"),
        };
        SessionEnd {
            end,
            exit_code: Some(code),
            crash_code: None,
            reason: reason.into(),
        }
        .write(&self.session)
        .unwrap();
        std::fs::write(self.session.join("exit_code"), format!("{code}\n")).unwrap();
        self.done = true;
        true
    }

    fn stop(&mut self) {
        self.stops += 1;
        stop_process(self.child.id()).ok();
    }

    fn kill(&mut self) {
        self.child.kill().ok();
    }

    fn session(&mut self) -> Option<PathBuf> {
        self.done.then(|| self.session.clone())
    }
}

#[cfg(unix)]
pub(crate) const BOOT: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
echo "[10:00:00.100] [my] hello" >> "$L"
trap 'echo "[10:00:09.000] [core] closing" >> "$L"; exit 0' TERM
sleep 0.3
echo '[10:00:01.000] [career] flow "#957880c6" Entering -> Active' >> "$L"
echo "[10:00:01.100] [events] game.ready" >> "$L"
echo "frame times: p50 8.00 ms, p95 12.00 ms, p99 30.00 ms, max 90 ms" > "$GAME/swofty/perf.log"
while :; do sleep 0.05; done
"##;

#[cfg(unix)]
pub(crate) const CRASH: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
echo '[10:00:01.000] [career] flow "#957880c6" Entering -> Active' >> "$L"
sleep 0.3
echo "[10:00:02.000] [crash] unhandled exception 0xc0000005" >> "$L"
exit 3
"##;

#[cfg(unix)]
pub(crate) const BENCH: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "$*" > "$GAME/args.txt"
cp "$GAME/benchmark/swofty_test.xml" "$GAME/seen.xml"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
sleep 0.2
echo "[10:00:25.000] [events] race.start (bahrain, 1 lap)" >> "$L"
echo "frame times: p50 8.00 ms, p95 12.00 ms, p99 22.50 ms, max 90 ms" > "$GAME/swofty/perf.log"
sleep 0.2
exit 0
"##;

#[cfg(unix)]
pub(crate) const STUCK: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
trap 'exit 0' TERM
while :; do sleep 0.05; done
"##;

pub(crate) fn quick() -> Times {
    Times {
        start: Duration::from_secs(10),
        step: Duration::from_secs(5),
        test: Duration::from_secs(20),
        race: Duration::from_secs(10),
        finish: Duration::from_secs(10),
        restop: Duration::from_secs(2),
        poll: Duration::from_millis(20),
        grace: Duration::from_secs(5),
        quit: Duration::from_secs(5),
        title_press: Duration::from_millis(100),
        career: Duration::from_secs(10),
    }
}

#[cfg(unix)]
pub(crate) fn status_of<'r>(r: &'r Report, id: &str) -> &'r TestReport {
    r.tests.iter().find(|t| t.id == id).unwrap()
}

#[cfg(unix)]
pub(crate) const AGENT_GAME: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
echo "$SWOFTY_TEST_RUN|$SWOFTY_TEST_TOKEN|$SWOFTY_EARLY_STAGE" > "$GAME/env.txt"
echo '[10:00:01.000] [career] flow "#a4d3a4b2" Entering -> Active' >> "$L"
trap 'exit 0' TERM
while [ ! -e "$GAME/pressed" ]; do sleep 0.02; done
echo '[10:00:02.000] [career] flow "#957880c6" Entering -> Active' >> "$L"
while [ ! -e "$GAME/quit" ]; do sleep 0.02; done
echo "[10:00:09.000] [test] quit" >> "$L"
exit 0
"##;

#[cfg(unix)]
pub(crate) const CAREER_GAME: &str = r##"
L="$GAME/swofty/loader.log"; mkdir -p "$GAME/swofty"
echo "[10:00:00.000] swofty loader (version.dll proxy) attached, pid $$" > "$L"
echo "$SWOFTY_TEST_RUN|$SWOFTY_TEST_TOKEN|$SWOFTY_EARLY_STAGE" > "$GAME/env.txt"
trap 'exit 0' TERM
while [ ! -e "$GAME/quit" ]; do sleep 0.02; done
exit 0
"##;

/// How the fake agent answers the commands that differ between tests.
#[cfg(unix)]
#[derive(Clone, Copy, Default)]
pub(crate) struct AgentPlay {
    /// `title` answers unavailable, so the CLI falls back to the pad.
    pub(crate) title_unbound: bool,
    /// `career` answers ok and the agent writes sandbox.json armed.
    pub(crate) career_ok: bool,
}

#[cfg(unix)]
pub(crate) fn same_command(line: &str, cmd: &str, args: &str) -> bool {
    let Some((got_cmd, got_args)) = line.split_once(' ') else {
        return false;
    };
    let parse = |t: &str| serde_json::from_str::<serde_json::Value>(t).ok();
    got_cmd == cmd && parse(got_args).is_some() && parse(got_args) == parse(args)
}

/// The in-game agent's part, played by a thread: it reads the run folder
/// and token from the game's environment, checks run.json holds the same
/// token, then answers commands in the run's agent/ folder.
#[cfg(unix)]
pub(crate) fn fake_agent(game: PathBuf, play: AgentPlay) -> std::thread::JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        use std::io::Write;
        let start = std::time::Instant::now();
        let mut seen = Vec::new();
        let (run, token) = loop {
            if let Ok(t) = std::fs::read_to_string(game.join("env.txt"))
                && let [r, k, _] = t.trim_end_matches('\n').split('|').collect::<Vec<_>>()[..]
            {
                break (PathBuf::from(r), k.to_owned());
            }
            assert!(start.elapsed() < Duration::from_secs(10), "no game env");
            std::thread::sleep(Duration::from_millis(10));
        };
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(run.join(sandbox::RUN_FILE)).unwrap()).unwrap();
        assert_eq!(
            doc["token"],
            token.as_str(),
            "run.json holds the environment's token"
        );
        let dir = run.join(sandbox::AGENT);
        let log = game.join("swofty/loader.log");
        let out = |line: String| {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(agent::RESULTS))
                .unwrap();
            writeln!(f, "{line}").unwrap();
        };
        out(format!(
            r#"{{"event":"hello","v":1,"pid":4242,"run":{},"dlls":{}}}"#,
            doc["run"], doc["agent"]["dlls"]
        ));
        let mut done = 0;
        let mut money = 1000;
        loop {
            let text = std::fs::read_to_string(dir.join(agent::COMMANDS)).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            for l in &lines[done..] {
                let c: serde_json::Value = serde_json::from_str(l).unwrap();
                let seq = c["seq"].as_u64().unwrap();
                let cmd = c["cmd"].as_str().unwrap().to_owned();
                seen.push(format!("{cmd} {}", c["args"]));
                if run.join("saves").is_dir() {
                    std::fs::write(run.join(sandbox::STATUS_FILE), r#"{"state":"armed"}"#).unwrap();
                }
                let reply = match cmd.as_str() {
                    "title" if play.title_unbound => {
                        r#""status":"unavailable","error":"the title's sync point is not bound on this build""#
                            .to_owned()
                    }
                    "title" | "pad" => {
                        std::fs::write(game.join("pressed"), "").unwrap();
                        r#""status":"ok""#.to_owned()
                    }
                    "career" if play.career_ok => {
                        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
                        writeln!(
                            f,
                            "[10:00:05.000] [test] career ready at {} (slot {})",
                            c["args"]["at"].as_str().unwrap(),
                            c["args"]["slot"]
                        )
                        .unwrap();
                        r#""status":"ok","value":{"at":"team_hq","slot":"newest"}"#.to_owned()
                    }
                    "career" => {
                        r#""status":"timeout","error":"career load: the resume card's press did not happen within 30 s""#
                            .to_owned()
                    }
                    "route" if c["args"]["route"] == "career" => r#""status":"ok""#.to_owned(),
                    "route" => r#""status":"unavailable","error":"no home view model""#.to_owned(),
                    "read" if c["args"]["path"] == "career.money" => {
                        money += 500;
                        format!(r#""status":"ok","value":{{"value":{money},"type":"i64"}}"#)
                    }
                    "read" => r#""status":"ok","value":{"value":800.5,"type":"f32"}"#.to_owned(),
                    "wait" => {
                        out(r#"{"event":"fired","id":"career.hub"}"#.to_owned());
                        r#""status":"ok","value":{"id":"career.hub","fires":1}"#.to_owned()
                    }
                    "run_test" => r#""status":"error","error":"1 expectation failed","value":{"expects":[{"ok":true,"what":"on the hub"},{"ok":false,"what":"the R&D tab is hidden"}]}"#.to_owned(),
                    "quit" => r#""status":"ok""#.to_owned(),
                    "home" => r#""status":"ok","value":{"home":true,"notice_presses":1}"#.to_owned(),
                    _ => r#""status":"unavailable","error":"the car struct is not bound in this build""#.to_owned(),
                };
                out(format!(r#"{{"seq":{seq},{reply},"ms":1}}"#));
                if cmd == "quit" {
                    std::fs::write(game.join("quit"), "").unwrap();
                    return seen;
                }
            }
            done = lines.len();
            if start.elapsed() > Duration::from_secs(20) {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })
}

pub(crate) fn fixture_at(tests_dir: &Path, name: &str, sha: &str) {
    let remote = tests_dir.join("captured-remote");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join(fixture::PROFILE_FILES[0]), [7u8; 900]).unwrap();
    std::fs::write(remote.join(fixture::PROFILE_FILES[1]), [8u8; 416]).unwrap();
    let meta = fixture::Meta {
        exe: fixture::Exe {
            sha256: sha.into(),
            version: None,
        },
        created: "now".into(),
        ..Default::default()
    };
    fixture::capture(name, &tests_dir.join("fixtures"), &remote, &[], meta).unwrap();
}
