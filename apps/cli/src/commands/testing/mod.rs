//! `swofty test`: plan and run addon tests. A run takes the game lock,
//! builds a throwaway profile per batch, starts the game through
//! `swofty run --apply --direct`, waits for the environment, runs the steps
//! through the in-game test agent's mailbox, checks the results and the
//! session's logs and writes the reports. `swofty test shell` starts the
//! game in test mode and sends the agent the commands a person types.

mod fixture_command;
mod game_lock;
mod harness;
mod launcher;
mod preflight;
mod run;
mod shell;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::Value;
use swofty_core::store::Store;
use swofty_core::testsuite::plan::{self, Plan};
use swofty_core::testsuite::report::exit;
use swofty_core::testsuite::suite::{self, Selector, Suite};

use self::fixture_command::fixture_command;
use self::run::run_command;
use self::shell::{session_command, shell_command};
use crate::commands::{Execute, Invocation};

impl Execute for TestCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(
            &invocation.store,
            invocation.named_profile.as_deref(),
            &invocation.profile,
            self,
        )
    }
}

#[derive(Subcommand)]
pub enum TestCommand {
    #[command(
        about = "Show the tests a run would start, grouped by environment, and how many game starts it takes"
    )]
    Plan(PlanArgs),
    #[command(
        about = "Start F1 25 in each test's environment, run its steps, check what happened and write the report"
    )]
    Run(RunArgs),
    #[command(
        about = "Start F1 25 in test mode and send the test agent the commands you type (route, read, wait, pad, capture, career load, quit...)"
    )]
    Shell(ShellArgs),
    #[command(
        about = "Start F1 25 straight into a Grand Prix or Time Trial session, with no title or menu, then take commands as the shell does",
        after_help = "Example (a 3-lap race at Silverstone in Vettel's Ferrari, on the 2020 Classic Seasons box):\n  swofty --profile default test session grand_prix cs_f1_2020 silverstone session=race laps=3 team=ferrari driver=vettel --fixture <fixture>\n\nA Grand Prix takes the box (its entrant list), the track, team= and driver= (the player's seat; a part of the label is enough: williams, vettel), session= (practice1..3, qualifying, race, or a game session name such as practice_s or qualifying_o) and laps= (races). weather= and ai_difficulty= are not applied yet. The game runs on a copy of --profile (or the active profile) with its addons and settings, behind a cover screen until the garage or the race broadcast is up. Use --fixture: a new Grand Prix overwrites the profile save's Grand Prix."
    )]
    Session(SessionArgs),
    #[command(
        subcommand,
        about = "Capture, list and check fixtures: saved games in a known state"
    )]
    Fixture(FixtureCommand),
}

#[derive(Args)]
pub struct ShellArgs {
    #[arg(
        long = "addon",
        value_name = "ID",
        help = "An installed addon to switch on in the shell's profile (repeatable)"
    )]
    addons: Vec<String>,
    #[arg(
        long,
        help = "Close a running F1 25 first, by its exact process id, and wait for its game files to be put back"
    )]
    take_over: bool,
    #[arg(
        long,
        value_name = "TIME",
        default_value = "30m",
        value_parser = parse_wait,
        help = "How long to wait for the game to be free: 0, 90s, 30m, 2h"
    )]
    wait: Duration,
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    game: Option<PathBuf>,
    #[arg(
        long,
        value_name = "DIR",
        help = "The machine's game lock folder (as for swofty test run)"
    )]
    lock: Option<PathBuf>,
    #[arg(
        long,
        value_name = "FIXTURE",
        help = "Start in the save sandbox with this fixture's saves (a fixture folder or a name in the fixture store), so career load, advance and save now work"
    )]
    fixture: Option<String>,
    #[arg(skip)]
    direct: Option<Value>,
    #[arg(skip)]
    base: Option<String>,
}

#[derive(Args)]
pub struct SessionArgs {
    #[arg(
        required = true,
        num_args = 1..,
        value_name = "PLAN",
        help = "<time_trial|grand_prix> [box] [track] [team=..] [driver=..] [session=..] [weather=..] [laps=..] [ai_difficulty=..]"
    )]
    plan: Vec<String>,
    #[command(flatten)]
    shell: ShellArgs,
}

#[derive(Subcommand)]
pub enum FixtureCommand {
    #[command(
        about = "Copy a test run's sandbox saves and addon storage into a new fixture in the fixture store"
    )]
    Capture(CaptureArgs),
    #[command(about = "List the fixtures in the fixture store and check their files")]
    List(StoreArgs),
    #[command(about = "Check a fixture's files against its fixture.json")]
    Check(CheckArgs),
}

#[derive(Args)]
pub struct StoreArgs {
    #[arg(
        long,
        value_name = "DIR",
        help = "The fixture store (default: $SWOFTY_TEST_FIXTURES, else test-fixtures in the data folder)"
    )]
    store: Option<PathBuf>,
}

#[derive(Args)]
pub struct CaptureArgs {
    #[arg(help = "The new fixture's name (letters, digits, - _ .)")]
    name: String,
    #[arg(
        long,
        value_name = "RUN",
        conflicts_with = "from",
        help = "The test run whose sandbox to capture, as named under test-runs/ (default: the newest)"
    )]
    run: Option<String>,
    #[arg(
        long,
        value_name = "DIR",
        help = "Capture a save folder instead of a test run: a Steam remote/ folder or the userdata/<app> folder holding \
                one, or an EA app copy's savegame/ea folder or the savegame folder holding one, such as a copy of a \
                backup. Never the live folder"
    )]
    from: Option<PathBuf>,
    #[arg(long, help = "What the fixture holds, in a sentence")]
    description: Option<String>,
    #[arg(
        long = "addon",
        value_name = "ID=VERSION",
        help = "An addon the fixture's careers need (repeatable; a run's addons are recorded on their own)"
    )]
    addons: Vec<String>,
    #[arg(
        long = "career",
        value_name = "MODE[:SLOT]",
        help = "A career the fixture holds, by its game mode and save slot (repeatable)"
    )]
    careers: Vec<String>,
    #[arg(long, help = "Free text for fixture.json's holds.notes")]
    notes: Option<String>,
    #[command(flatten)]
    store: StoreArgs,
}

#[derive(Args)]
pub struct CheckArgs {
    #[arg(help = "A fixture name in the store or a fixture folder")]
    fixture: String,
    #[command(flatten)]
    store: StoreArgs,
}

#[derive(Args)]
pub struct Select {
    #[arg(
        required = true,
        help = "An addon folder, its tests folder or a .test.json file"
    )]
    paths: Vec<PathBuf>,
    #[arg(
        short = 't',
        long = "tag",
        value_name = "TAG",
        help = "Only tests with this tag (repeatable)"
    )]
    tags: Vec<String>,
    #[arg(
        long = "id",
        value_name = "ID",
        help = "Only this test, by id or suite/id (repeatable)"
    )]
    ids: Vec<String>,
}

#[derive(Args)]
pub struct PlanArgs {
    #[command(flatten)]
    select: Select,
    #[arg(long, help = "Print the plan as JSON")]
    json: bool,
}

#[derive(Args)]
pub struct RunArgs {
    #[command(flatten)]
    select: Select,
    #[arg(
        long,
        help = "Folder for the report (default: swofty-test/<run> in this folder)"
    )]
    out: Option<PathBuf>,
    #[arg(
        long,
        help = "Close a running F1 25 first, by its exact process id, and wait for its game files to be put back"
    )]
    take_over: bool,
    #[arg(
        long,
        value_name = "TIME",
        default_value = "30m",
        value_parser = parse_wait,
        help = "How long to wait for the game to be free: 0, 90s, 30m, 2h"
    )]
    wait: Duration,
    #[arg(long, help = crate::commands::cli::GAME_HELP)]
    game: Option<PathBuf>,
    #[arg(
        long,
        value_name = "DIR",
        help = "The machine's game lock folder (default: $SWOFTY_GAME_LOCK, else /mnt/work/F1/f1-game.lock.d where /mnt/work/F1 exists, else swofty-game.lock.d next to the data folder)"
    )]
    lock: Option<PathBuf>,
    #[arg(long, help = "Print report.json instead of the summary")]
    json: bool,
}

pub(crate) fn parse_wait(s: &str) -> std::result::Result<Duration, String> {
    let s = s.trim();
    let (num, unit) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(at) => s.split_at(at),
        None => (s, "s"),
    };
    let n: u64 = num
        .parse()
        .map_err(|_| format!("{s}: expected a time such as 0, 90s, 30m or 2h"))?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        _ => return Err(format!("{s}: expected a time such as 0, 90s, 30m or 2h")),
    };
    Ok(Duration::from_secs(secs))
}

/// `named` is the `--profile` given on the command line and `active` the profile it resolves to (the active
/// profile without one). A shell starts on a copy of `named`; a session on a copy of `active`.
pub fn command(store: &Store, named: Option<&str>, active: &str, cmd: TestCommand) -> Result<ExitCode> {
    match cmd {
        TestCommand::Plan(a) => plan_command(&a),
        TestCommand::Run(a) => run_command(store, a),
        TestCommand::Shell(mut a) => {
            a.base = named.map(str::to_owned);
            shell_command(store, a)
        }
        TestCommand::Session(mut a) => {
            a.shell.base = Some(active.to_owned());
            session_command(store, a)
        }
        TestCommand::Fixture(c) => fixture_command(store, c),
    }
}

/// The folder of each suite's test file: a test's `fixture` path is relative to it.
pub(crate) fn bases(suites: &[Suite]) -> BTreeMap<String, PathBuf> {
    suites
        .iter()
        .map(|s| {
            (
                s.id.clone(),
                s.file.parent().map(Path::to_path_buf).unwrap_or_default(),
            )
        })
        .collect()
}

pub(crate) fn load(select: &Select) -> std::result::Result<(Vec<Suite>, Plan), String> {
    let mut suites: Vec<Suite> = Vec::new();
    for p in &select.paths {
        for s in suite::load(p).map_err(|e| e.0)? {
            if let Some(o) = suites.iter().find(|o| o.id == s.id) {
                return Err(format!(
                    "{}: suite {} is also in {}",
                    s.file.display(),
                    s.id,
                    o.file.display()
                ));
            }
            suites.push(s);
        }
    }
    let selector = Selector {
        tags: select.tags.clone(),
        ids: select.ids.clone(),
    };
    let plan = plan::plan(&suites, &selector);
    if plan.tests() == 0 {
        let mut want: Vec<String> = select.tags.iter().map(|t| format!("-t {t}")).collect();
        want.extend(select.ids.iter().map(|i| format!("--id {i}")));
        return Err(match want.is_empty() {
            true => "the test files hold no tests".to_owned(),
            false => format!("no test matches {}", want.join(" ")),
        });
    }
    Ok((suites, plan))
}

pub(crate) fn infra(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("swofty: {e}");
    ExitCode::from(exit::INFRA)
}

fn plan_command(a: &PlanArgs) -> Result<ExitCode> {
    let (_, plan) = match load(&a.select) {
        Ok(p) => p,
        Err(e) => return Ok(infra(e)),
    };
    if a.json {
        println!("{}", serde_json::to_string_pretty(&plan.json())?);
    } else {
        print!("{}", plan.text());
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn say(text: &str) {
    eprintln!("[test] {text}");
}

#[cfg(test)]
mod fakes;
#[cfg(test)]
mod tests;
