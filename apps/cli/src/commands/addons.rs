use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use swofty_core::saves::SavesWarning;
use swofty_core::store::Store;

use crate::commands::registry;
use crate::commands::{Execute, Invocation};
use crate::console::prompt;
use std::process::ExitCode;

impl Execute for AddonsCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, &invocation.profile, self).map(|()| ExitCode::SUCCESS)
    }
}

#[derive(Subcommand)]
pub(crate) enum AddonsCommand {
    List,
    Install {
        path: PathBuf,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
        #[arg(long, short, help = "Don't ask, even when the addon has affected your saves")]
        yes: bool,
    },
    #[command(about = "Delete an installed addon and take it out of every profile")]
    Remove {
        id: String,
        #[arg(long, short, help = "Don't ask, even when the addon has affected your saves")]
        yes: bool,
    },
    #[command(about = "Read every file of the installed addons, or of one, and list the damaged ones")]
    Verify {
        id: Option<String>,
    },
}

pub(crate) fn command(store: &Store, profile: &str, cmd: AddonsCommand) -> Result<()> {
    match cmd {
        AddonsCommand::List => {
            let status = registry::status(store);
            let p = store.profile(profile)?;
            let enabled: Vec<&str> = p.enabled().collect();
            for (id, m) in store.addons()? {
                let state = if m.is_builtin() {
                    "built in"
                } else if m.is_shader_only() {
                    "shaders"
                } else if enabled.contains(&id.as_str()) {
                    "enabled"
                } else {
                    "disabled"
                };
                println!("{id:32} {:10} {state:9} {}", m.version.to_string(), m.name);
                for line in status.get(&id).into_iter().flatten() {
                    println!("{:32} {:10} {:9}   last session: {line}", "", "", "");
                }
            }
            for u in store.unreadable_addons() {
                let version = u.version.as_deref().unwrap_or("?");
                println!(
                    "{:32} {version:10} {:9} {}: it {}",
                    u.id, "unusable", u.name, u.reason
                );
            }
        }
        AddonsCommand::Verify { id } => verify(store, id.as_deref())?,
        AddonsCommand::Install { path } => {
            for result in store.install_addons(&path) {
                let m = result.with_context(|| format!("installing {}", path.display()))?;
                println!("installed {} {}", m.id, m.version);
                for line in swofty_core::tracks::left_out_lines(store, &m) {
                    println!("warning: {line}");
                }
                for f in installed_check(store, &m) {
                    let tag = if f.level == swofty_core::stdlib::deps::Level::Hint {
                        "hint"
                    } else {
                        "warning"
                    };
                    println!("{tag}: {}", f.block());
                }
            }
        }
        AddonsCommand::Enable { id } if let Some(u) = unreadable(store, &id) => {
            bail!("{}", u.line())
        }
        AddonsCommand::Disable { id, .. } if unreadable(store, &id).is_some() => {
            store.update_profile(profile, |p| p.set_enabled(&id, false))?;
            println!("disabled {id} in profile {profile}");
        }
        AddonsCommand::Remove { id, .. } if unreadable(store, &id).is_some() => {
            store.remove_addon(&id)?;
            println!("removed {id}");
        }
        AddonsCommand::Enable { id }
        | AddonsCommand::Disable { id, .. }
        | AddonsCommand::Remove { id, .. }
            if !store.addons()?.contains_key(&id) =>
        {
            bail!("{id} is not installed")
        }
        AddonsCommand::Enable { id } => toggle(store, profile, &id, true)?,
        AddonsCommand::Disable { id, yes } => {
            let p = store.profile(profile)?;
            if let Some(w) = store.registry()?.turn_off_warning(&p, &id) {
                confirm_saves(&w, "turn off", yes)?;
            }
            if p.enabled().any(|on| on == id) {
                confirm_careers(store, &id, yes)?;
            }
            toggle(store, profile, &id, false)?
        }
        AddonsCommand::Remove { id, yes } => {
            if let Some(w) = store.addons()?.get(&id).and_then(SavesWarning::of) {
                confirm_saves(&w, "remove", yes)?;
            }
            store.remove_addon(&id)?;
            println!("removed {id}");
        }
    }
    Ok(())
}

fn verify(store: &Store, only: Option<&str>) -> Result<()> {
    use swofty_core::addons::files;
    let installed = store.addons()?;
    if let Some(id) = only
        && !installed.contains_key(id)
    {
        bail!("{id} is not installed");
    }
    let mut damaged = 0;
    for (id, m) in installed
        .iter()
        .filter(|(id, _)| only.is_none_or(|o| o == id.as_str()))
    {
        let dir = store.addons_dir().join(id);
        if store.is_builtin(id) || !dir.is_dir() {
            continue;
        }
        let damage = files::verify(&dir, m);
        let stops = damage.iter().any(|d| d.stops);
        if !stops {
            let unlisted = files::read_index(&dir).is_none();
            let note = if unlisted {
                " (installed without a file list: the files its manifest names, archives and plugins checked)"
            } else {
                ""
            };
            println!("{id:32} ok{note}");
        } else {
            damaged += 1;
        }
        for d in damage {
            let warning = if d.stops { "" } else { "warning: " };
            println!("{id:32} {warning}{}", d.line());
        }
    }
    if damaged > 0 {
        bail!("{damaged} damaged addon(s): install them again");
    }
    Ok(())
}

fn installed_check(
    store: &Store,
    m: &swofty_core::manifest::AddonManifest,
) -> Vec<swofty_core::stdlib::deps::Finding> {
    use swofty_core::stdlib::deps;
    let Ok(installed) = store.addons() else {
        return Vec::new();
    };
    let all = deps::read_all(store, &installed);
    let Some(new) = all.get(&m.id) else {
        return Vec::new();
    };
    let others: Vec<deps::Std> = all.values().cloned().collect();
    deps::at_install(new, &installed, &others).findings
}

fn confirm_careers(store: &Store, id: &str, yes: bool) -> Result<()> {
    let careers = registry::careers_needing(store, id);
    if careers.is_empty() || yes {
        return Ok(());
    }
    let name = store
        .addons()?
        .get(id)
        .map_or_else(|| id.to_owned(), |m| m.name.clone());
    let n = careers.len();
    let mut text = format!("{name} is in {n} career{}\n", if n == 1 { "" } else { "s" });
    for c in &careers {
        let label = if c.label.is_empty() {
            c.career.as_str()
        } else {
            c.label.as_str()
        };
        let why = c
            .addons
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.why.as_deref())
            .map(|w| format!(" ({w})"))
            .unwrap_or_default();
        text.push_str(&format!("  {label}{why}\n"));
    }
    text.push_str(&format!(
        "  {} won't load until {name} is back on. Your other careers aren't affected.",
        if n == 1 { "That career" } else { "Those careers" }
    ));
    if !prompt::interactive() {
        bail!("{text}\nstdin is not a terminal: pass --yes to turn off {id} anyway");
    }
    println!("{text}");
    if !prompt::yes(&format!("Turn off {id} anyway?"))? {
        bail!("{id} is unchanged");
    }
    Ok(())
}

fn unreadable(store: &Store, id: &str) -> Option<swofty_core::store::Unreadable> {
    store.unreadable_addons().into_iter().find(|u| u.id == id)
}

fn confirm_saves(w: &SavesWarning, verb: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    let (id, warning) = (&w.id, w.text());
    if !prompt::interactive() {
        bail!("{warning}\nstdin is not a terminal: pass --yes to {verb} {id} anyway");
    }
    println!("{warning}");
    let mut ask = verb.to_owned();
    ask[..1].make_ascii_uppercase();
    if !prompt::yes(&format!("{ask} {id} anyway?"))? {
        bail!("{id} is unchanged");
    }
    Ok(())
}

fn toggle(store: &Store, profile: &str, id: &str, on: bool) -> Result<()> {
    let mut p = store.profile(profile)?;
    store.registry()?.set_enabled(&mut p, id, on)?;
    store.save_profile(&p)?;
    println!(
        "{} {id} in profile {profile}",
        if on { "enabled" } else { "disabled" }
    );
    Ok(())
}
