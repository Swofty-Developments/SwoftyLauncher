mod addon_commands;
mod commands;
mod console;
mod session_lock;
mod signals;
mod stores;
mod time;

use std::ffi::OsString;
use std::process::ExitCode;

use anyhow::Result;
use clap::FromArgMatches;
use swofty_core::store::Store;

use crate::addon_commands::AddonCommands;
use crate::commands::cli::{Cli, Command, data_dir_arg};
use crate::commands::{Execute, Invocation, doctor, registry};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let store = data_dir_arg(&args)
        .map(Store::new)
        .unwrap_or_else(Store::open_default)
        .with_builtins(swofty_builtins::all());
    match run_cli(&store, args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("swofty: {}", causes(&e));
            let stopped = e
                .chain()
                .any(|c| matches!(c.downcast_ref(), Some(swofty_core::Error::Dependencies(_))));
            match stopped {
                true => ExitCode::from(swofty_core::stdlib::deps::STOP_EXIT),
                false => ExitCode::FAILURE,
            }
        }
    }
}

fn causes(e: &anyhow::Error) -> String {
    let mut out = e.to_string();
    for c in e.chain().skip(1).map(|c| c.to_string()) {
        if !out.contains(&c) {
            out = format!("{out}: {c}");
        }
    }
    out
}

fn migrate(store: &Store) {
    match store.migrate() {
        Ok(notes) => notes.iter().for_each(|n| eprintln!("swofty: {n}")),
        Err(e) => eprintln!("swofty: data migration failed: {e}"),
    }
}

fn run_cli(store: &Store, mut args: Vec<OsString>) -> Result<ExitCode> {
    let addons = AddonCommands::load(store);
    if addons.provides("mods") {
        addon_commands::forward_maps(&mut args);
    }
    if addons.provides("mods")
        && let Some((forwarded, note)) = addon_commands::forward_mods(store, &args)
    {
        eprintln!("swofty: {note}");
        args = forwarded;
    }
    let matches = addons.command().get_matches_from(args);
    migrate(store);
    if let Some((name, sub)) = matches.subcommand()
        && let Some((id, spec)) = addons.find(name)
    {
        let profile = matches
            .get_one::<String>("profile")
            .cloned()
            .unwrap_or_else(|| store.active_profile());
        return addons.run(store, &profile, id, spec, sub);
    }
    dispatch(store, Cli::from_arg_matches(&matches)?)
}

fn dispatch(store: &Store, cli: Cli) -> Result<ExitCode> {
    let invocation = Invocation {
        store: store.clone(),
        profile: cli.profile.clone().unwrap_or_else(|| store.active_profile()),
        named_profile: cli.profile,
    };
    cli.command.execute(&invocation)
}

impl Execute for Command {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        match self {
            Command::Run(args) => args.execute(invocation),
            Command::Build(args) => args.execute(invocation),
            Command::Conflicts(args) => args.execute(invocation),
            Command::Apply(args) => args.execute(invocation),
            Command::Verify(args) => args.execute(invocation),
            Command::Restore(args) => args.execute(invocation),
            Command::Addons(cmd) => cmd.execute(invocation),
            Command::Offline(cmd) => cmd.execute(invocation),
            Command::Setup(cmd) => cmd.execute(invocation),
            Command::Settings(cmd) => cmd.execute(invocation),
            Command::Profile(cmd) => cmd.execute(invocation),
            Command::Db(args) => args.execute(invocation),
            Command::Logs(args) => args.execute(invocation),
            Command::Doctor => doctor::doctor(&invocation.store),
            Command::Game(cmd) => cmd.execute(invocation),
            Command::Games(args) => args.execute(invocation),
            Command::Saves(cmd) => cmd.execute(invocation),
            Command::Laps(cmd) => cmd.execute(invocation),
            Command::Steam(args) => args.execute(invocation),
            Command::Postfx(cmd) => cmd.execute(invocation),
            Command::Shaders(cmd) => cmd.execute(invocation),
            Command::Test(cmd) => cmd.execute(invocation),
            Command::Dev(cmd) => cmd.execute(invocation),
            Command::Update(args) => args.execute(invocation),
            Command::Registry(cmd) => cmd.execute(invocation),
            Command::Hud(cmd) => cmd.execute(invocation),
            Command::Careers => {
                registry::careers(&invocation.store, &invocation.profile).map(|()| ExitCode::SUCCESS)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_says_its_os_message_once() {
        let e: anyhow::Error = swofty_core::Error::Io {
            path: "F1 25/a.erp".into(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        }
        .into();
        assert_eq!(causes(&e), "F1 25/a.erp: entity not found");
        let e = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::NotFound)).context("reading x");
        assert_eq!(causes(&e), "reading x: entity not found");
    }
}
