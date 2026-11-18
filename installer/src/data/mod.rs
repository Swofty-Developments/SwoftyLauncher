mod files;
mod moving;

use std::path::{Path, PathBuf};

use swofty_core::addons::builtin::CORE_ID;
use swofty_core::data::relocate::{
    self as data_move, Location,
    find::{self, Found},
    link::target as link_target,
};
use swofty_core::platform::disk;
use swofty_core::platform::process::{same_folder, same_path};

pub use moving::{DataMove, Ops, plan};

pub const ENV: &str = "SWOFTY_DATA_DIR";
pub const FOLDER: &str = if cfg!(windows) { "Swofty" } else { "swofty" };
const MOVING: &str = "Moving your data folder";
const COPYING: &str = "Copying your data folder";
const CHECKING: &str = "Checking the copied data folder";

#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub default: PathBuf,
    pub env: Option<PathBuf>,
    pub install: Option<PathBuf>,
}

impl Data {
    pub fn find() -> Self {
        Self {
            default: swofty_core::data::paths::platform_data_dir(),
            env: std::env::var_os(ENV).filter(|v| !v.is_empty()).map(PathBuf::from),
            install: None,
        }
    }

    pub fn apart(&self, to: &Path) -> Result<(), String> {
        let Some(root) = &self.install else {
            return Ok(());
        };
        let within = |inner: &Path, outer: &Path| inner.ancestors().any(|a| same_path(a, outer));
        if within(to, root) {
            return Err(format!(
                "Updates cannot install when the data folder is inside the install folder. Choose a data folder outside {}.",
                root.display()
            ));
        }
        if within(root, to) {
            return Err(format!(
                "Updates cannot install when the install folder is inside the data folder. Choose a data folder that does not contain {}.",
                root.display()
            ));
        }
        Ok(())
    }

    pub fn link(&self) -> Option<PathBuf> {
        link_target(&self.default)
    }

    pub fn current(&self) -> PathBuf {
        self.env
            .clone()
            .or_else(|| self.link())
            .unwrap_or_else(|| self.default.clone())
    }

    pub fn movable(&self) -> bool {
        self.env.as_ref().is_none_or(|e| cfg!(windows) && e.is_absolute())
    }

    pub fn describe(&self, to: &Path) -> Result<String, String> {
        if let Some(env) = &self.env
            && !self.movable()
        {
            return Ok(format!(
                "{ENV} sets the data folder to {}. Change the variable to move it.",
                env.display()
            ));
        }
        Ok(match plan(self, to)? {
            Some(m) => m.describe(),
            None => match (&self.env, self.link()) {
                (Some(_), _) => format!("Stays in {}, the folder {ENV} names.", to.display()),
                (None, Some(link)) => format!(
                    "Stays in {}. {} links there.",
                    link.display(),
                    self.default.display()
                ),
                (None, None) => format!("Stays in {}.", self.default.display()),
            },
        })
    }
}

pub fn size(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{} GB", disk::gb(bytes))
    } else {
        format!("{} MB", bytes.div_ceil(1 << 20))
    }
}

pub fn addon_count(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("addons")).map_or(0, |read| {
        read.flatten()
            .filter(|e| e.path().join("swofty.json").is_file())
            .filter(|e| !swofty_core::updates::managed::is_managed(&e.file_name().to_string_lossy()))
            .count()
    })
}

pub fn finder(data: &Data, installs: &[PathBuf]) -> Vec<Found> {
    if data.env.is_some() || addon_count(&data.current()) > 0 {
        return Vec::new();
    }
    let location = Location {
        default: data.default.clone(),
        env: None,
        install: data.install.clone(),
        games: Vec::new(),
    };
    let bundled = [CORE_ID.to_owned(), swofty_core::updates::managed::ID.to_owned()];
    find::candidates(&location, installs, &bundled)
}

pub fn adopt(data: &Data, found: &Path, installs: &[PathBuf]) -> Result<(), String> {
    let location = Location {
        default: data.default.clone(),
        env: None,
        install: data.install.clone(),
        games: Vec::new(),
    };
    let bundled = [CORE_ID.to_owned(), swofty_core::updates::managed::ID.to_owned()];
    data_move::adopt(&location, found, installs, &bundled).map_err(|e| e.to_string())?;
    match data_move::recover(&data.default, &data_move::Ops::system()) {
        Some(Ok(_)) | None => Ok(()),
        Some(Err(e)) => Err(e.to_string()),
    }
}

pub fn on_other_drive(a: &Path, b: &Path) -> bool {
    let volume = |p: &Path| p.ancestors().find(|a| a.exists()).and_then(disk::volume);
    matches!((volume(a), volume(b)), (Some(x), Some(y)) if x != y)
}

pub fn on_install_drive(install: &Path) -> PathBuf {
    let mut root = PathBuf::new();
    for part in install.components() {
        match part {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => root.push(part),
            _ => break,
        }
    }
    if cfg!(windows) && root.parent().is_none() && root.as_os_str().len() > 1 {
        root.join(FOLDER)
    } else {
        install.parent().unwrap_or(&root).join(FOLDER)
    }
}

pub fn drive_name(install: &Path) -> String {
    match install.components().next() {
        Some(std::path::Component::Prefix(p)) if cfg!(windows) => {
            p.as_os_str().to_string_lossy().into_owned()
        }
        _ => install.parent().unwrap_or(install).display().to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    Leave,
    Move,
}

pub fn asks(install: &Path, data: &Data) -> bool {
    drives_differ(data, on_other_drive(install, &data.current()))
}

fn drives_differ(data: &Data, other_drive: bool) -> bool {
    data.movable() && data.env.is_none() && other_drive
}

pub fn target(choice: Choice, install: &Path, data: &Data, typed: &str) -> String {
    target_with(choice, asks(install, data), install, typed)
}

fn target_with(choice: Choice, asked: bool, install: &Path, typed: &str) -> String {
    if asked && choice == Choice::Move {
        on_install_drive(install).display().to_string()
    } else {
        typed.to_owned()
    }
}

pub fn pick(picked: PathBuf, data: &Data) -> PathBuf {
    let empty = std::fs::read_dir(&picked).is_ok_and(|mut d| d.next().is_none());
    let named = picked.file_name().is_some_and(|n| n.eq_ignore_ascii_case(FOLDER));
    if empty || named || same_folder(&picked, &data.current()) {
        picked
    } else {
        picked.join(FOLDER)
    }
}

#[cfg(test)]
mod tests;
