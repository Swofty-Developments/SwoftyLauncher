use std::path::Path;

use crate::Error;
use crate::store::Store;

/// A build or install that stopped on a missing, cut-short or damaged file, as the
/// addon or game file it is and what to do about it. Any other error is
/// returned as it is.
pub fn named(store: &Store, game: &Path, e: Error) -> Error {
    if let Error::Formats(swofty_formats::Error::Frame {
        path: Some(path),
        resource,
        fragment,
        reason,
        ..
    }) = &e
    {
        let detail = match resource {
            Some(r) => format!("{r}: fragment {fragment} does not decode: {reason}"),
            None => format!("fragment {fragment} does not decode: {reason}"),
        };
        return damaged_archive(store, game, path, &detail).map_or(e, Error::Other);
    }
    let (Error::Io { path, source } | Error::Formats(swofty_formats::Error::IoAt { path, source })) = &e
    else {
        return e;
    };
    let gone = source.kind() == std::io::ErrorKind::NotFound;
    if !gone && source.kind() != std::io::ErrorKind::UnexpectedEof {
        return e;
    }
    if let Some((id, name, file)) = addon_file(store, path) {
        let what = match gone {
            true => format!("{name} ({id}) is missing its file {file}"),
            false => format!("{name} ({id}) has an incomplete or damaged {file}"),
        };
        return Error::Other(format!(
            "{what}. Turn {name} off in this profile, or remove it on the Addons page and install or import it again."
        ));
    }
    if let Ok(rest) = path.strip_prefix(game) {
        let file = slashed(rest);
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

/// `rest` with forward slashes, as the launcher names a file inside a folder.
fn slashed(rest: &Path) -> String {
    rest.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The addon whose folder holds `path`: its id, its name and the file inside it.
fn addon_file(store: &Store, path: &Path) -> Option<(String, String, String)> {
    let rest = path.strip_prefix(store.addons_dir()).ok()?;
    let id = rest
        .components()
        .next()?
        .as_os_str()
        .to_string_lossy()
        .into_owned();
    let file = rest
        .strip_prefix(&id)
        .ok()
        .filter(|f| !f.as_os_str().is_empty())?;
    let name = store
        .addons()
        .ok()
        .and_then(|a| a.get(&id).map(|m| m.name.clone()))
        .unwrap_or_else(|| id.clone());
    Some((id, name, slashed(file)))
}

/// What a build that read the damaged archive `path` tells the player: the addon whose file it is, the game
/// file, or the game file whose original the launcher backed up there, which also sends the player through
/// the setup again; `detail` says what does not decode. None for an archive anywhere else.
fn damaged_archive(store: &Store, game: &Path, path: &Path, detail: &str) -> Option<String> {
    if let Some((id, name, file)) = addon_file(store, path) {
        return Some(format!(
            "{name} ({id}) has a damaged {file} ({detail}). Turn {name} off in this profile, or remove it on the \
             Addons page and install or import it again."
        ));
    }
    if let Ok(rest) = path.strip_prefix(game) {
        return Some(format!(
            "{} in the game folder is damaged ({detail}); verify the game files in Steam (open {}) or repair the \
             game in the EA app, then try again",
            slashed(rest),
            crate::game_folder::stock::STEAM_VERIFY_URL
        ));
    }
    path.strip_prefix(store.root.join("baseline/objects")).ok()?;
    let sha = path.file_name()?.to_string_lossy().into_owned();
    let mut rels = backed_up_as(&store.root.join("baseline"), &sha);
    if rels.is_empty() {
        rels.push("game file".to_owned());
    }
    crate::game_folder::setup::redo(&store.root).ok();
    Some(crate::game_folder::setup::backups_damaged(&rels, detail))
}

/// The game files whose original any record of originals under `baselines` backs up as `sha`.
fn backed_up_as(baselines: &Path, sha: &str) -> Vec<String> {
    let mut rels = std::collections::BTreeSet::new();
    for dir in std::fs::read_dir(baselines).into_iter().flatten().flatten() {
        let Ok(bytes) = std::fs::read(dir.path().join("baseline.json")) else {
            continue;
        };
        let Ok(b) = serde_json::from_slice::<crate::game_folder::baseline::Baseline>(&bytes) else {
            continue;
        };
        rels.extend(
            b.files
                .into_iter()
                .filter(|(_, r)| r.sha256 == sha)
                .map(|(rel, _)| rel),
        );
    }
    rels.into_iter().collect()
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

/// The refusal for an addon's change, or an entry of the launcher's own menus, the game's own copy of `rel`
/// already holds. `play` is for a check before Play, which puts a backed-up original back itself.
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
