use std::path::PathBuf;

use crate::commands::{Execute, Invocation};
use anyhow::{Context, Result};
use clap::Subcommand;
use std::process::ExitCode;
use swofty_core::manifest::Binds;
use swofty_core::store::Store;

impl Execute for SettingsCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, &invocation.profile, self).map(|()| ExitCode::SUCCESS)
    }
}

#[derive(Subcommand)]
pub(crate) enum SettingsCommand {
    #[command(about = "List every addon's settings with the profile's values")]
    List { id: Option<String> },
    #[command(about = "Set an addon setting; the value is JSON (true, 5, \"replica\") or plain text")]
    Set {
        id: String,
        option: String,
        value: String,
    },
    #[command(about = "Put an addon's settings back to their defaults")]
    Reset { id: String },
    #[command(about = "Write the session settings file (swofty/settings.json) for the profile")]
    Export {
        #[arg(long, help = "File to write (default: stdout)")]
        out: Option<PathBuf>,
    },
    #[command(about = "Merge the changes made in game into the profile")]
    Merge,
}

pub(crate) fn command(store: &Store, profile: &str, cmd: SettingsCommand) -> Result<()> {
    let addons = store.addons()?;
    let installed = |id: &str| addons.get(id).with_context(|| format!("{id} is not installed"));
    match cmd {
        SettingsCommand::List { id } => {
            let p = store.profile(profile)?;
            let pending = p.rebuild_pending(&addons);
            let on = swofty_core::addons::settings::OnAddons::of(&p, &addons);
            for m in addons
                .values()
                .filter(|m| id.as_deref().is_none_or(|i| i == m.id))
            {
                if m.options.is_empty() {
                    continue;
                }
                println!("{} ({})", m.name, m.id);
                for o in &m.options {
                    let v = on.value(&p, m, o);
                    let mark = match on.missing(o) {
                        Some(n) => format!(", off: needs {n} on"),
                        None if pending.contains(&format!("{}.{}", m.id, o.id)) => ", rebuild pending".into(),
                        None => String::new(),
                    };
                    let binds = match o.binds {
                        Binds::Native => "live",
                        Binds::Db => "rebuild",
                    };
                    println!("  {:24} {:16} {binds}{mark}", o.id, o.text(&v));
                }
            }
        }
        SettingsCommand::Set { id, option, value } => {
            let m = installed(&id)?;
            let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
            let mut p = store.profile(profile)?;
            let changed = p.set_option(m, &option, value)?;
            store.save_profile(&p)?;
            let o = m
                .options
                .iter()
                .find(|o| o.id == option)
                .expect("set_option checked it");
            println!(
                "{id}.{option} = {}{}",
                o.text(&p.option(m, o)),
                if !changed {
                    " (unchanged)"
                } else if o.binds == Binds::Db {
                    "; rebuild to apply"
                } else {
                    ""
                }
            );
        }
        SettingsCommand::Reset { id } => {
            let m = installed(&id)?;
            let mut p = store.profile(profile)?;
            p.reset_options(m)?;
            store.save_profile(&p)?;
            println!("{id}: settings back to their defaults in profile {profile}");
        }
        SettingsCommand::Export { out } => {
            let s = store.session_settings(profile)?;
            let text = serde_json::to_string_pretty(&s)? + "\n";
            match out {
                Some(path) => {
                    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(&path, text)?;
                    println!("wrote {}", path.display());
                }
                None => print!("{text}"),
            }
        }
        SettingsCommand::Merge => match store.merge_requests(profile, true)? {
            None => println!("no in-game changes for profile {profile}"),
            Some(m) => report_merge(&m),
        },
    }
    Ok(())
}

pub(crate) fn report_merge(m: &swofty_core::addons::settings::Merge) {
    for c in &m.changed {
        println!("in game: {c}");
    }
    for s in &m.skipped {
        println!("in game, skipped: {s}");
    }
    if m.changed.is_empty() && m.skipped.is_empty() {
        println!("in game: no changes");
    }
}
