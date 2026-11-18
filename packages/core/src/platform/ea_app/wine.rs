//! The EA app's Wine prefix: its user.reg and the wineserver running it.

use std::path::{Path, PathBuf};

/// The `user.reg` of the EA app's Wine prefix from the `launch.ea_prefix`
/// setting: the prefix itself, or Proton's compatdata folder around it.
pub fn prefix_user_reg(setting: &Path) -> Option<PathBuf> {
    [setting.join("user.reg"), setting.join("pfx/user.reg")]
        .into_iter()
        .find(|p| p.is_file())
}

/// The wineserver running the Wine prefix `pfx`. Wine names the server's
/// folder after the prefix folder's device and inode, and the server works
/// in it.
#[cfg(not(windows))]
pub fn prefix_server(pfx: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(pfx).ok()?;
    let folder = format!("server-{:x}-{:x}", meta.dev(), meta.ino());
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let pid = e.file_name().to_str()?.parse().ok()?;
        let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
        let cwd = std::fs::read_link(e.path().join("cwd")).ok()?;
        (comm.trim_end() == "wineserver" && cwd.file_name()? == folder.as_str()).then_some(pid)
    })
}
