use anyhow::{Result, bail};
use clap::Subcommand;
use swofty_core::store::Store;

use crate::commands::{Execute, Invocation};
use crate::console::prompt;
use std::process::ExitCode;

impl Execute for ProfileCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, self).map(|()| ExitCode::SUCCESS)
    }
}

#[derive(Subcommand)]
pub(crate) enum ProfileCommand {
    #[command(about = "List the profiles and mark the active one")]
    List,
    #[command(
        about = "Delete a profile with its settings, build cache and saves; the active profile switches first"
    )]
    Delete {
        #[arg(help = "The profile to delete")]
        name: String,
        #[arg(long, short, help = "Delete without asking")]
        yes: bool,
    },
}

pub(crate) fn command(store: &Store, cmd: ProfileCommand) -> Result<()> {
    match cmd {
        ProfileCommand::List => {
            let active = store.active_profile();
            for name in store.profile_names()? {
                let note = if name == active {
                    "active"
                } else if name == swofty_core::profile::VANILLA {
                    "built in"
                } else {
                    ""
                };
                let line = format!("{name:40} {note}");
                println!("{}", line.trim_end());
            }
        }
        ProfileCommand::Delete { name, yes } => {
            let plan = store.plan_profile_deletion(&name)?;
            if swofty_core::platform::process::game_running() {
                bail!("close F1 25 before deleting a profile");
            }
            if !yes {
                confirm_delete(&plan)?;
            }
            let done = store.delete_profile(&name)?;
            if let Some(to) = &done.switch_to {
                println!("{to} is now the active profile");
            }
            println!("deleted profile {name}");
        }
    }
    Ok(())
}

fn confirm_delete(plan: &swofty_core::store::ProfileDeletion) -> Result<()> {
    let name = &plan.name;
    if !prompt::interactive() {
        bail!("stdin is not a terminal: pass --yes to delete {name} without asking");
    }
    println!(
        "Deleting {name} removes its choice of addons and mods, their settings{}.",
        if plan.saves {
            ", its build cache and its saves"
        } else {
            " and its build cache"
        }
    );
    println!("Installed addons and mods stay in the library, and other profiles don't change.");
    if let Some(to) = &plan.switch_to {
        println!("{name} is the active profile, so {to} becomes active first.");
    }
    if !prompt::yes(&format!("Delete profile {name}?"))? {
        bail!("not deleted: {name} is unchanged");
    }
    Ok(())
}
