mod bag;
mod browse;
mod manage;
mod plain;
mod session;

use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Subcommand;
use swofty_core::saves::launcher as launcher_saves;
use swofty_core::saves::vault::{self, Location, Vault};
use swofty_core::store::Store;

use crate::commands::saves::browse::{careers, list, show, versions};
use crate::commands::saves::manage::{backup, export_saves, import_saves, pin_snapshot, restore_snapshot};
use crate::commands::saves::plain::{careers_in_plain, restore_career};
use crate::commands::{Execute, Invocation};
pub use session::{
    after_session, before_session, learn_stuck_weekends, notice_since, withdraw_unanswered_restores,
};

impl Execute for SavesCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, self)
    }
}

#[derive(Subcommand)]
pub enum SavesCommand {
    #[command(
        about = "List the career vault's snapshots of the profile save, newest first, with the cloud status"
    )]
    List {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "Show one snapshot: its files with their hashes, Steam's record of them, and its events"
    )]
    Show {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Snapshot the profile save now and pin it, for example before playing from Steam")]
    Backup,
    #[command(
        about = "Copy a snapshot back over the profile save (F1 25 and Steam, or the EA app, must be closed); snapshots the current save first"
    )]
    Restore {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "List the versions of the profile save with the careers each holds, its cloud status, and what restoring it gets back and loses"
    )]
    Versions {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "List the careers the vault has seen, with their status and the addons they need")]
    Careers {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Print the tree of a decoded root container (a .bag file from the plain save store)")]
    DecodeBag {
        #[arg(help = "The .bag file")]
        file: std::path::PathBuf,
        #[arg(long, help = "Print the whole tree as JSON")]
        json: bool,
    },
    #[command(
        about = "List the careers of the plain copy the game writes, where each lives, and the core's answers to restore requests"
    )]
    Plain {
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "Ask the core to put one mode, both its containers, back from the plain copy at the next Play"
    )]
    RestoreCareer {
        #[arg(help = "The mode, as `swofty saves plain` prints it, or the name of either of its containers")]
        mode: String,
        #[arg(
            long,
            help = "The generation to restore from (default: the newest save that has it)"
        )]
        generation: Option<u64>,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(
        about = "Map the row ids that moved (swofty/id_aliases.json) in the careers of a plain store; --dry-run only reports"
    )]
    RemapIds {
        #[arg(long, help = "The plain root: the folder that holds CURRENT and generations/")]
        plain: std::path::PathBuf,
        #[arg(long, help = "The build's id_aliases.json")]
        aliases: std::path::PathBuf,
        #[arg(long, help = "Report what would be mapped and write nothing")]
        dry_run: bool,
        #[arg(
            long,
            help = "Also report what mapping this save data file (NXDSMWW.BWW) would do; --dry-run only"
        )]
        save: Option<std::path::PathBuf>,
    },
    #[command(about = "Keep a snapshot: retention never removes it")]
    Pin {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Undo `pin`; reasons other than your own pin keep the snapshot")]
    Unpin {
        #[arg(help = "The snapshot, as `swofty saves list` prints it")]
        snapshot: String,
        #[arg(long, help = SOURCE_HELP)]
        source: Option<String>,
    },
    #[command(about = "Show or set the career vault's size cap in MB (500 by default)")]
    Cap { mb: Option<u64> },
    #[command(
        about = "Copy a Steam save, or another profile's launcher saves, into a profile's launcher saves (F1 25 must be closed); keeps what they held in the vault first"
    )]
    Import {
        #[arg(
            long,
            value_name = "steam",
            conflicts_with = "from_profile",
            help = "Import the Steam save (the default)"
        )]
        from: Option<String>,
        #[arg(
            long,
            value_name = "PROFILE",
            help = "Import another profile's launcher saves"
        )]
        from_profile: Option<String>,
        #[arg(long, help = "The profile to import into (default: the active profile)")]
        profile: Option<String>,
    },
    #[command(
        about = "Copy a profile's launcher saves over the Steam save (F1 25 and Steam must be closed); keeps the Steam save in the vault first"
    )]
    Export {
        #[arg(long, value_name = "steam", help = "Where to copy to; only steam")]
        to: String,
        #[arg(long, help = "The profile to export (default: the active profile)")]
        profile: Option<String>,
    },
}

const PINNED: &str = "pinned";

const SOURCE_HELP: &str =
    "The vault to use, steam-<id>, ea-<user> or launcher-<profile> (default: the save of the copy Play uses)";

fn default_location(store: &Store, active: Option<Location>) -> Option<Location> {
    let name = store.active_profile();
    let uses_launcher_saves = store
        .profile(&name)
        .is_ok_and(|p| !p.is_vanilla() && p.launcher_saves);
    if !uses_launcher_saves {
        return active;
    }
    match &active {
        Some(l) if !launcher_saves::supported(l.layout) => active,
        Some(l) => launcher_saves::store_for(&store.root, &name, l),
        None => Some(launcher_saves::location(&store.root, &name)),
    }
}

fn open(store: &Store, source: Option<&str>) -> Result<(Vault, Option<Location>)> {
    let active = Location::active(&store.root);
    match source {
        Some(s) => Ok((
            Vault::open(&store.root, s)?,
            active
                .filter(|l| l.source == s)
                .or_else(|| Location::of_source(&store.root, s)),
        )),
        None => match default_location(store, active) {
            Some(l) => Ok((Vault::of(&store.root, &l)?, Some(l))),
            None => match Vault::sources(&store.root).as_slice() {
                [only] => Ok((Vault::open(&store.root, only)?, None)),
                [] => bail!(
                    "no F1 25 profile save found and the career vault in {} is empty",
                    Vault::root(&store.root).display()
                ),
                many => bail!(
                    "no F1 25 profile save found; pass --source with one of {}",
                    many.join(", ")
                ),
            },
        },
    }
}

pub fn command(store: &Store, cmd: SavesCommand) -> Result<ExitCode> {
    match cmd {
        SavesCommand::Import {
            from,
            from_profile,
            profile,
        } => import_saves(store, from, from_profile, profile)?,
        SavesCommand::Export { to, profile } => export_saves(store, &to, profile)?,
        SavesCommand::List { source } => list(store, source.as_deref())?,
        SavesCommand::Show { snapshot, source } => show(store, &snapshot, source.as_deref())?,
        SavesCommand::Versions { source } => versions(store, source.as_deref())?,
        SavesCommand::Careers { source } => careers(store, source.as_deref())?,
        SavesCommand::Pin { snapshot, source } => pin_snapshot(store, &snapshot, source.as_deref(), true)?,
        SavesCommand::Unpin { snapshot, source } => pin_snapshot(store, &snapshot, source.as_deref(), false)?,
        SavesCommand::DecodeBag { file, json } => bag::decode_bag(&file, json)?,
        SavesCommand::Plain { source } => careers_in_plain(store, source.as_deref())?,
        SavesCommand::RestoreCareer {
            mode,
            generation,
            source,
        } => restore_career(store, &mode, generation, source.as_deref())?,
        SavesCommand::RemapIds {
            plain,
            aliases,
            dry_run,
            save,
        } => plain::remap_ids(&plain, &aliases, dry_run, save.as_deref())?,
        SavesCommand::Backup => backup(store)?,
        SavesCommand::Restore { snapshot, source } => restore_snapshot(store, &snapshot, source.as_deref())?,
        SavesCommand::Cap { mb: None } => println!(
            "{} MB",
            store.config().vault_cap_mb.unwrap_or(vault::DEFAULT_CAP_MB)
        ),
        SavesCommand::Cap { mb: Some(n) } => {
            if n == 0 {
                bail!("the cap must be at least 1 MB");
            }
            let mut c = store.config();
            c.vault_cap_mb = Some(n);
            store.save_config(&c)?;
            println!("the career vault keeps at most {n} MB; the next snapshot prunes to it");
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
