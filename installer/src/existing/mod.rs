mod lnk;
mod moving;
mod roots;

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use swofty_core::platform::process::same_folder;
use swofty_core::updates::release as update;

use self::roots::find;
use crate::FOLDER_NAME;
use crate::theme::MUTED;

pub use moving::{Move, plan};

const RELEASE: [&str; 8] = [
    "bin",
    "loader",
    "dxc",
    "share",
    "addons",
    "LICENSE",
    "README.md",
    "THIRD_PARTY.md",
];

const DATA_ENTRIES: [&str; 7] = [
    "addons",
    "config.json",
    "profiles",
    "saves",
    "state",
    "cache",
    "baseline",
];

pub fn is_install(dir: &Path) -> bool {
    dir.join("bin").join(update::exe("swofty-launcher")).is_file() && dir.join("loader").is_dir()
}

pub fn default_root() -> PathBuf {
    let parent = if cfg!(windows) {
        dirs::data_local_dir().map(|d| d.join("Programs"))
    } else {
        dirs::home_dir()
    };
    parent.unwrap_or_default().join(FOLDER_NAME)
}

pub struct Existing {
    found: Vec<PathBuf>,
    picked: usize,
    keep: bool,
}

impl Existing {
    pub fn find() -> Self {
        Self {
            found: find(),
            picked: 0,
            keep: false,
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.found
    }

    pub fn root(&self) -> Option<&Path> {
        self.found.get(self.picked).map(PathBuf::as_path)
    }

    pub fn list(&mut self, ui: &mut egui::Ui, folder: &mut String) {
        if self.found.len() < 2 {
            return;
        }
        ui.label(RichText::new("Installed in").color(MUTED));
        for (i, root) in self.found.iter().enumerate() {
            if ui
                .radio_value(&mut self.picked, i, root.display().to_string())
                .clicked()
            {
                *folder = root.display().to_string();
            }
        }
        ui.add_space(12.0);
    }

    pub fn status(&mut self, ui: &mut egui::Ui, folder: &str) {
        let Ok(target) = crate::resolve(folder) else {
            return;
        };
        match self.found.iter().position(|r| same_folder(r, &target)) {
            Some(i) => self.picked = i,
            None if is_install(&target) => {
                self.found.push(target.clone());
                self.picked = self.found.len() - 1;
            }
            None => {}
        }
        let Some(from) = self.root().map(Path::to_path_buf) else {
            return;
        };
        ui.add_space(6.0);
        if same_folder(&from, &target) {
            ui.label(RichText::new("Updates your install in place.").color(MUTED));
            return;
        }
        ui.label(
            RichText::new(if self.keep {
                format!("Installs a second copy and keeps the one in {}.", from.display())
            } else {
                format!(
                    "Moves your install from {} to {}.",
                    from.display(),
                    target.display()
                )
            })
            .color(MUTED),
        );
    }

    pub fn differs_from(&self, target: &Path) -> bool {
        self.root().is_some_and(|from| !same_folder(from, target))
    }

    pub fn advanced(&mut self, ui: &mut egui::Ui, folder: &str) {
        let Ok(target) = crate::resolve(folder) else {
            return;
        };
        if self.differs_from(&target) {
            ui.checkbox(&mut self.keep, "Keep a second copy of the launcher");
        }
    }

    pub fn planned(&self, target: &Path) -> Result<Option<Move>, String> {
        match self.root() {
            Some(from) if !self.keep && is_install(from) && !same_folder(from, target) => {
                plan(from, target, &swofty_core::data::paths::data_dir()).map(Some)
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests;
