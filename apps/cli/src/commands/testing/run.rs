use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::Result;
use swofty_core::store::Store;
use swofty_core::testsuite::lock;
use swofty_core::testsuite::plan::Plan;
use swofty_core::testsuite::report::{self, Report, exit};

use crate::commands::testing::game_lock::{free_the_game, hh_mm, take_lock};
use crate::commands::testing::harness::{Harness, Times};
use crate::commands::testing::launcher::SwoftyRun;
use crate::commands::testing::preflight::{preflight, sandbox_preflight};
use crate::commands::testing::{RunArgs, bases, infra, load, say};
use crate::time::stamp;

pub(crate) fn run_command(store: &Store, a: RunArgs) -> Result<ExitCode> {
    let started = Instant::now();
    let (suites, plan) = match load(&a.select) {
        Ok(p) => p,
        Err(e) => return Ok(infra(e)),
    };
    let run_id = stamp();
    let out = a
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("swofty-test").join(&run_id));
    if let Err(e) = std::fs::create_dir_all(&out) {
        return Ok(infra(format!("{}: {e}", out.display())));
    }
    let mut report = Report::new(&run_id, &out.display().to_string());
    report.game_starts = plan.game_starts();
    let busy = if plan.game_starts() == 0 {
        Harness::skip_all(&plan, &mut report);
        false
    } else {
        let ids: Vec<String> = suites.iter().map(|s| s.id.clone()).collect();
        let suite_bases = bases(&suites);
        let run = GameRun {
            store,
            args: &a,
            plan: &plan,
            run_id: &run_id,
            suites: &ids,
            bases: &suite_bases,
            out: &out,
        };
        match run_with_game(run, &mut report) {
            Ok(()) => false,
            Err(Stop::Busy(why)) => {
                report.errors.push(why);
                Harness::not_run(&plan, &mut report, "the game was not free");
                true
            }
            Err(Stop::Infra(why)) => {
                report.errors.push(why);
                Harness::not_run(&plan, &mut report, "the run could not start the game");
                false
            }
        }
    };
    report.ms = started.elapsed().as_millis() as u64;
    report.exit_code = if busy { exit::BUSY } else { report.compute_exit() };
    if let Err(e) = write_reports(&out, &report) {
        eprintln!("swofty: could not write the report in {}: {e}", out.display());
    }
    if a.json {
        print!("{}", report.json());
    } else {
        print!("{}", report.summary());
    }
    Ok(ExitCode::from(report.exit_code))
}

fn write_reports(out: &Path, r: &Report) -> std::io::Result<()> {
    std::fs::write(out.join(report::JSON_FILE), r.json())?;
    std::fs::write(out.join(report::JUNIT_FILE), r.junit())?;
    std::fs::write(out.join(report::SUMMARY_FILE), r.summary())
}

#[derive(Debug)]
pub(crate) enum Stop {
    Busy(String),
    Infra(String),
}

pub(crate) const MIN_FREE: u64 = 1 << 30;

struct GameRun<'a> {
    store: &'a Store,
    args: &'a RunArgs,
    plan: &'a Plan,
    run_id: &'a str,
    suites: &'a [String],
    bases: &'a BTreeMap<String, PathBuf>,
    out: &'a Path,
}

fn run_with_game(run: GameRun<'_>, report: &mut Report) -> std::result::Result<(), Stop> {
    let GameRun {
        store,
        args: a,
        plan,
        run_id,
        suites,
        bases,
        out,
    } = run;
    let g = crate::commands::build::game(store, a.game.clone()).map_err(|e| Stop::Infra(format!("{e:#}")))?;
    crate::commands::build::supported(&g).ok();
    report.game_build = swofty_core::game_folder::exe_version::read(&g.dir.join(swofty_core::GAME_EXE));
    preflight(store, &g, plan, out).map_err(Stop::Infra)?;
    let exe_sha = sandbox_preflight(store, &g, plan, bases).map_err(Stop::Infra)?;
    let live_saves = exe_sha
        .as_ref()
        .and_then(|_| swofty_core::testsuite::sandbox::live_saves(&g));
    let dir = a.lock.clone().unwrap_or_else(|| lock::default_path(&store.root));
    let owner = lock::owner_line(run_id, suites, &hh_mm());
    let lock = take_lock(&dir, &owner, a.wait)?;
    say(&format!("holding the game lock {}", dir.display()));
    free_the_game(store, a.take_over, a.wait)?;
    let launch_json = store.root.join("state/launch.json");
    let saved = std::fs::read(&launch_json).ok();
    let profile = format!("test-{run_id}");
    let mut launcher = SwoftyRun {
        data: store.root.clone(),
        game: g.dir.clone(),
    };
    let mut h = Harness {
        store,
        game: &g.dir,
        out,
        profile: &profile,
        launcher: &mut launcher,
        times: Times::default(),
        run_id,
        bases: bases.clone(),
        exe_sha,
        live_saves,
    };
    h.run(plan, report);
    if store.profile_names().is_ok_and(|n| n.contains(&profile))
        && let Err(e) = store.delete_profile(&profile)
    {
        report
            .errors
            .push(format!("could not delete the test profile {profile}: {e}"));
    }
    let put_back = match saved {
        Some(bytes) => std::fs::write(&launch_json, bytes),
        None => std::fs::remove_file(&launch_json).or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        }),
    };
    if let Err(e) = put_back {
        report
            .errors
            .push(format!("could not put {} back: {e}", launch_json.display()));
    }
    if let Err(e) = lock.release() {
        report
            .errors
            .push(format!("could not release the game lock {}: {e}", dir.display()));
    }
    Ok(())
}
