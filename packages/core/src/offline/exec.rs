use std::path::{Path, PathBuf};
use std::process::Command;

use crate::platform::app_control::Block;
use crate::{Error, Result, io_err};

use super::EXTENSION_VERSION;

/// How a runner is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exec {
    /// Executed as is.
    Direct,
    /// The binary names a program loader (ELF interpreter) this system does
    /// not have, so it runs through the system's copy of that loader.
    Loader { wanted: PathBuf, system: PathBuf },
}

/// Where Linux distributions keep the program loader.
#[cfg(target_os = "linux")]
const LOADER_DIRS: &[&str] = &[
    "/lib64",
    "/usr/lib64",
    "/lib/x86_64-linux-gnu",
    "/usr/lib/x86_64-linux-gnu",
    "/lib",
    "/usr/lib",
];

/// Decide how to start a runner, refusing one that is missing or cannot start
/// here. A Linux binary whose ELF interpreter does not exist fails to exec with
/// "No such file or directory" even though the file is present; it is run
/// through the system loader instead when there is one.
pub fn exec_mode(program: &Path) -> Result<Exec> {
    #[cfg(target_os = "linux")]
    let dirs: Vec<&Path> = LOADER_DIRS.iter().map(Path::new).collect();
    #[cfg(not(target_os = "linux"))]
    let dirs: Vec<&Path> = Vec::new();
    exec_mode_in(program, &dirs)
}

pub(super) fn exec_mode_in(program: &Path, loader_dirs: &[&Path]) -> Result<Exec> {
    if !program.is_file() {
        return Err(Error::Other(format!(
            "{} is missing; reinstall the extension with `swofty offline fetch`",
            program.display()
        )));
    }
    if !cfg!(target_os = "linux") {
        return Ok(Exec::Direct);
    }
    let Some(wanted) = elf_interpreter(program)? else {
        return Ok(Exec::Direct);
    };
    if wanted.exists() {
        return Ok(Exec::Direct);
    }
    let system = wanted
        .file_name()
        .and_then(|name| loader_dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()));
    match system {
        Some(system) => Ok(Exec::Loader { wanted, system }),
        None => Err(Error::Other(format!(
            "{} cannot run on this system: it was built to start through {}, which does not \
             exist here, and no system copy of that loader was found. This is a packaging \
             problem in the offline play {EXTENSION_VERSION} Linux build, not in your install.",
            program.display(),
            wanted.display()
        ))),
    }
}

/// The PT_INTERP path of a 64-bit little-endian ELF file, or None for a static
/// binary or anything that is not one.
pub(super) fn elf_interpreter(path: &Path) -> Result<Option<PathBuf>> {
    const PT_INTERP: usize = 3;
    let data = std::fs::read(path).map_err(io_err(path))?;
    if data.get(..6) != Some(b"\x7fELF\x02\x01".as_slice()) {
        return Ok(None);
    }
    let le = |at: usize, len: usize| -> Option<usize> {
        let bytes = data.get(at..at.checked_add(len)?)?;
        let value = bytes.iter().rev().fold(0u64, |v, b| (v << 8) | u64::from(*b));
        usize::try_from(value).ok()
    };
    let (Some(phoff), Some(phentsize), Some(phnum)) = (le(0x20, 8), le(0x36, 2), le(0x38, 2)) else {
        return Ok(None);
    };
    for i in 0..phnum {
        let Some(ph) = i.checked_mul(phentsize).and_then(|o| o.checked_add(phoff)) else {
            return Ok(None);
        };
        if le(ph, 4) != Some(PT_INTERP) {
            continue;
        }
        let interp = (|| {
            let offset = le(ph.checked_add(0x08)?, 8)?;
            let size = le(ph.checked_add(0x20)?, 8)?;
            data.get(offset..offset.checked_add(size)?)
        })();
        return Ok(interp.map(|raw| PathBuf::from(String::from_utf8_lossy(raw).trim_end_matches('\0'))));
    }
    Ok(None)
}

pub(super) fn command(program: &Path) -> Result<Command> {
    Ok(command_for(program, exec_mode(program)?))
}

pub(super) fn command_for(program: &Path, exec: Exec) -> Command {
    match exec {
        Exec::Direct => Command::new(program),
        Exec::Loader { system, .. } => {
            let mut cmd = Command::new(system);
            cmd.arg(program);
            cmd
        }
    }
}

/// A tool that would not start; one Windows refused says what blocked it.
pub(super) fn start_error(program: &Path, e: std::io::Error) -> Error {
    match Block::of(program, &e) {
        Some(b) => Error::Blocked(b),
        None => Error::Other(format!("could not start {}: {e}", program.display())),
    }
}

/// Explain an exit that came from the system loader rather than the tool: a C
/// library older than the build needs, or a missing shared library.
pub(super) fn loader_hint(program: &Path, status: std::process::ExitStatus) -> &'static str {
    let via_loader = matches!(exec_mode(program), Ok(Exec::Loader { .. }));
    if via_loader && status.code() == Some(127) {
        " (the system loader could not start it: this system's libraries are older than the \
         offline play Linux build needs)"
    } else {
        ""
    }
}
