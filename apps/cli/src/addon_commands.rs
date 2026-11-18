use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, CommandFactory};
use swofty_core::addons::registry::{Host, Registry};
use swofty_core::launcher::CommandSpec;
use swofty_core::manifest::MANIFEST_FILE;
use swofty_core::store::Store;

use crate::commands::cli::Cli;

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
            developer: crate::commands::dev::enabled(store),
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
        let game = crate::commands::build::game(store, None).ok();
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use clap::FromArgMatches;
    use swofty_core::launcher::{Addon, ArgSpec, Builtin};
    use swofty_core::manifest::AddonManifest;

    use super::*;
    use crate::commands::cli::{Command, data_dir_arg};

    struct Mods(AddonManifest);

    impl Addon for Mods {
        fn manifest(&self) -> &AddonManifest {
            &self.0
        }

        fn commands(&self) -> Vec<CommandSpec> {
            let spec = |name: &str, args: Vec<ArgSpec>| CommandSpec {
                name: name.into(),
                about: String::new(),
                args,
                subcommands: Vec::new(),
            };
            let paths = ArgSpec {
                name: "path".into(),
                help: String::new(),
                required: true,
                many: true,
            };
            vec![
                CommandSpec {
                    subcommands: vec![spec("list", Vec::new()), spec("import", vec![paths])],
                    ..spec("mods", Vec::new())
                },
                spec("build", Vec::new()),
            ]
        }
    }

    fn store(root: &Path) -> Store {
        let manifest = AddonManifest::parse(
            r#"{"schema_version":1,"id":"t.mods","name":"Mods","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#,
        )
        .unwrap();
        Store::new(root).with_builtins(vec![Builtin {
            addon: Arc::new(Mods(manifest)),
            locked: false,
            default_on: true,
        }])
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn an_addon_command_a_built_in_took_stays_reachable_under_its_addon_id() {
        let spec = |name: &str| CommandSpec {
            name: name.into(),
            about: format!("does {name}"),
            args: Vec::new(),
            subcommands: Vec::new(),
        };
        let all = vec![("someone.tool".to_owned(), vec![spec("registry"), spec("tidy")])];
        let accepted = vec![("someone.tool".to_owned(), spec("tidy"))];
        let addons = AddonCommands {
            registry: Registry::default(),
            namespaced: namespaced(&all, &accepted),
            specs: accepted,
            developer: false,
        };
        let listed: Vec<(&str, bool)> = addons
            .namespaced
            .iter()
            .map(|n| (n.name.as_str(), n.listed))
            .collect();
        assert_eq!(
            listed,
            [("someone.tool:registry", true), ("someone.tool:tidy", false)]
        );
        let cmd = addons.command();
        for (typed, known_as) in [
            ("someone.tool:registry", "registry"),
            ("someone.tool:tidy", "tidy"),
            ("tidy", "tidy"),
        ] {
            let m = cmd.clone().try_get_matches_from(["swofty", typed]).unwrap();
            let (name, _) = m.subcommand().unwrap();
            let (id, spec) = addons.find(name).unwrap();
            assert_eq!((id, spec.name.as_str()), ("someone.tool", known_as));
        }
        let m = cmd
            .clone()
            .try_get_matches_from(["swofty", "registry", "check"])
            .unwrap();
        assert!(addons.find(m.subcommand().unwrap().0).is_none());
        let help = cmd.clone().render_help().to_string();
        assert!(help.contains("someone.tool:registry"), "{help}");
        assert!(!help.contains("someone.tool:tidy"), "{help}");
        for name in ["dev", "test", "registry", "careers"] {
            assert!(cmd.find_subcommand(name).is_some(), "{name}");
        }
    }

    #[test]
    fn addon_commands_take_mods_and_forward_the_old_forms() {
        let root = std::env::temp_dir().join(format!("swofty-cli-commands-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let addon = root.join("my-addon");
        std::fs::create_dir_all(&addon).unwrap();
        std::fs::write(addon.join(MANIFEST_FILE), "{}").unwrap();
        let store = store(&root.join("data"));
        let addons = AddonCommands::load(&store);
        assert_eq!(addons.specs.len(), 1);
        let cmd = addons.command();
        assert_eq!(
            cmd.find_subcommand("addons").unwrap().get_all_aliases().count(),
            0
        );
        let m = cmd
            .try_get_matches_from(["swofty", "--profile", "p", "mods", "import", "a.serps", "b.zip"])
            .unwrap();
        let (name, sub) = m.subcommand().unwrap();
        let (mut path, mut input) = (vec![name.to_owned()], Vec::new());
        command_input(sub, &addons.specs[0].1, &mut path, &mut input);
        assert_eq!(path, ["mods", "import"]);
        assert_eq!(input, ["a.serps", "b.zip"]);
        std::fs::write(root.join("b.zip"), "zip").unwrap();
        absolute_paths(&mut input, &root);
        assert_eq!(input, ["a.serps", root.join("b.zip").to_str().unwrap()]);
        let mut dotted = vec!["./my-addon".to_owned(), "on".to_owned()];
        absolute_paths(&mut dotted, &root);
        assert_eq!(dotted, [addon.to_str().unwrap(), "on"]);
        let dir = addon.to_str().unwrap();
        let (args, note) =
            forward_mods(&store, &os(&["swofty", "--profile", "p", "mods", "install", dir])).unwrap();
        assert_eq!(args, os(&["swofty", "--profile", "p", "addons", "install", dir]));
        assert_eq!(note, "swofty mods install is now swofty addons install");
        assert!(forward_mods(&store, &os(&["swofty", "mods", "enable", "swofty.core"])).is_some());
        assert!(forward_mods(&store, &os(&["swofty", "mods", "enable", "rm.livery"])).is_none());
        assert!(forward_mods(&store, &os(&["swofty", "mods", "install", "pack.serps"])).is_none());
        assert_eq!(
            data_dir_arg(&os(&["swofty", "build", "--data-dir", "/d"])),
            Some("/d".into())
        );
        assert_eq!(
            data_dir_arg(&os(&["swofty", "--data-dir=/e", "build"])),
            Some("/e".into())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_profile_that_starts_with_a_dash_goes_through_as_one_value() {
        use swofty_core::platform::process::flag;
        let root = std::env::temp_dir().join(format!("swofty-cli-dash-profile-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let store = store(&root);
        let cmd = AddonCommands::load(&store).command();
        let args = vec![
            OsString::from("swofty"),
            flag("data-dir", "/data/-swofty"),
            flag("profile", "-swofty"),
            "run".into(),
            "--apply".into(),
            flag("game", "/games/-F1 25"),
        ];
        let cli = Cli::from_arg_matches(&cmd.clone().try_get_matches_from(&args).unwrap()).unwrap();
        assert_eq!(cli.profile.as_deref(), Some("-swofty"));
        assert_eq!(cli.data_dir.as_deref(), Some(Path::new("/data/-swofty")));
        assert!(matches!(cli.command, Command::Run(_)));
        assert_eq!(command_at(&args), Some(3));
        assert_eq!(data_dir_arg(&args), Some("/data/-swofty".into()));

        let split = cmd
            .try_get_matches_from(["swofty", "--profile", "-swofty", "run", "--apply"])
            .unwrap_err();
        assert_eq!(split.kind(), clap::error::ErrorKind::UnknownArgument);
        std::fs::remove_dir_all(&root).ok();
    }
}
