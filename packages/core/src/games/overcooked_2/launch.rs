//! Starting Overcooked 2 with the Swofty Core: the Linux build through Doorstop's `LD_PRELOAD`, the Windows build
//! through Doorstop's `winhttp.dll` and `doorstop_config.ini` placed next to the exe for the session.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::core_dir::CoreDir;
use crate::{Error, Result, io_err};

pub const LINUX_EXE: &str = "Overcooked2.x86_64";
pub const WINDOWS_EXE: &str = "Overcooked2.exe";
pub const DOORSTOP_DLL: &str = "winhttp.dll";
pub const DOORSTOP_INI: &str = "doorstop_config.ini";
pub const STEAM_APP_ID: &str = "728880";
/// The loader the Linux build's ELF names; a system without it (NixOS) runs the game through `steam-run`.
pub const ELF_LOADER: &str = "/lib64/ld-linux-x86-64.so.2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
}

impl Os {
    pub fn here() -> Self {
        if cfg!(windows) { Self::Windows } else { Self::Linux }
    }
}

/// A file the launch puts in the game folder for the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Copy { from: PathBuf, to: PathBuf },
    Write { to: PathBuf, text: String },
}

impl Place {
    pub fn path(&self) -> &Path {
        match self {
            Self::Copy { to, .. } | Self::Write { to, .. } => to,
        }
    }
}

/// How Play starts the game: the program, its arguments and environment, and the files it places first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub place: Vec<Place>,
    /// The game's exe, which the game process runs as.
    pub exe: PathBuf,
}

/// What [`plan`] needs to know about this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub os: Os,
    /// `LD_PRELOAD` as the launcher has it.
    pub ld_preload: Option<String>,
    /// The `steam-run` to wrap the Linux build in, when the system has no [`ELF_LOADER`].
    pub wrapper: Option<PathBuf>,
}

impl Host {
    pub fn here() -> Self {
        let os = Os::here();
        let wrapper = (os == Os::Linux && !Path::new(ELF_LOADER).exists())
            .then(|| which("steam-run"))
            .flatten();
        Self {
            os,
            ld_preload: std::env::var("LD_PRELOAD").ok().filter(|v| !v.is_empty()),
            wrapper,
        }
    }
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Which build of the game is in `game_dir`, for `os`.
pub fn build_in(game_dir: &Path, os: Os) -> Result<PathBuf> {
    let linux = game_dir.join(LINUX_EXE);
    let windows = game_dir.join(WINDOWS_EXE);
    match os {
        Os::Linux if linux.is_file() => Ok(linux),
        Os::Windows if windows.is_file() => Ok(windows),
        Os::Linux if windows.is_file() => Err(Error::Other(
            "this is the Windows build of Overcooked 2; the Swofty Core plays the Linux build on Linux, the \
             Windows build under Proton is not supported yet. Install the Linux build in Steam (Properties > \
             Compatibility, untick forcing a compatibility tool)"
                .into(),
        )),
        _ => Err(Error::Other(format!(
            "{} has no {}",
            game_dir.display(),
            match os {
                Os::Linux => LINUX_EXE,
                Os::Windows => WINDOWS_EXE,
            }
        ))),
    }
}

/// The launch of the copy in `game_dir` with `core`, playing the session at `session`.
pub fn plan(game_dir: &Path, core: &CoreDir, session: &Path, host: &Host) -> Result<Plan> {
    let exe = build_in(game_dir, host.os)?;
    let session_dir = session.parent().unwrap_or(game_dir);
    let args = vec![
        "-logFile".to_owned(),
        session_dir
            .join(super::session::GAME_LOG_FILE)
            .display()
            .to_string(),
    ];
    let mut env = vec![
        ("SteamAppId".to_owned(), STEAM_APP_ID.to_owned()),
        ("SteamGameId".to_owned(), STEAM_APP_ID.to_owned()),
        (super::session::ENV.to_owned(), session.display().to_string()),
    ];
    match host.os {
        Os::Linux => {
            let preload = match &host.ld_preload {
                Some(existing) => format!("{}:{existing}", core.doorstop_linux().display()),
                None => core.doorstop_linux().display().to_string(),
            };
            env.push(("DOORSTOP_ENABLED".into(), "1".into()));
            env.push((
                "DOORSTOP_TARGET_ASSEMBLY".into(),
                core.target_assembly().display().to_string(),
            ));
            let (program, args) = match &host.wrapper {
                Some(wrapper) => {
                    let mut wrapped = vec![
                        "env".to_owned(),
                        format!("LD_PRELOAD={preload}"),
                        exe.display().to_string(),
                    ];
                    wrapped.extend(args);
                    (wrapper.clone(), wrapped)
                }
                None => {
                    env.push(("LD_PRELOAD".into(), preload));
                    (exe.clone(), args)
                }
            };
            Ok(Plan {
                program,
                args,
                cwd: game_dir.to_path_buf(),
                env,
                place: Vec::new(),
                exe,
            })
        }
        Os::Windows => {
            let ini = format!(
                "[General]\r\nenabled = true\r\ntarget_assembly = {}\r\n",
                core.target_assembly().display()
            );
            Ok(Plan {
                program: exe.clone(),
                args,
                cwd: game_dir.to_path_buf(),
                env,
                place: vec![
                    Place::Copy {
                        from: core.doorstop_windows(),
                        to: game_dir.join(DOORSTOP_DLL),
                    },
                    Place::Write {
                        to: game_dir.join(DOORSTOP_INI),
                        text: ini,
                    },
                ],
                exe,
            })
        }
    }
}

impl Plan {
    /// Puts the plan's files in the game folder. A file of the same name already there is refused unless an
    /// earlier session left it, so a player's own Doorstop or mod loader is never overwritten.
    pub fn place(&self) -> Result<Vec<PathBuf>> {
        let mut placed = Vec::new();
        for p in &self.place {
            let to = p.path();
            let ours = match p {
                Place::Copy { from, to } => std::fs::read(from).ok() == std::fs::read(to).ok(),
                Place::Write { to, text } => std::fs::read_to_string(to).ok().as_ref() == Some(text),
            };
            if to.exists() && !ours {
                for done in &placed {
                    std::fs::remove_file(done).ok();
                }
                return Err(Error::Other(format!(
                    "{} is already in the game folder; take it out (another mod loader?) and press Play again",
                    to.display()
                )));
            }
            match p {
                Place::Copy { from, to } => std::fs::copy(from, to).map(|_| ()),
                Place::Write { to, text } => std::fs::write(to, text),
            }
            .map_err(io_err(to))?;
            placed.push(to.to_path_buf());
        }
        Ok(placed)
    }

    /// Spawns the game with its output in `output`.
    pub fn spawn(&self, output: &Path) -> Result<std::process::Child> {
        let out = std::fs::File::create(output).map_err(io_err(output))?;
        let err = out.try_clone().map_err(io_err(output))?;
        Command::new(&self.program)
            .args(&self.args)
            .current_dir(&self.cwd)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| Error::Other(format!("could not start {}: {e}", self.program.display())))
    }
}
