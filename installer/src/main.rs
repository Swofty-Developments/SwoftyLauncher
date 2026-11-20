#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod data;
mod existing;
mod running;
mod shortcut;
mod theme;
#[cfg(windows)]
mod win32;

use std::path::PathBuf;

use eframe::egui;

use self::app::Installer;

const FOLDER_NAME: &str = "SwoftyLauncher";
const FINDING: &str = "Finding the latest release";
const HEIGHT: f32 = 340.0;

fn resolve(folder: &str) -> Result<PathBuf, String> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Err("Choose a folder to install in.".into());
    }
    let path = match folder.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => dirs::home_dir()
            .ok_or("Could not find your home folder.")?
            .join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(folder),
    };
    std::path::absolute(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../apps/launcher/assets/icon-256.png"))
        .expect("the bundled icon is a png");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Swofty Launcher Installer")
            .with_app_id("net.swofty.ModLauncher.Installer")
            .with_inner_size([540.0, HEIGHT])
            .with_resizable(false)
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Swofty Launcher Installer",
        options,
        Box::new(|cc| Ok(Box::new(Installer::new(cc)))),
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn asks_windows_to_run_it_with_the_rights_it_was_started_with() {
        let manifest = win32::manifest().expect("an embedded application manifest");
        assert!(manifest.contains(r#"level="asInvoker""#), "{manifest}");
    }
}
