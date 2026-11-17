mod addon_commands;
mod commands;
mod console;
mod recover;
mod signals;
mod stores;
mod time;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{FromArgMatches, Parser, Subcommand};
use swofty_core::store::Store;

use crate::addon_commands::AddonCommands;
use crate::commands::{
    addons, build, db, dev, doctor, game, hud, laps, logs, offline, postfx, profile, registry, run, saves,
    settings, setup, shaders, testing, update,
};
use crate::stores::steam;

#[derive(Parser)]
#[command(
    name = "swofty",
    version,
    about = "Swofty Launcher for F1 25: addons, mods and tracks"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "SWOFTY_DATA_DIR",
        help = "Data directory (default: platform data dir)"
    )]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true, help = "Profile (default: the active profile)")]
    profile: Option<String>,
    #[command(subcommand)]
    command: Command,
}

const GAME_HELP: &str = "Game folder (default: the active copy, see swofty game list)";

#[derive(Subcommand)]
enum Command {
    #[command(about = "Launch F1 25 modded and offline, streaming logs; use as the Steam launch option")]
    Run(run::RunArgs),
    #[command(about = "Resolve the profile and build its overlay")]
    Build {
        #[arg(long, help = GAME_HELP)]
        game: Option<PathBuf>,
        #[arg(
            long,
            help = "Also list every side-data finding of add-row copies: known lookups, matches to review, matches in other game areas and acknowledged ones"
        )]
        side_data: bool,
    },
    #[command(about = "Show which addons override which in the profile's last build")]
    Conflicts {
        #[arg(long, help = "Build the profile without installing it and show its conflicts")]
        check: bool,
        #[arg(long, help = GAME_HELP)]
        game: Option<PathBuf>,
    },
    #[command(
        about = "Build the profile and install it into the game folder (database, tracks); swofty restore undoes it"
    )]
    Apply {
        #[arg(long, help = GAME_HELP)]
        game: Option<PathBuf>,
    },
    #[command(about = "Report drift from the baseline and check the game files parse and round-trip")]
    Verify {
        #[arg(long)]
        game: Option<PathBuf>,
    },
    #[command(
        about = "Put the game folder and prefix back to the baseline: roll back journals, remove additions"
    )]
    Restore {
        #[arg(long)]
        game: Option<PathBuf>,
    },
    #[command(
        subcommand,
        alias = "mods",
        about = "List, install, enable and disable addons"
    )]
    Addons(addons::AddonsCommand),
    #[command(
        subcommand,
        about = "Offline play: consent and the launch extension that runs sessions without EA AntiCheat"
    )]
    Offline(offline::OfflineCommand),
    #[command(
        subcommand,
        about = "The one-time setup before modded play: put the game folder back, verify it, then play"
    )]
    Setup(setup::SetupCommand),
    #[command(subcommand, about = "Show and change addon settings in the profile")]
    Settings(settings::SettingsCommand),
    #[command(subcommand, about = "List and delete profiles")]
    Profile(profile::ProfileCommand),
    #[command(about = "Browse the game database and edit DB Editor addons")]
    Db {
        #[arg(long, help = GAME_HELP)]
        game: Option<PathBuf>,
        #[command(subcommand)]
        cmd: db::DbCommand,
    },
    #[command(
        about = "Summarise a previous session's logs and crashes; `logs send` sends your logs to the developers"
    )]
    Logs {
        #[command(subcommand)]
        action: Option<LogsCommand>,
        dir: Option<PathBuf>,
        #[arg(long, env = "SWOFTY_EXE_COPY")]
        exe: Option<PathBuf>,
    },
    #[command(about = "Report what was discovered: Steam, the game, Proton, the launch option and the store")]
    Doctor,
    #[command(
        subcommand,
        about = "List the copies of F1 25 (Steam, EA app, added folders) and pick the one to play"
    )]
    Game(game::GameCommand),
    #[command(
        about = "Show whether Steam is running and signed in; --start starts it and waits, as Play does"
    )]
    Steam {
        #[arg(
            long,
            help = "Start Steam if it's closed and wait up to 90 s for it to sign in"
        )]
        start: bool,
    },
    #[command(
        subcommand,
        about = "Shader configs and effects: list, edit, check and choose the profile's config"
    )]
    Shaders(shaders::ShadersCommand),
    #[command(
        subcommand,
        about = "Post-process passes: compile an addon's shaders to check them"
    )]
    Postfx(postfx::PostfxCommand),
    #[command(
        subcommand,
        about = "The career vault: snapshots of the profile save around every session; list, show, back up and restore them"
    )]
    Saves(saves::SavesCommand),
    #[command(
        subcommand,
        about = "Time Trial boards: the laps waiting to upload and the ones sent"
    )]
    Laps(laps::LapsCommand),
    #[command(
        subcommand,
        about = "Test addons in the game: plan the game starts, run the tests and write the report"
    )]
    Test(testing::TestCommand),
    #[command(
        subcommand,
        about = "Developer mode: make, link, run, check, test and pack an addon"
    )]
    Dev(dev::DevCommand),
    #[command(
        subcommand,
        about = "The addon registry: check dependencies, and show the core's conflicts and status from your last session"
    )]
    Registry(registry::RegistryCommand),
    #[command(
        subcommand,
        about = "The HUD composer: check the stock dial facts and compose an addon's hud.element records"
    )]
    Hud(hud::HudCommand),
    #[command(about = "List every career in the career index, the profile that started it and what it needs")]
    Careers,
    #[command(about = "Check for a new release and install it into this copy")]
    Update {
        #[arg(long, help = "Only report whether a new release is available")]
        check: bool,
    },
}

#[derive(Subcommand)]
enum LogsCommand {
    #[command(
        about = "Send the launcher and game logs to the developers on Discord, with your username and what went wrong"
    )]
    Send(logs::send::SendArgs),
}

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

fn data_dir_arg(args: &[OsString]) -> Option<PathBuf> {
    args.iter().enumerate().find_map(|(i, a)| {
        if a == "--data-dir" {
            return args.get(i + 1).map(PathBuf::from);
        }
        a.to_str()?.strip_prefix("--data-dir=").map(PathBuf::from)
    })
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
    let store = store.clone();
    let named = cli.profile.clone();
    let profile = cli.profile.unwrap_or_else(|| store.active_profile());
    match cli.command {
        Command::Run(args) => run::run(&store, &profile, args),
        Command::Build { game, side_data } => build::build(&store, &profile, game, side_data),
        Command::Conflicts { check, game } => build::conflicts(&store, &profile, check, game),
        Command::Apply { game } => build::apply(&store, &profile, game),
        Command::Verify { game } => build::verify(&store, game),
        Command::Restore { game } => build::restore(&store, game),
        Command::Addons(cmd) => addons::command(&store, &profile, cmd).map(|_| ExitCode::SUCCESS),
        Command::Offline(cmd) => offline::command(&store, cmd).map(|_| ExitCode::SUCCESS),
        Command::Setup(cmd) => setup::command(&store, cmd).map(|_| ExitCode::SUCCESS),
        Command::Settings(cmd) => settings::command(&store, &profile, cmd).map(|_| ExitCode::SUCCESS),
        Command::Profile(cmd) => profile::command(&store, cmd).map(|_| ExitCode::SUCCESS),
        Command::Db { game, cmd } => db::command(&store, &profile, game, cmd),
        Command::Logs {
            action: Some(LogsCommand::Send(args)),
            ..
        } => logs::send::send(&store, args),
        Command::Logs { dir, exe, .. } => logs::summarise(&store, dir, exe),
        Command::Doctor => doctor::doctor(&store),
        Command::Game(cmd) => game::command(&store, cmd).map(|_| ExitCode::SUCCESS),
        Command::Saves(cmd) => saves::command(&store, cmd),
        Command::Laps(cmd) => laps::command(&store, &profile, cmd),
        Command::Steam { start } => steam::check(&store, start),
        Command::Postfx(cmd) => postfx::command(cmd),
        Command::Shaders(cmd) => shaders::command(&store, &profile, cmd),
        Command::Test(cmd) => testing::command(&store, named.as_deref(), &profile, cmd),
        Command::Dev(cmd) => dev::command(&store, &profile, cmd),
        Command::Update { check } => update::update(check),
        Command::Registry(cmd) => registry::command(&store, &profile, cmd),
        Command::Hud(cmd) => hud::command(&store, cmd),
        Command::Careers => registry::careers(&store, &profile).map(|_| ExitCode::SUCCESS),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;
    use std::sync::Arc;

    use clap::CommandFactory;
    use swofty_core::launcher::CommandSpec;
    use swofty_core::manifest::MANIFEST_FILE;
    use swofty_core::registry::Registry;

    use swofty_core::launcher::{Addon, ArgSpec, Builtin};
    use swofty_core::manifest::AddonManifest;

    use super::*;
    use crate::addon_commands::{absolute_paths, command_at, command_input, forward_mods, namespaced};
    use crate::commands::addons::AddonsCommand;

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
    fn logs_takes_a_session_folder_or_send() {
        let parse = |args: &[&str]| Cli::try_parse_from(os(args)).map(|c| c.command);
        let Ok(Command::Logs {
            action: None, dir, ..
        }) = parse(&["swofty", "logs", "/tmp/s1"])
        else {
            panic!("logs <dir>");
        };
        assert_eq!(dir, Some(PathBuf::from("/tmp/s1")));
        assert!(matches!(
            parse(&["swofty", "logs"]),
            Ok(Command::Logs {
                action: None,
                dir: None,
                ..
            })
        ));
        let Ok(Command::Logs {
            action: Some(LogsCommand::Send(a)),
            dir: None,
            ..
        }) = parse(&[
            "swofty",
            "logs",
            "send",
            "--name",
            "swofty",
            "--message",
            "it crashed",
        ])
        else {
            panic!("logs send");
        };
        assert_eq!(
            (a.name.as_deref(), a.message.as_deref()),
            (Some("swofty"), Some("it crashed"))
        );
        assert_eq!(a.dry_run, None);
        let Ok(Command::Logs {
            action: Some(LogsCommand::Send(a)),
            ..
        }) = parse(&["swofty", "logs", "send", "--dry-run"])
        else {
            panic!("--dry-run");
        };
        assert_eq!(a.dry_run, Some(None));
        let Ok(Command::Logs {
            action: Some(LogsCommand::Send(a)),
            ..
        }) = parse(&["swofty", "logs", "send", "--name", "x1", "--dry-run", "out.zip"])
        else {
            panic!("--dry-run <zip>");
        };
        assert_eq!(a.dry_run, Some(Some(PathBuf::from("out.zip"))));
    }

    #[test]
    fn logs_send_dry_run_writes_the_zip() {
        let root = std::env::temp_dir().join(format!("swofty-cli-sendlogs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data = root.join("data");
        std::fs::create_dir_all(data.join("state")).unwrap();
        std::fs::write(data.join("state/launcher-run.log"), "hello\n").unwrap();
        std::fs::write(data.join("config.json"), r#"{"discord_username":"remembered"}"#).unwrap();
        let store = Store::new(&data);
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let args = logs::send::SendArgs {
            name: None,
            message: Some("  it froze on the grid  ".into()),
            dry_run: Some(Some(out.clone())),
        };
        assert_eq!(logs::send::send(&store, args).unwrap(), ExitCode::SUCCESS);
        let written: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(written.len(), 1);
        let name = written[0].to_string_lossy().into_owned();
        assert!(
            name.starts_with("swofty-logs-remembered-") && name.ends_with(".zip"),
            "{name}"
        );
        let zip = zip_names(&out.join(&name));
        assert!(zip.contains(&"state/launcher-run.log".to_owned()), "{zip:?}");
        assert!(zip.contains(&"summary.txt".to_owned()));
        let bad = logs::send::SendArgs {
            name: Some("x".into()),
            message: None,
            dry_run: Some(None),
        };
        assert!(logs::send::send(&store, bad).is_err());
        let unsaid = logs::send::SendArgs {
            name: None,
            message: None,
            dry_run: Some(Some(out.join("unsaid.zip"))),
        };
        assert_eq!(logs::send::send(&store, unsaid).unwrap(), ExitCode::SUCCESS);
        assert!(out.join("unsaid.zip").is_file());
        let sending = |message: Option<&str>| logs::send::SendArgs {
            name: None,
            message: message.map(Into::into),
            dry_run: None,
        };
        let err = logs::send::send_to(&store, sending(None), None).unwrap_err();
        assert!(err.to_string().contains("pass --message"), "{err}");
        let err = logs::send::send_to(&store, sending(Some("  it froze ")), None).unwrap_err();
        assert!(err.to_string().contains("at least 10 characters"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }

    fn stub_log_server(code: u16) -> (String, std::thread::JoinHandle<usize>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/logs", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 16 << 10];
            let end = loop {
                if let Some(head) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let text = String::from_utf8_lossy(&buf[..head]).to_ascii_lowercase();
                    let len: usize = text
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    break head + 4 + len;
                }
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed early");
                buf.extend_from_slice(&chunk[..n]);
            };
            while buf.len() < end {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed early");
                buf.extend_from_slice(&chunk[..n]);
            }
            write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )
            .unwrap();
            end
        });
        (url, handle)
    }

    #[test]
    fn logs_send_keeps_the_zip_in_the_data_folder() {
        let root = std::env::temp_dir().join(format!("swofty-cli-keeplogs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data = root.join("data");
        std::fs::create_dir_all(data.join("state")).unwrap();
        std::fs::write(data.join("state/launcher-run.log"), "hello\n").unwrap();
        let store = Store::new(&data);
        let kept = |store: &Store| -> Vec<PathBuf> {
            let mut zips: Vec<PathBuf> = std::fs::read_dir(swofty_core::send_logs::kept_dir(store))
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            zips.sort();
            zips
        };
        let args = |name: &str| logs::send::SendArgs {
            name: Some(name.into()),
            message: Some("it froze on the grid".into()),
            dry_run: None,
        };

        let (url, server) = stub_log_server(200);
        let hook = swofty_core::send_logs::LogServer::new(Some(&url));
        assert_eq!(
            logs::send::send_to(&store, args("sender"), hook).unwrap(),
            ExitCode::SUCCESS
        );
        assert!(server.join().unwrap() > 0);
        let zips = kept(&store);
        assert_eq!(zips.len(), 1, "{zips:?}");
        assert_eq!(zips[0].parent(), Some(data.join("logs").as_path()));
        let name = zips[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("swofty-logs-sender-") && name.ends_with(".zip"),
            "{name}"
        );
        assert!(zip_names(&zips[0]).contains(&"state/launcher-run.log".to_owned()));
        assert_eq!(store.config().discord_username.as_deref(), Some("sender"));

        let (url, server) = stub_log_server(400);
        let hook = swofty_core::send_logs::LogServer::new(Some(&url));
        let err = logs::send::send_to(&store, args("refused"), hook).unwrap_err();
        assert!(err.to_string().contains("HTTP 400"), "{err}");
        server.join().unwrap();
        let zips = kept(&store);
        assert_eq!(zips.len(), 2, "a refused send keeps its zip too: {zips:?}");
        assert!(zips.iter().any(|z| {
            z.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("swofty-logs-refused-")
        }));

        let err = logs::send::send_to(&store, args("nowhere"), None).unwrap_err();
        assert!(err.to_string().contains("--dry-run"), "{err}");
        assert_eq!(kept(&store).len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let data = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&data);
        [
            "summary.txt",
            "config.json",
            "addons.json",
            "state/launcher-run.log",
        ]
        .iter()
        .filter(|n| text.contains(*n))
        .map(|n| (*n).to_owned())
        .collect()
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
        use swofty_core::process::flag;
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

    #[test]
    fn mods_is_a_hidden_alias_of_addons() {
        for name in ["addons", "mods"] {
            let cli = Cli::try_parse_from(["swofty", name, "enable", "a.b"]).unwrap();
            assert!(matches!(cli.command, Command::Addons(AddonsCommand::Enable { id }) if id == "a.b"));
        }
        let cmd = Cli::command();
        let addons = cmd.find_subcommand("addons").unwrap();
        assert_eq!(addons.get_all_aliases().collect::<Vec<_>>(), ["mods"]);
        assert_eq!(addons.get_visible_aliases().count(), 0);
    }
}
