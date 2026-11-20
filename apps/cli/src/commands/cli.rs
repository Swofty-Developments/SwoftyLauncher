use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::commands::{
    addons, build, db, dev, game, games, hud, laps, logs, offline, postfx, profile, registry, run, saves,
    settings, setup, shaders, steam, testing, update,
};

#[derive(Parser)]
#[command(
    name = "swofty",
    version,
    about = "Swofty Launcher for F1 25: addons, mods and tracks"
)]
pub(crate) struct Cli {
    #[arg(
        long,
        global = true,
        env = "SWOFTY_DATA_DIR",
        help = "Data directory (default: platform data dir)"
    )]
    pub(crate) data_dir: Option<PathBuf>,
    #[arg(long, global = true, help = "Profile (default: the active profile)")]
    pub(crate) profile: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

pub(crate) const GAME_HELP: &str = "Game folder (default: the active copy, see swofty game list)";

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(about = "Launch F1 25 modded and offline, streaming logs; use as the Steam launch option")]
    Run(run::RunArgs),
    #[command(about = "Resolve the profile and build its overlay")]
    Build(build::BuildArgs),
    #[command(about = "Show which addons override which in the profile's last build")]
    Conflicts(build::ConflictsArgs),
    #[command(
        about = "Build the profile and install it into the game folder (database, tracks); swofty restore undoes it"
    )]
    Apply(build::ApplyArgs),
    #[command(about = "Report drift from the baseline and check the game files parse and round-trip")]
    Verify(build::VerifyArgs),
    #[command(
        about = "Put the game folder and prefix back to the baseline: roll back journals, remove additions"
    )]
    Restore(build::RestoreArgs),
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
    Db(db::DbArgs),
    #[command(
        about = "Summarise a previous session's logs and crashes; `logs send` sends your logs to the developers"
    )]
    Logs(logs::LogsArgs),
    #[command(about = "Report what was discovered: Steam, the game, Proton, the launch option and the store")]
    Doctor,
    #[command(
        subcommand,
        about = "List the copies of F1 25 (Steam, EA app, added folders) and pick the one to play"
    )]
    Game(game::GameCommand),
    #[command(
        about = "List the games the launcher knows, whether each is supported, and where each was found"
    )]
    Games(games::GamesArgs),
    #[command(
        about = "Show whether Steam is running and signed in; --start starts it and waits, as Play does"
    )]
    Steam(steam::SteamArgs),
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
    Update(update::UpdateArgs),
}

pub(crate) fn data_dir_arg(args: &[OsString]) -> Option<PathBuf> {
    args.iter().enumerate().find_map(|(i, a)| {
        if a == "--data-dir" {
            return args.get(i + 1).map(PathBuf::from);
        }
        a.to_str()?.strip_prefix("--data-dir=").map(PathBuf::from)
    })
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;
    use crate::commands::addons::AddonsCommand;
    use crate::commands::logs::{LogsArgs, LogsCommand};

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn logs_takes_a_session_folder_or_send() {
        let parse = |args: &[&str]| Cli::try_parse_from(os(args)).map(|c| c.command);
        let Ok(Command::Logs(LogsArgs {
            action: None, dir, ..
        })) = parse(&["swofty", "logs", "/tmp/s1"])
        else {
            panic!("logs <dir>");
        };
        assert_eq!(dir, Some(PathBuf::from("/tmp/s1")));
        assert!(matches!(
            parse(&["swofty", "logs"]),
            Ok(Command::Logs(LogsArgs {
                action: None,
                dir: None,
                ..
            }))
        ));
        let Ok(Command::Logs(LogsArgs {
            action: Some(LogsCommand::Send(a)),
            dir: None,
            ..
        })) = parse(&[
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
        let Ok(Command::Logs(LogsArgs {
            action: Some(LogsCommand::Send(a)),
            ..
        })) = parse(&["swofty", "logs", "send", "--dry-run"])
        else {
            panic!("--dry-run");
        };
        assert_eq!(a.dry_run, Some(None));
        let Ok(Command::Logs(LogsArgs {
            action: Some(LogsCommand::Send(a)),
            ..
        })) = parse(&["swofty", "logs", "send", "--name", "x1", "--dry-run", "out.zip"])
        else {
            panic!("--dry-run <zip>");
        };
        assert_eq!(a.dry_run, Some(Some(PathBuf::from("out.zip"))));
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
