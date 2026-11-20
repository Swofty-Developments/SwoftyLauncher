use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser};

use crate::commands::testing::*;

#[test]
fn waits_parse_as_seconds_minutes_or_hours() {
    assert_eq!(parse_wait("0"), Ok(Duration::ZERO));
    assert_eq!(parse_wait("90s"), Ok(Duration::from_secs(90)));
    assert_eq!(parse_wait("30m"), Ok(Duration::from_secs(1800)));
    assert_eq!(parse_wait("2h"), Ok(Duration::from_secs(7200)));
    assert!(parse_wait("soon").is_err());
    assert!(parse_wait("5d").is_err());
}

#[test]
fn the_cli_takes_the_documented_forms() {
    let parse = |args: &[&str]| crate::Cli::try_parse_from(args).map(|c| c.command);
    let Ok(crate::Command::Test(TestCommand::Run(a))) = parse(&[
        "swofty",
        "test",
        "run",
        "./my-addon",
        "-t",
        "smoke",
        "--take-over",
        "--wait",
        "0",
    ]) else {
        panic!("test run");
    };
    assert_eq!(a.select.paths, [PathBuf::from("./my-addon")]);
    assert_eq!(a.select.tags, ["smoke"]);
    assert!(a.take_over);
    assert_eq!(a.wait, Duration::ZERO);
    let Ok(crate::Command::Test(TestCommand::Run(a))) = parse(&["swofty", "test", "run", "x"]) else {
        panic!("defaults");
    };
    assert_eq!(
        a.wait,
        Duration::from_secs(1800),
        "a run waits for the game by default"
    );
    let Ok(crate::Command::Test(TestCommand::Plan(p))) =
        parse(&["swofty", "test", "plan", "./my-addon", "--id", "a", "--id", "s/b"])
    else {
        panic!("test plan");
    };
    assert_eq!(p.select.ids, ["a", "s/b"]);
    assert!(parse(&["swofty", "test", "plan"]).is_err(), "a path is required");
    assert!(
        parse(&["swofty", "run", "--game-arg=-benchmark"]).is_err(),
        "game arguments need a direct launch"
    );
    assert!(
        parse(&[
            "swofty",
            "run",
            "--direct",
            "--game-arg=-benchmark",
            "--game-arg",
            "x.xml"
        ])
        .is_ok()
    );
    crate::Cli::command().debug_assert();
}

#[test]
fn shell_is_a_test_command() {
    let cli = crate::Cli::try_parse_from(["swofty", "test", "shell", "--addon", "me.x", "--wait", "0"]);
    assert!(cli.is_ok(), "{:?}", cli.err().map(|e| e.to_string()));
    let Ok(crate::Command::Test(TestCommand::Shell(a))) =
        crate::Cli::try_parse_from(["swofty", "test", "shell", "--fixture", "my-team-r3"]).map(|c| c.command)
    else {
        panic!("shell --fixture");
    };
    assert_eq!(a.fixture.as_deref(), Some("my-team-r3"));
}
