use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, CommandFactory};
use swofty_core::launcher::CommandSpec;
use swofty_core::manifest::MANIFEST_FILE;
use swofty_core::registry::{Host, Registry};
use swofty_core::store::Store;

use crate::Cli;

pub(crate) struct AddonCommands {
    pub(crate) registry: Registry,
    pub(crate) specs: Vec<(String, CommandSpec)>,
    pub(crate) namespaced: Vec<Namespaced>,
    pub(crate) developer: bool,
}

/// An addon command as `swofty <addon id>:<name>`. Every addon command has one, so a command whose
/// name a built-in command (or another addon) took stays reachable; only those are listed in help.
pub(crate) struct Namespaced {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) spec: CommandSpec,
    pub(crate) listed: bool,
}

pub(crate) fn namespaced(
    all: &[(String, Vec<CommandSpec>)],
    accepted: &[(String, CommandSpec)],
) -> Vec<Namespaced> {
    all.iter()
        .flat_map(|(id, specs)| specs.iter().map(move |spec| (id, spec)))
        .map(|(id, spec)| Namespaced {
            id: id.clone(),
            name: format!("{id}:{}", spec.name),
            spec: spec.clone(),
            listed: !accepted.iter().any(|(a, s)| a == id && s.name == spec.name),
        })
        .collect()
}

impl AddonCommands {
    pub(crate) fn load(store: &Store) -> Self {
        let registry = store.registry().unwrap_or_else(|e| {
            eprintln!("swofty: {e}");
            Registry::default()
        });
        let reserved: Vec<String> = Cli::command()
            .get_subcommands()
            .map(|c| c.get_name().to_owned())
            .chain(["help".to_owned()])
            .collect();
        let reserved: Vec<&str> = reserved.iter().map(String::as_str).collect();
        let (specs, _) = registry.commands(&reserved);
        let all: Vec<(String, Vec<CommandSpec>)> = registry
            .iter()
            .map(|e| (e.id().to_owned(), e.addon.commands()))
            .collect();
        let namespaced = namespaced(&all, &specs);
        for n in namespaced.iter().filter(|n| n.listed) {
            eprintln!(
                "swofty: {}: its command {} has the name of a built-in command or another addon's; run it as swofty {}",
                n.id, n.spec.name, n.name
            );
        }
        Self {
            registry,
            specs,
            namespaced,
            developer: crate::dev::enabled(store),
        }
    }

    /// The addon command `name` names, with the name the addon knows it by.
    pub(crate) fn find(&self, name: &str) -> Option<(&str, &CommandSpec)> {
        self.specs
            .iter()
            .find(|(_, s)| s.name == name)
            .map(|(id, s)| (id.as_str(), s))
            .or_else(|| {
                self.namespaced
                    .iter()
                    .find(|n| n.name == name)
                    .map(|n| (n.id.as_str(), &n.spec))
            })
    }

    pub(crate) fn provides(&self, name: &str) -> bool {
        self.specs.iter().any(|(_, s)| s.name == name)
    }

    pub(crate) fn run(
        &self,
        store: &Store,
        profile: &str,
        id: &str,
        spec: &CommandSpec,
        matches: &ArgMatches,
    ) -> Result<ExitCode> {
        let (mut path, mut input) = (vec![spec.name.clone()], Vec::new());
        command_input(matches, spec, &mut path, &mut input);
        if let Ok(cwd) = std::env::current_dir() {
            absolute_paths(&mut input, &cwd);
        }
        let entry = self.registry.get(id).context("addon is gone")?;
        let p = store.profile(profile)?;
        let game = crate::build::game(store, None).ok();
        let host = Host::new(store, id, &p, game.as_ref())?;
        let out = entry.addon.command(&host.cx(), &path, &input)?;
        if !out.is_empty() {
            println!("{}", out.trim_end());
        }
        Ok(ExitCode::SUCCESS)
    }

    pub(crate) fn command(&self) -> clap::Command {
        let mut cmd = Cli::command();
        if self.provides("mods") {
            cmd = cmd.mut_subcommand("addons", |c| c.alias(None));
        }
        let developer = self.developer;
        cmd = cmd.mut_subcommand("dev", |c| c.hide(!developer));
        let cmd = self
            .specs
            .iter()
            .fold(cmd, |cmd, (_, spec)| cmd.subcommand(clap_command(spec)));
        self.namespaced.iter().fold(cmd, |cmd, n| {
            let about = match n.listed {
                true => format!(
                    "{} ({}'s {} command: swofty {} is a built-in command or another addon's)",
                    n.spec.about, n.id, n.spec.name, n.spec.name
                ),
                false => n.spec.about.clone(),
            };
            cmd.subcommand(
                clap_command(&n.spec)
                    .name(n.name.clone())
                    .about(about)
                    .hide(!n.listed),
            )
        })
    }
}

fn clap_command(spec: &CommandSpec) -> clap::Command {
    let mut cmd = clap::Command::new(spec.name.clone()).about(spec.about.clone());
    for a in &spec.args {
        let arg = Arg::new(a.name.clone()).help(a.help.clone()).required(a.required);
        cmd = cmd.arg(if a.many {
            arg.num_args(1..).action(ArgAction::Append)
        } else {
            arg
        });
    }
    if !spec.subcommands.is_empty() && spec.args.is_empty() {
        cmd = cmd.subcommand_required(true);
    }
    spec.subcommands
        .iter()
        .fold(cmd, |cmd, s| cmd.subcommand(clap_command(s)))
}

pub(crate) fn command_input(
    m: &ArgMatches,
    spec: &CommandSpec,
    path: &mut Vec<String>,
    args: &mut Vec<String>,
) {
    for a in &spec.args {
        if let Some(values) = m.get_many::<String>(&a.name) {
            args.extend(values.cloned());
        }
    }
    if let Some((name, sub)) = m.subcommand()
        && let Some(s) = spec.subcommands.iter().find(|s| s.name == name)
    {
        path.push(name.to_owned());
        command_input(sub, s, path, args);
    }
}

/// Addon commands run in their own folder, so a value naming an existing
/// path relative to `cwd` is passed on as an absolute path.
pub(crate) fn absolute_paths(args: &mut [String], cwd: &Path) {
    for a in args {
        let at = cwd.join(a.as_str());
        if !a.is_empty()
            && Path::new(a.as_str()).is_relative()
            && at.exists()
            && let Some(abs) = std::path::absolute(&at)
                .ok()
                .and_then(|p| p.to_str().map(str::to_owned))
        {
            *a = abs;
        }
    }
}

pub(crate) fn command_at(args: &[OsString]) -> Option<usize> {
    let mut skip = false;
    args.iter().enumerate().skip(1).find_map(|(i, a)| {
        if std::mem::take(&mut skip) {
            return None;
        }
        let a = a.to_str()?;
        skip = a == "--data-dir" || a == "--profile";
        (!a.starts_with('-')).then_some(i)
    })
}

pub(crate) fn forward_maps(args: &mut [OsString]) {
    if let Some(at) = command_at(args).filter(|&at| args[at] == "maps") {
        args[at] = "mods".into();
    }
}

pub(crate) fn forward_mods(store: &Store, args: &[OsString]) -> Option<(Vec<OsString>, String)> {
    let at = command_at(args)?;
    if args[at] != "mods" {
        return None;
    }
    let verb = args.get(at + 1)?.to_str()?;
    let target = args.get(at + 2)?;
    let addon = match verb {
        "install" => Path::new(target).join(MANIFEST_FILE).is_file(),
        "enable" | "disable" => store.addons().ok()?.contains_key(target.to_str()?),
        _ => false,
    };
    if !addon {
        return None;
    }
    let mut forwarded = args.to_vec();
    forwarded[at] = "addons".into();
    Some((
        forwarded,
        format!("swofty mods {verb} is now swofty addons {verb}"),
    ))
}
