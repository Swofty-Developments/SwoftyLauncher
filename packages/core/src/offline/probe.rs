use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::{Artifacts, EXTENSION_VERSION, command, loader_hint, start_error};

/// Result of probing the game build for support.
#[derive(Debug, Clone)]
pub struct Probe {
    pub supported: bool,
    pub report: String,
}

pub fn probe_path(data: &Path) -> PathBuf {
    data.join("state/offline/probe.json")
}

fn probe_key(exe: &Path) -> Option<String> {
    let meta = std::fs::metadata(exe).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!(
        "{EXTENSION_VERSION}|{}|{}|{modified}",
        exe.display(),
        meta.len()
    ))
}

/// [`probe`], skipped when the same extension release already found this
/// exact game executable supported.
pub fn probe_cached(data: &Path, game: &Path, a: &Artifacts) -> Result<Probe> {
    let key = probe_key(&game.join(crate::GAME_EXE));
    let path = probe_path(data);
    if let Some(key) = &key
        && let Ok((seen, report)) = crate::read_json::<(String, String)>(&path)
        && seen == *key
    {
        return Ok(Probe {
            supported: true,
            report,
        });
    }
    let found = probe(game, a)?;
    if found.supported
        && let Some(key) = key
    {
        crate::write_json(&path, &(key, &found.report)).ok();
    }
    Ok(found)
}

/// Ask the extension whether this game build is supported (fail closed).
pub fn probe(game: &Path, a: &Artifacts) -> Result<Probe> {
    let out = command(&a.probe)?
        .arg(game.join(crate::GAME_EXE))
        .output()
        .map_err(|e| start_error(&a.probe, e))?;
    let report = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    match out.status.code() {
        Some(0) => Ok(Probe {
            supported: true,
            report,
        }),
        Some(1) => Ok(Probe {
            supported: false,
            report,
        }),
        code => Err(Error::Other(format!(
            "probe failed ({code:?}): {}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            loader_hint(&a.probe, out.status)
        ))),
    }
}
