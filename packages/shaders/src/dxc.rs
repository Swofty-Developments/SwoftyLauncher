//! Finding the dxc that compiles effect passes.

use std::ffi::OsString;
use std::path::PathBuf;

fn exe_name() -> &'static str {
    if cfg!(windows) { "dxc.exe" } else { "dxc" }
}

/// The dxc to compile with: the one `SWOFTY_DXC` names, else `bin/dxc` (`bin\dxc.exe` on Windows) in the first of
/// `folders` that has it, else the first on `PATH`. A release's `dxc/` folder and the one `tools/dxc/fetch.sh`
/// unpacks at the repository root both have that layout.
pub fn find_dxc(folders: &[PathBuf]) -> Option<PathBuf> {
    find_in(std::env::var_os("SWOFTY_DXC"), folders, std::env::var_os("PATH"))
}

fn find_in(named: Option<OsString>, folders: &[PathBuf], path: Option<OsString>) -> Option<PathBuf> {
    if let Some(p) = named.filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    folders
        .iter()
        .map(|f| f.join("bin").join(exe_name()))
        .chain(std::env::split_paths(&path.unwrap_or_default()).map(|d| d.join(exe_name())))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-shaders-dxc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put_dxc(folder: &std::path::Path) -> PathBuf {
        std::fs::create_dir_all(folder).unwrap();
        let exe = folder.join(exe_name());
        std::fs::write(&exe, b"").unwrap();
        exe
    }

    #[test]
    fn the_named_dxc_comes_first_then_the_folders_then_path() {
        let root = scratch("order");
        let in_folder = put_dxc(&root.join("repo/dxc/bin"));
        let on_path = put_dxc(&root.join("path"));
        let path = std::env::join_paths([root.join("path")]).ok();
        let folders = [root.join("missing"), root.join("repo/dxc")];
        assert_eq!(
            find_in(Some("/named/dxc".into()), &folders, path.clone()),
            Some(PathBuf::from("/named/dxc"))
        );
        assert_eq!(find_in(Some("".into()), &folders, path.clone()), Some(in_folder));
        assert_eq!(find_in(None, &folders[..1], path), Some(on_path));
        assert_eq!(find_in(None, &folders[..1], None), None);
        std::fs::remove_dir_all(&root).ok();
    }
}
