use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::commands::dev::SDK_ENV;

pub fn sdk_root() -> Result<PathBuf> {
    let bin = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.to_path_buf()));
    [
        std::env::var_os(SDK_ENV).map(PathBuf::from),
        bin.as_ref().and_then(|b| Some(b.parent()?.join("sdk"))),
        bin.map(|b| b.join("sdk")),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")),
    ]
    .into_iter()
    .flatten()
    .find(|d| is_sdk(d))
    .context("the SDK folder isn't there (tools/, templates/ and packages/sdk/); set SWOFTY_SDK_DIR to it")
}

fn is_sdk(dir: &Path) -> bool {
    dir.join("tools/pack_addon.py").is_file() && dir.join("packages/sdk/include/swofty_api.h").is_file()
}

pub fn python() -> Result<PathBuf> {
    ["python3", "python"]
        .iter()
        .find_map(|p| which(p))
        .context("python3 isn't installed; dev pack and dev check need it")
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|d| {
        let exts: &[&str] = if cfg!(windows) {
            &[".exe", ".cmd", ""]
        } else {
            &[""]
        };
        exts.iter()
            .map(|e| d.join(format!("{name}{e}")))
            .find(|p| p.is_file())
    })
}

pub fn compiler() -> Result<(PathBuf, Vec<String>)> {
    if let Some(cxx) = std::env::var_os("SWOFTY_CXX") {
        return Ok((PathBuf::from(cxx), Vec::new()));
    }
    if let Some(zig) = which("zig") {
        let mut args = vec!["c++".to_owned()];
        if cfg!(windows) {
            args.extend(["-target".into(), "x86_64-windows-gnu".into()]);
        }
        return Ok((zig, args));
    }
    ["clang++", "g++", "c++"]
        .iter()
        .find_map(|c| which(c))
        .map(|c| (c, Vec::new()))
        .context("no C++ compiler: install zig (or set SWOFTY_CXX)")
}
