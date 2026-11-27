use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use swofty_core::store::Store;

use super::wrapper_process::WRAPPER_LOG;
use crate::console::output::tee_to;

pub(super) const LAUNCHER_LOGGED: &str = "SWOFTY_WRAPPER_LOG_REDIRECTED";
const CAP: u64 = 256 << 10;

pub(super) fn launcher_started() -> bool {
    std::env::var_os(LAUNCHER_LOGGED).is_some()
}

pub(super) fn start(store: &Store) {
    if launcher_started() {
        return;
    }
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let line = header(
        &crate::time::stamp(),
        &args,
        swofty_core::platform::process::parent_name().as_deref(),
        std::env::var_os("SteamAppId").is_some(),
        env!("CARGO_PKG_VERSION"),
    );
    if let Some(mut file) = open(&store.root.join(WRAPPER_LOG), CAP)
        && writeln!(file, "{line}").is_ok()
    {
        tee_to(file, CAP);
    }
}

pub(super) fn header(
    stamp: &str,
    args: &[OsString],
    parent: Option<&str>,
    steam_app_id: bool,
    version: &str,
) -> String {
    let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    format!(
        "{stamp} [wrapper] started outside the launcher: args [{}], parent {}, SteamAppId {}, launcher {version}",
        args.join(" "),
        parent.unwrap_or("unknown"),
        if steam_app_id { "set" } else { "not set" },
    )
}

fn open(path: &Path, cap: u64) -> Option<File> {
    std::fs::create_dir_all(path.parent()?).ok()?;
    let full = std::fs::metadata(path).is_ok_and(|m| m.len() > cap);
    std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(path)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_says_how_the_wrapper_was_started() {
        let args: Vec<OsString> = ["run", "--", "C:\\Games\\F1_25.exe"].map(OsString::from).to_vec();
        assert_eq!(
            header("20261128-101500", &args, Some("steam.exe"), true, "1.10.60"),
            "20261128-101500 [wrapper] started outside the launcher: args [run -- C:\\Games\\F1_25.exe], \
             parent steam.exe, SteamAppId set, launcher 1.10.60"
        );
        let bare = header("20261128-101500", &[], None, false, "1.10.60");
        assert!(bare.contains("parent unknown, SteamAppId not set"), "{bare}");
    }

    #[test]
    fn the_log_is_appended_to_until_it_is_over_the_cap() {
        let dir = std::env::temp_dir().join(format!("swofty-run-{}-wlog", std::process::id()));
        let path = dir.join("state/wrapper-run.log");
        open(&path, 100).unwrap().write_all(b"first\n").unwrap();
        open(&path, 100).unwrap().write_all(b"second\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        std::fs::write(&path, "x".repeat(101)).unwrap();
        open(&path, 100).unwrap().write_all(b"fresh\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
