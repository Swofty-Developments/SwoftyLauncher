use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use swofty_core::store::Store;
use swofty_core::testsuite::agent;
use swofty_core::testsuite::fixture::{self, Fixture};
use swofty_core::testsuite::lock;
use swofty_core::testsuite::plan::Batch;
use swofty_core::testsuite::report::exit;
use swofty_core::testsuite::sandbox;
use swofty_core::testsuite::session;
use swofty_core::testsuite::suite::{Env, Kind};

use crate::commands::testing::game_lock::{free_the_game, hh_mm, take_lock};
use crate::commands::testing::harness::{BatchResults, Harness, Times, until_polled};
use crate::commands::testing::launcher::SwoftyRun;
use crate::commands::testing::preflight::terms;
use crate::commands::testing::run::Stop;
use crate::commands::testing::{SessionArgs, ShellArgs, infra, say};
use crate::time::stamp;

/// The request the core's direct start reads (native/src/core/testing/direct_start.cpp): the game starts with
/// `SWOFTY_DIRECT_START=@<run folder>/direct_start.json` and goes from boot straight to the session.
const DIRECT_START_ENV: &str = "SWOFTY_DIRECT_START";
const DIRECT_START_FILE: &str = "direct_start.json";

pub(crate) fn session_command(store: &Store, a: SessionArgs) -> Result<ExitCode> {
    let words: Vec<&str> = a.plan.iter().map(String::as_str).collect();
    let mut request = match session::shell_args(&words) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("swofty: {e}");
            return Ok(ExitCode::from(exit::FAIL));
        }
    };
    request["target"] = "session".into();
    let mut shell = a.shell;
    shell.direct = Some(request);
    shell_command(store, shell)
}

pub(crate) fn shell_command(store: &Store, a: ShellArgs) -> Result<ExitCode> {
    let run_id = format!("shell-{}", stamp());
    let g = match crate::commands::build::game(store, a.game.clone()) {
        Ok(g) => g,
        Err(e) => return Ok(infra(format!("{e:#}"))),
    };
    if let Err(e) = terms(store) {
        return Ok(infra(e));
    }
    if !swofty_core::game_folder::setup::done(&store.root) {
        return Ok(infra(swofty_core::game_folder::setup::NEEDED));
    }
    let dir = a.lock.clone().unwrap_or_else(|| lock::default_path(&store.root));
    let owner = lock::owner_line(&run_id, &["shell".to_owned()], &hh_mm());
    let lock = match take_lock(&dir, &owner, a.wait) {
        Ok(l) => l,
        Err(Stop::Busy(why)) => {
            eprintln!("swofty: {why}");
            return Ok(ExitCode::from(exit::BUSY));
        }
        Err(Stop::Infra(why)) => return Ok(infra(why)),
    };
    let code = shell_session(store, &g.dir, &a, &run_id);
    lock.release().ok();
    code
}

fn shell_session(store: &Store, game: &Path, a: &ShellArgs, run_id: &str) -> Result<ExitCode> {
    if let Err(Stop::Busy(why) | Stop::Infra(why)) = free_the_game(store, a.take_over, a.wait) {
        return Ok(infra(why));
    }
    let fixture = match &a.fixture {
        None => None,
        Some(spec) => {
            let cwd = std::env::current_dir()?;
            let path = match Path::new(spec).join(fixture::FILE).is_file() {
                true => cwd.join(spec),
                false => match fixture::resolve(spec, Some(&cwd), &fixture::store(&store.root)) {
                    Ok(p) => p,
                    Err(e) => return Ok(infra(e)),
                },
            };
            match fixture::load(&path) {
                Ok(f) => Some(f),
                Err(e) => return Ok(infra(e)),
            }
        }
    };
    let exe_sha = match &fixture {
        None => None,
        Some(_) => {
            let mut hashes = swofty_core::game_folder::baseline::Hashes::load(&store.root);
            let sha = hashes.sha256(&game.join(swofty_core::GAME_EXE))?;
            hashes.save();
            let loader = swofty_core::addons::builtin::loader_dir();
            match loader.as_deref().map(|l| sandbox::verified(l, &sha)) {
                Some(Ok(proof)) => say(&format!("save sandbox: verified for this F1_25.exe by {proof}")),
                Some(Err(e)) => return Ok(infra(format!("the shell's save sandbox: {e}"))),
                None => {
                    return Ok(infra(
                        "the shell's save sandbox needs the launcher's loader, which was not found",
                    ));
                }
            }
            Some(sha)
        }
    };
    let live_saves = exe_sha
        .as_ref()
        .and_then(|_| swofty_core::install::Game::locate(&store.root, game.to_path_buf()).ok())
        .and_then(|g| sandbox::live_saves(&g));
    let launch_json = store.root.join("state/launch.json");
    let saved = std::fs::read(&launch_json).ok();
    let profile = format!("test-{run_id}");
    let out = PathBuf::from("swofty-test").join(run_id);
    std::fs::create_dir_all(&out)?;
    let mut launcher = SwoftyRun {
        data: store.root.clone(),
        game: game.to_path_buf(),
    };
    let h = Harness {
        store,
        game,
        out: &out,
        profile: &profile,
        launcher: &mut launcher,
        times: Times::default(),
        run_id,
        bases: BTreeMap::new(),
        exe_sha,
        live_saves,
    };
    let b = Batch {
        key: "shell".into(),
        env: Env {
            kind: match (&a.direct, &fixture) {
                (Some(_), _) => Kind::Session,
                (None, Some(_)) => Kind::Career,
                (None, None) => Kind::Boot,
            },
            params: match &a.direct {
                Some(Value::Object(m)) => m.clone().into_iter().collect(),
                _ => Default::default(),
            },
        },
        addons: a.addons.clone(),
        settings: Default::default(),
        tests: Vec::new(),
    };
    let code = shell_loop(h, &b, a.base.as_deref(), &out, fixture);
    if store.profile_names().is_ok_and(|n| n.contains(&profile)) {
        store.delete_profile(&profile).ok();
    }
    match saved {
        Some(bytes) => std::fs::write(&launch_json, bytes).ok(),
        None => std::fs::remove_file(&launch_json).ok(),
    };
    code
}

fn shell_loop(
    h: Harness,
    b: &Batch,
    base: Option<&str>,
    out: &Path,
    fixture: Option<Fixture>,
) -> Result<ExitCode> {
    match base {
        Some(base) => {
            h.profile_from(base, h.profile, &b.addons)?;
            say(&format!("profile: a copy of {base} with its addons and settings"));
        }
        None => h.profile(b, h.profile)?,
    }
    let (mut br, mut reports) = Harness::blank(b, 1);
    let mut agent = h.open_run(b, 1, out, fixture.is_some())?;
    let sandbox = match fixture {
        None => None,
        Some(f) => match h.open_sandbox(f, b, h.profile, &agent.run) {
            Ok(s) => Some(s),
            Err(e) => {
                h.close_run(Some(agent), &mut br);
                return Ok(infra(format!("the save sandbox: {e}")));
            }
        },
    };
    let mut env = agent.run.env();
    if b.env.kind == Kind::Session {
        let path = agent.run.root.join(DIRECT_START_FILE);
        let request: serde_json::Map<String, Value> = b.env.params.clone().into_iter().collect();
        std::fs::write(&path, serde_json::to_vec_pretty(&Value::Object(request))?)?;
        env.push((DIRECT_START_ENV.to_owned(), format!("@{}", path.display())));
        say(&format!("direct start: {}", path.display()));
    }
    say(&format!(
        "starting F1 25 in test mode (run folder {}{})",
        agent.run.root.display(),
        if sandbox.is_some() { ", save sandbox" } else { "" }
    ));
    let mut run = h.launcher.launch(h.profile, &[], &env, out)?;
    let times = Times::default();
    let mut ok = agent
        .hello(times.start, times.poll, &mut *run)
        .map_err(anyhow::Error::msg);
    if let Err(e) = &ok {
        eprintln!("swofty: {e}");
    } else {
        eprintln!(
            "type a command (title, home, route <name>, tile <id>, read <path>, wait <event> [s], wait screen=<name> [s], pad <button>, capture [name], career load <slot|newest> [at <place>], advance <days>, save now, quit); end with Ctrl-D"
        );
    }
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    while ok.is_ok() && !run.finished() {
        eprint!("> ");
        let Some(Ok(line)) = lines.next() else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }
        let (cmd, args, timeout) = match agent::parse_shell(line) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };
        let limit = timeout.map_or(
            match cmd.as_str() {
                "career" => times.career,
                _ => Duration::from_secs(60),
            },
            Duration::from_millis,
        );
        let before = agent.client.events.len();
        match agent.call(&cmd, args, limit, &times, &mut *run) {
            Ok(r) => println!("{}", r.text()),
            Err(e) => println!("{e}"),
        }
        for (name, v) in &agent.client.events[before..] {
            println!("  [{name}] {v}");
        }
        ok = Ok(());
    }
    if agent.hello {
        agent.client.send("quit", Value::Null, None).ok();
        until_polled(times.quit, times.poll, || run.finished());
    }
    h.finish(&mut *run).ok();
    agent.client.poll();
    agent.keep().ok();
    br.started_game = true;
    let sandboxed = sandbox.is_some();
    if let Some(s) = sandbox {
        h.close_sandbox(
            s,
            Some(&agent.run),
            b,
            h.profile,
            BatchResults {
                br: &mut br,
                reports: &mut reports,
                runnable: &[],
            },
        );
    }
    let id = agent.run.id.clone();
    h.close_run(Some(agent), &mut br);
    for n in br.error.iter().chain(&br.notes) {
        eprintln!("swofty: {n}");
    }
    if sandboxed {
        eprintln!("capture its saves with: swofty test fixture capture <name> --run {id}");
    }
    eprintln!("results: {}", out.display());
    Ok(match br.error {
        Some(_) => ExitCode::from(exit::INFRA),
        None => ExitCode::SUCCESS,
    })
}
