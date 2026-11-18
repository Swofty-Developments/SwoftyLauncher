use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use swofty_core::manifest::MANIFEST_FILE;
use swofty_core::store::Store;

use crate::commands::dev::recording::{manifest_json, native_dlls};

pub fn needles(id: &str, manifest: &Value) -> Vec<String> {
    let mut out = vec![id.to_owned()];
    for dll in native_dlls(manifest) {
        if let Some(name) = Path::new(&dll).file_name() {
            out.push(name.to_string_lossy().into_owned());
        }
    }
    out
}

pub fn matches(line: &str, needles: &[String]) -> bool {
    needles.iter().any(|n| {
        line.match_indices(n.as_str()).any(|(at, _)| {
            let before = line[..at].chars().next_back();
            let after = line[at + n.len()..].chars().next();
            let edge = |c: Option<char>| {
                c.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'))
            };
            edge(before) && (edge(after) || after == Some('/'))
        })
    })
}

pub(super) fn logs(
    store: &Store,
    addon: Option<String>,
    follow: bool,
    session: Option<PathBuf>,
) -> Result<ExitCode> {
    let (id, manifest) = match addon {
        Some(a) if !Path::new(&a).join(MANIFEST_FILE).is_file() => {
            let dir = store.addons_dir().join(&a);
            let m = manifest_json(&dir).unwrap_or(Value::Null);
            (a, m)
        }
        a => {
            let dir = a.map(PathBuf::from).unwrap_or(std::env::current_dir()?);
            let m = manifest_json(&dir)?;
            let id = m["id"].as_str().context("swofty.json has no id")?.to_owned();
            (id, m)
        }
    };
    let needles = needles(&id, &manifest);
    let file = match session {
        Some(dir) => dir.join("loader.log"),
        None => {
            let live = crate::commands::build::game(store, None)
                .ok()
                .map(|g| g.dir.join("swofty/loader.log"))
                .filter(|p| p.is_file());
            match live {
                Some(p) => p,
                None => store
                    .latest_session()
                    .context("no loader.log yet: start the game with swofty dev run")?
                    .join("loader.log"),
            }
        }
    };
    let mut at = 0u64;
    let mut pending = String::new();
    loop {
        if let Ok(mut f) = std::fs::File::open(&file) {
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            if len < at {
                at = 0;
            }
            f.seek(SeekFrom::Start(at))?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            at += buf.len() as u64;
            pending.push_str(&String::from_utf8_lossy(&buf));
            while let Some(nl) = pending.find('\n') {
                let line: String = pending.drain(..=nl).collect();
                let line = line.trim_end();
                if matches(line, &needles) {
                    println!("{line}");
                }
            }
        } else if !follow {
            bail!("{} isn't there", file.display());
        }
        if !follow {
            return Ok(ExitCode::SUCCESS);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
