mod addon_logs;
mod check;
mod data_paths;
mod link;
pub mod lock;
mod native_test;
mod recording;
mod scaffold;
mod toolchain;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use swofty_core::store::Store;

use self::addon_logs::logs;
use self::link::link;
use self::lock::LOCK_FILE;
use self::native_test::test;
use self::recording::{Recorded, manifest_json, register_dlls, write_lock};
use self::scaffold::{new_addon, pack};
use self::toolchain::{python, sdk_root};
use crate::commands::{Execute, Invocation};

impl Execute for DevCommand {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        command(&invocation.store, &invocation.profile, self)
    }
}

pub const SDK_ENV: &str = "SWOFTY_SDK_DIR";

pub const RECORDER_ENV: &str = "SWOFTY_RECORDER_DIR";

pub const WINE_ENV: &str = "SWOFTY_WINE";

pub const DEVELOPER_ENV: &str = "SWOFTY_DEVELOPER";

#[derive(Subcommand, Debug, PartialEq)]
pub enum DevCommand {
    #[command(about = "Turn Developer mode on: the swofty dev tools and the launcher's Developer section")]
    On,
    #[command(about = "Turn Developer mode off")]
    Off,
    #[command(about = "Make a new addon from the template")]
    New {
        id: String,
        #[arg(long, help = "Folder to create (default: ./<id>)")]
        dir: Option<PathBuf>,
        #[arg(
            long,
            help = "A standard library 1.6 addon: a SWOFTY_REGISTER, the 1.6 headers and a fake-core unit test"
        )]
        registry: bool,
    },
    #[command(about = "Install an addon by reference: every build copies from the folder")]
    Link { folder: Option<PathBuf> },
    #[command(
        about = "Build the profile, start the game and stream its log lines (swofty run --apply --direct)"
    )]
    Run {
        #[arg(long, help = crate::commands::cli::GAME_HELP)]
        game: Option<PathBuf>,
    },
    #[command(about = "Show the addon's lines from loader.log; --follow keeps reading")]
    Logs {
        #[arg(help = "The addon's folder or id (default: the current folder)")]
        addon: Option<String>,
        #[arg(long, short, help = "Keep reading as the game writes")]
        follow: bool,
        #[arg(long, help = "A session folder to read instead of the game's loader.log")]
        session: Option<PathBuf>,
    },
    #[command(
        about = "Check the manifest, registry.json, the data files and references without starting the game"
    )]
    Check { folder: Option<PathBuf> },
    #[command(
        about = "Build and run the addon's unit tests (test/*.cpp) against the fake core and the core's registry engine"
    )]
    Test {
        folder: Option<PathBuf>,
        #[arg(long, help = "Only record the calls; leave the core's registry engine out")]
        recording_only: bool,
    },
    #[command(about = "Write registry.lock.json from the addon's SWOFTY_REGISTER and pack the addon")]
    Pack {
        folder: Option<PathBuf>,
        #[arg(long, help = "Where the package goes (default: <folder>/dist)")]
        out: Option<PathBuf>,
        #[arg(long, help = "Write a .zip instead of a .swoftyaddon")]
        zip: bool,
    },
}

pub fn enabled(store: &Store) -> bool {
    store.config().developer || std::env::var(DEVELOPER_ENV).is_ok_and(|v| v == "1")
}

pub fn command(store: &Store, profile: &str, cmd: DevCommand) -> Result<ExitCode> {
    match cmd {
        DevCommand::On | DevCommand::Off => {
            let mut config = store.config();
            config.developer = cmd == DevCommand::On;
            store.save_config(&config)?;
            println!(
                "Developer mode is {}",
                if config.developer { "on" } else { "off" }
            );
            return Ok(ExitCode::SUCCESS);
        }
        _ if !enabled(store) => {
            bail!("Developer mode is off; turn it on in the launcher's Settings or with swofty dev on")
        }
        _ => {}
    }
    let here = || std::env::current_dir().context("no current folder");
    let folder = |f: Option<PathBuf>| -> Result<PathBuf> {
        let f = match f {
            Some(f) => f,
            None => here()?,
        };
        std::path::absolute(&f).with_context(|| format!("{}", f.display()))
    };
    match cmd {
        DevCommand::On | DevCommand::Off => unreachable!(),
        DevCommand::New { id, dir, registry } => {
            let dest = match dir {
                Some(d) => d,
                None => here()?.join(&id),
            };
            new_addon(&sdk_root()?, &id, &dest, registry)?;
            println!("made {} in {}", id, dest.display());
            Ok(ExitCode::SUCCESS)
        }
        DevCommand::Link { folder: f } => {
            let id = link(store, profile, &folder(f)?)?;
            println!("linked {id}: every build copies it from its folder");
            Ok(ExitCode::SUCCESS)
        }
        DevCommand::Run { game } => {
            let args = run_args(game.as_deref());
            let parsed = <RunWrap as clap::Parser>::try_parse_from(args)?;
            crate::commands::run::run(store, profile, parsed.args)
        }
        DevCommand::Logs {
            addon,
            follow,
            session,
        } => logs(store, addon, follow, session),
        DevCommand::Check { folder: f } => {
            let report = check::check(store, &sdk_root().ok(), &folder(f)?)?;
            for line in &report.lines {
                println!("{line}");
            }
            println!(
                "{} error{}, {} warning{}",
                report.errors,
                if report.errors == 1 { "" } else { "s" },
                report.warnings,
                if report.warnings == 1 { "" } else { "s" }
            );
            Ok(if report.errors > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        DevCommand::Test {
            folder: f,
            recording_only,
        } => test(&sdk_root()?, &folder(f)?, store, !recording_only),
        DevCommand::Pack { folder: f, out, zip } => {
            let folder = folder(f)?;
            let out = out.unwrap_or_else(|| folder.join("dist"));
            let sdk = sdk_root()?;
            let mut with = Vec::new();
            match write_lock(store, &sdk, &folder)? {
                Some(Recorded { lock, files, missing }) => {
                    let n = lock.provides.values().map(Vec::len).sum::<usize>();
                    println!(
                        "wrote {LOCK_FILE}: {n} record{}, {} provider{}, {} need{}",
                        plural(n),
                        lock.providers.len(),
                        plural(lock.providers.len()),
                        lock.needs.len(),
                        plural(lock.needs.len())
                    );
                    for m in &missing {
                        println!("warning: the addon's code names {m}, which isn't in the addon");
                    }
                    with = files;
                }
                None => println!("no SWOFTY_REGISTER in the addon's DLLs: no {LOCK_FILE}"),
            }
            let pkg = pack(&sdk, &folder, &out, zip, &with)?;
            println!("{}", pkg.trim_end());
            Ok(ExitCode::SUCCESS)
        }
    }
}

pub fn data_root(store: &Store) -> PathBuf {
    store
        .config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[derive(clap::Parser)]
struct RunWrap {
    #[command(flatten)]
    args: crate::commands::run::RunArgs,
}

pub fn run_args(game: Option<&Path>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["run".into(), "--apply".into(), "--direct".into()];
    if let Some(g) = game {
        args.push(swofty_core::platform::process::flag("game", g));
    }
    args
}

#[cfg(test)]
mod tests;
