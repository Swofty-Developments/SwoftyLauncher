//! Server modules: signed files the launcher downloads, verifies and loads at runtime.
// lead: merge with infra. Infra's `ensure` and `place` replace this file; only `Module`, `Unavailable` and
// the `ensure` signature below are what `library` and the session client are written against.

pub mod library;

use std::path::{Path, PathBuf};

/// A verified module file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
}

/// Why a module can't be used, as a sentence for the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The launcher's server and the signed-in session, for asking which modules this player may load.
#[derive(Debug, Clone)]
pub struct Api {
    pub server: String,
    pub token: Option<String>,
}

/// The environment variable that names a local build of module `name`: `SWOFTY_MODULE_<NAME>`.
pub fn override_env(name: &str) -> String {
    format!("SWOFTY_MODULE_{}", name.to_ascii_uppercase().replace('-', "_"))
}

/// The platform name of this build's library modules.
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// The verified file of module `name` for `platform` at `abi`.
// lead: merge with infra. This stand-in only honours the local override.
pub fn ensure(
    _data: &Path,
    api: &Api,
    name: &str,
    _platform: &str,
    _abi: u32,
) -> Result<Module, Unavailable> {
    let var = override_env(name);
    if let Some(path) = std::env::var_os(&var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
    {
        if !path.is_file() {
            return Err(Unavailable(format!(
                "{var} names {}, which is not a file",
                path.display()
            )));
        }
        eprintln!(
            "warning: module {name}: using the local file {} ({var}), unsigned",
            path.display()
        );
        return Ok(Module {
            name: name.to_owned(),
            version: "local".into(),
            path,
            sha256: String::new(),
        });
    }
    Err(Unavailable(match api.token {
        None => "Sign in to use this.".into(),
        Some(_) => "This part of the launcher isn't available right now.".into(),
    }))
}
