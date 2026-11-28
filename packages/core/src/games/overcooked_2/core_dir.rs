//! Where the Swofty Core for Overcooked 2 is: the core lane's `dist/` folder, in the release bundle or a source
//! checkout.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Result};

pub const ENV: &str = "SWOFTY_OC2_CORE_DIR";
/// Beside the launcher's `bin/` folder in the release bundle.
pub const BUNDLED: &str = "cores/overcooked_2";
/// In a source checkout, found walking up from the running exe.
pub const CHECKOUT: &str = "games/overcooked_2/dist";
pub const TARGET_ASSEMBLY: &str = "core/Swofty.Overcooked2.Core.dll";
pub const DOORSTOP_LINUX: &str = "doorstop/linux_x64/libdoorstop.so";
pub const DOORSTOP_WINDOWS: &str = "doorstop/windows_x64/winhttp.dll";

/// The versions in the core's `core.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Versions {
    #[serde(default)]
    pub core: Option<String>,
    #[serde(default)]
    pub std: Option<String>,
    #[serde(default)]
    pub harmony: Option<String>,
    #[serde(default)]
    pub doorstop: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreDir(pub PathBuf);

impl CoreDir {
    pub fn target_assembly(&self) -> PathBuf {
        self.0.join(TARGET_ASSEMBLY)
    }

    pub fn doorstop_linux(&self) -> PathBuf {
        self.0.join(DOORSTOP_LINUX)
    }

    pub fn doorstop_windows(&self) -> PathBuf {
        self.0.join(DOORSTOP_WINDOWS)
    }

    pub fn versions(&self) -> Versions {
        crate::read_json(&self.0.join("core.json")).unwrap_or_default()
    }

    /// A folder holds the core when it has the target assembly.
    pub fn holds_core(dir: &Path) -> bool {
        dir.join(TARGET_ASSEMBLY).is_file()
    }
}

/// The core for the launcher whose exe is `exe`, first match wins: [`ENV`], [`BUNDLED`] beside the exe's `bin/`
/// folder, then [`CHECKOUT`] in a folder above the exe.
pub fn find_for(env: Option<PathBuf>, exe: Option<&Path>) -> Option<CoreDir> {
    if let Some(dir) = env.filter(|d| !d.as_os_str().is_empty()) {
        return Some(CoreDir(dir));
    }
    let exe_dir = exe?.parent()?;
    let bundle = exe_dir
        .ancestors()
        .find(|d| d.file_name().is_some_and(|n| n == "bin"))
        .and_then(Path::parent)
        .unwrap_or(exe_dir);
    let bundled = bundle.join(BUNDLED);
    if CoreDir::holds_core(&bundled) {
        return Some(CoreDir(bundled));
    }
    exe_dir
        .ancestors()
        .map(|d| d.join(CHECKOUT))
        .find(|d| CoreDir::holds_core(d))
        .map(CoreDir)
}

/// The core for this launcher.
pub fn find() -> Option<CoreDir> {
    let exe = std::env::current_exe().ok();
    find_for(std::env::var_os(ENV).map(PathBuf::from), exe.as_deref())
}

/// [`find`], or why Play can't start the game modded.
pub fn require() -> Result<CoreDir> {
    let core = find().ok_or_else(|| {
        Error::Other(format!(
            "the Swofty Core for Overcooked 2 is not installed with this launcher ({BUNDLED} is missing); set {ENV} to its folder"
        ))
    })?;
    if !CoreDir::holds_core(&core.0) {
        return Err(Error::Other(format!(
            "{} has no {TARGET_ASSEMBLY}; it is not a Swofty Core for Overcooked 2 folder",
            core.0.display()
        )));
    }
    Ok(core)
}
