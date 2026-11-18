use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use swofty_core::launcher::Progress;
use swofty_core::platform::app_control::Block;
use swofty_core::session::COMPAT_LAYER;
use swofty_core::updates::release::{self as update, Install};

use crate::app::Reporter;
#[cfg(windows)]
use crate::win32;
use crate::{FOLDER_NAME, data, existing, shortcut};

pub(super) fn pick_folder(title: &str, current: &str) -> Option<PathBuf> {
    let start = PathBuf::from(current)
        .ancestors()
        .find(|p| p.is_dir())
        .map(Path::to_path_buf);
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder()
}

pub(super) fn install_folder(picked: PathBuf) -> PathBuf {
    let empty = std::fs::read_dir(&picked).is_ok_and(|mut d| d.next().is_none());
    let installed = picked.join("bin").join(update::exe("swofty-launcher")).is_file();
    let named = picked
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(FOLDER_NAME));
    if empty || installed || named {
        picked
    } else {
        picked.join(FOLDER_NAME)
    }
}

pub(super) fn install(
    root: &Path,
    desktop: bool,
    moving: Option<&existing::Move>,
    progress: &Reporter,
) -> Result<PathBuf, String> {
    let release = update::fetch_latest(Duration::from_secs(30))
        .map_err(|e| format!("Could not find the latest release: {e}"))?;
    if let Some(moving) = moving {
        progress.report(0.0, "Moving your install");
        moving.run(desktop)?;
    }
    std::fs::create_dir_all(root).map_err(|e| format!("Could not create {}: {e}", root.display()))?;
    let install = Install {
        root: root.to_path_buf(),
    };
    install.update(&release, progress).map_err(|e| e.to_string())?;
    let exe = install.launcher();
    progress.report(1.0, "Adding shortcuts");
    shortcut::create(root, &exe, desktop);
    Ok(exe)
}

/// Opens the installed launcher as the signed-in user. An elevated installer
/// hands it to the desktop's shell, so the launcher doesn't inherit the
/// administrator rights; started directly, it gets none of the installer's
/// compatibility layers.
pub(super) fn launch(exe: &Path, data_env: Option<&Path>) -> Result<(), String> {
    #[cfg(windows)]
    if let Some(root) = exe.parent().and_then(Path::parent)
        && swofty_core::platform::run_as_admin::elevated()
        && win32::shell_open(exe, root).is_ok()
    {
        return Ok(());
    }
    command(exe, data_env)
        .spawn()
        .map(drop)
        .map_err(|e| match Block::of(exe, &e) {
            Some(b) => format!("Installed, but the launcher would not start. {}", b.text()),
            None => format!(
                "Installed, but the launcher would not start ({}): {e}",
                exe.display()
            ),
        })
}

pub(super) fn command(exe: &Path, data_env: Option<&Path>) -> Command {
    let mut command = Command::new(exe);
    if let Some(root) = exe.parent().and_then(Path::parent) {
        command.current_dir(root);
    }
    if let Some(data) = data_env {
        command.env(data::ENV, data);
    }
    command
        .env_remove(COMPAT_LAYER)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
