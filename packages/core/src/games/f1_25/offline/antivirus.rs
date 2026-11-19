use std::path::{Path, PathBuf};

use crate::Error;
use crate::platform::app_control::Block;

use super::{EAC_EXE, REQUIRED, extension_dir};

pub(super) const ANTIVIRUS: &str = "an antivirus removed or blocked Swofty's offline play file. Add an exclusion for the \
     Swofty data folder (%LOCALAPPDATA%\\Swofty, or SWOFTY_DATA_DIR) and the F1 25 folder, restore the file \
     from the antivirus's quarantine, then press Play";

pub(super) fn av_error(e: Error) -> Error {
    match e {
        Error::Io { source, .. } if virus_blocked(&source) => {
            Error::Other(format!("offline play: {ANTIVIRUS}"))
        }
        e => e,
    }
}

/// The EA app's proxy failed to start offline play's file: an antivirus
/// blocked it (logged since `since`), or removed it after the session staged
/// it at `staged`, so the proxy found no file.
pub fn stub_blocked(game: &Path, proxy_logs: &[String], since: i64, staged: Option<i64>) -> Option<Block> {
    let logged = proxy_logs.iter().any(|log| {
        crate::diagnostics::ea_launch::proxy_virus_block(log, EAC_EXE, since)
            || staged.is_some_and(|at| crate::diagnostics::ea_launch::proxy_file_missing(log, EAC_EXE, at))
    });
    logged.then(|| stub_removed(game))
}

/// Offline play's file in the anti-cheat launcher's place, as an antivirus
/// blocked or removed it.
pub fn stub_removed(game: &Path) -> Block {
    Block {
        program: game.join(EAC_EXE),
        code: crate::diagnostics::quarantine::ERROR_VIRUS_INFECTED,
    }
}

/// When the session staged offline play, or last put its file back: the
/// journal's last write.
pub fn staged_at(journal: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(journal).ok()?.modified().ok()
}

pub(super) const STUB: &str = "f1op_stub.exe";

/// The extension's stub when it is the one file missing from the extension
/// folder, with the hook still beside it: an antivirus removed it.
pub fn stub_removed_from_extension(data: &Path) -> Option<PathBuf> {
    let dir = extension_dir(data);
    let stub = dir.join(STUB);
    let rest = REQUIRED
        .iter()
        .filter(|n| **n != STUB)
        .all(|n| dir.join(n).is_file());
    (rest && !stub.is_file()).then_some(stub)
}

pub fn stub_removed_text(data: &Path, stub: &Path) -> String {
    format!(
        "{STUB} is missing from {} while f1op_hook.dll is still there, so an antivirus most likely removed it. \
         Restore {STUB} from the antivirus's quarantine (Windows Security > Virus & threat protection > \
         Protection history) and add an exclusion for the Swofty data folder ({}) and the F1 25 folder, or \
         the antivirus removes it again",
        stub.parent().unwrap_or(stub).display(),
        data.display()
    )
}

pub(super) fn virus_code(code: i32) -> bool {
    crate::diagnostics::quarantine::is_virus_code(code)
}

pub(super) fn virus_blocked(e: &std::io::Error) -> bool {
    e.raw_os_error().filter(|_| cfg!(windows)).is_some_and(virus_code)
}
