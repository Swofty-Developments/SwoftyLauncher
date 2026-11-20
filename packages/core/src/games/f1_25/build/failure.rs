use std::path::Path;

use crate::Error;
use crate::store::Store;

/// A build or install that stopped on a missing or cut-short file, as the
/// addon or game file it is and what to do about it. Any other error is
/// returned as it is.
pub fn named(store: &Store, game: &Path, e: Error) -> Error {
    let (Error::Io { path, source } | Error::Formats(swofty_formats::Error::IoAt { path, source })) = &e
    else {
        return e;
    };
    let gone = source.kind() == std::io::ErrorKind::NotFound;
    if !gone && source.kind() != std::io::ErrorKind::UnexpectedEof {
        return e;
    }
    let rel = |rest: &Path| {
        rest.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    };
    if let Ok(rest) = path.strip_prefix(store.addons_dir())
        && let Some(id) = rest
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
        && let Ok(file) = rest.strip_prefix(&id)
        && !file.as_os_str().is_empty()
    {
        let name = store
            .addons()
            .ok()
            .and_then(|a| a.get(&id).map(|m| m.name.clone()))
            .unwrap_or_else(|| id.clone());
        let file = rel(file);
        let what = match gone {
            true => format!("{name} ({id}) is missing its file {file}"),
            false => format!("{name} ({id}) has an incomplete or damaged {file}"),
        };
        return Error::Other(format!(
            "{what}. Turn {name} off in this profile, or remove it on the Addons page and install or import it again."
        ));
    }
    if let Ok(rest) = path.strip_prefix(game) {
        let file = rel(rest);
        let version = crate::game_folder::exe_version::read(&game.join(crate::GAME_EXE));
        return Error::Other(match gone {
            true => format!(
                "{file} is missing from the game folder; {}, then try again",
                crate::game_folder::stock::missing_fix(1, version.as_deref())
            ),
            false => format!(
                "{file} in the game folder is incomplete or damaged; verify the game files in Steam (open {}) \
                 or repair the game in the EA app, then try again",
                crate::game_folder::stock::STEAM_VERIFY_URL
            ),
        });
    }
    e
}

/// What to do when Windows refused Swofty a file in the game folder `game` or
/// in its data folder `data`; None for any other error.
pub fn refused_fix(data: &Path, game: &Path, e: &Error) -> Option<String> {
    let (Error::Io { path, source } | Error::Formats(swofty_formats::Error::IoAt { path, source })) = e
    else {
        return None;
    };
    let in_use = cfg!(windows) && source.raw_os_error() == Some(32);
    if !in_use && source.kind() != std::io::ErrorKind::PermissionDenied {
        return None;
    }
    if path.starts_with(game) {
        return Some(game_refused_text(data, game, in_use));
    }
    path.starts_with(data).then(|| {
        format!(
            "Windows refused to let Swofty change its own files in {data}, which an antivirus does while it scans \
             them; add {data} and the F1 25 folder {game} as exclusions in your antivirus, then try again",
            data = data.display(),
            game = game.display()
        )
    })
}

/// What to tell the player when Windows refused a write in the game folder; `in_use` is a sharing
/// violation (os error 32): another program has the file open, which no administrator right fixes.
pub(super) fn game_refused_text(data: &Path, game: &Path, in_use: bool) -> String {
    if in_use {
        return format!(
            "another program has a file in {game} open, so Swofty can't change it: close F1 25 if it's still \
             running (look for F1_25.exe in Task Manager), let any Steam update or file check of F1 25 finish, \
             and add {game} and {data} as exclusions in your antivirus, then try again",
            game = game.display(),
            data = data.display()
        );
    }
    format!(
        "Windows refused to let Swofty change files in {}; run Swofty Launcher as administrator, or move F1 25 \
         to a folder outside Program Files",
        game.display()
    )
}

pub(super) fn loader_missing(exe_dir: Option<&Path>) -> String {
    let at = exe_dir.map_or_else(|| "the launcher's folder".to_owned(), |d| d.display().to_string());
    format!(
        "Swofty Launcher's game files (version.dll, swofty_core.dll) are missing from {at}; your antivirus may have removed them. Restore them or run the installer again"
    )
}

/// The refusal for an addon's change the game's own copy of `rel` already holds. `play` is for a check
/// before Play, which puts a backed-up original back itself.
pub fn stale_text(detail: &str, rel: &str, play: bool) -> String {
    let first = match play {
        true => "press Play, which puts its backed-up original back, or ",
        false => "the launcher has no clean copy of it on record; ",
    };
    format!(
        "{detail}. {rel} in the game folder still holds the launcher's changes from an earlier session: {first}repair \
         F1 25 in the EA app (Library > F1 25 > Manage > Repair) or verify the game files in Steam, then press Play \
         again"
    )
}

/// The refusal a check before Play shows for a build that stopped on `e`.
pub fn check_refusal(data: &Path, game: &Path, e: Error) -> Error {
    if let Error::StaleInput { rel, detail } = &e {
        return Error::Other(stale_text(detail, rel, true));
    }
    match refused_fix(data, game, &e) {
        Some(fix) => Error::Other(format!("{e}. {fix}")),
        None => e,
    }
}
