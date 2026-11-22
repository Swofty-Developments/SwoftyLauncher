//! The installed copy of the launcher and swapping a new release into it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;

use crate::data::relocate::{link, real_path, resolved};
use crate::launcher::Progress;
use crate::platform::process::same_path;
use crate::{Error, Result, io_err};

use super::download::{check_download, download_release, parse_sums};
use super::fetch::get_text;
use super::unpack::{exe, extract, release_root};
use super::{Asset, Release, now_secs};

pub const STAGING_DIR: &str = ".update";
/// Beside the staging folder rather than in it, so cleaning that folder never deletes a lock in use.
pub(super) const LOCK_FILE: &str = ".update.lock";
pub const INSTALL_MANIFEST: &str = ".install-manifest";
pub(super) const SWEPT_DIRS: [&str; 3] = ["loader", "addons", crate::content::postfx::DXC_DIR];

#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    pub root: PathBuf,
}

impl Install {
    pub fn current() -> std::result::Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Self::locate(&exe)?.apart_from(&crate::data::paths::data_dir())
    }

    pub fn apart_from(self, data: &Path) -> std::result::Result<Self, String> {
        if holds_data(&self.root, data) {
            return Err(DATA_IN_INSTALL.into());
        }
        Ok(self)
    }

    pub fn locate(exe: &Path) -> std::result::Result<Self, String> {
        let bin = exe
            .parent()
            .filter(|b| b.file_name().is_some_and(|n| n == "bin"))
            .ok_or("this copy does not run from a release folder")?;
        let root = bin
            .parent()
            .ok_or("this copy does not run from a release folder")?;
        if root.starts_with("/nix/store") {
            return Err("this copy is installed with Nix; update it with Nix".into());
        }
        if !crate::addons::builtin::is_loader(&root.join("loader")) {
            return Err("this copy has no loader folder next to bin".into());
        }
        for dir in [root, bin, &root.join("loader")] {
            let probe = dir.join(format!(".update-probe-{}", std::process::id()));
            std::fs::write(&probe, b"").map_err(|_| format!("no write access to {}", dir.display()))?;
            std::fs::remove_file(&probe).ok();
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn launcher(&self) -> PathBuf {
        self.root.join("bin").join(exe("swofty-launcher"))
    }

    pub fn staging(&self) -> PathBuf {
        self.root.join(STAGING_DIR)
    }

    /// Recover and remove what an update that died left in the staging folder. Leaves it alone and
    /// returns false while another process's update holds it; true once the folder is gone.
    pub fn cleanup(&self) -> bool {
        let Some(_lock) = UpdateLock::try_acquire(&self.root) else {
            return false;
        };
        self.clean_staging()
    }

    fn clean_staging(&self) -> bool {
        let dir = self.staging();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for work in entries.flatten().map(|e| e.path()) {
                if work.join(APPLYING).is_file() {
                    recover(&work, &self.root);
                }
            }
        }
        std::fs::remove_dir_all(&dir).ok();
        !dir.exists()
    }

    pub fn update(&self, release: &Release, progress: &dyn Progress) -> Result<Version> {
        let version = release
            .version()
            .ok_or_else(|| Error::Other(format!("{} is not a version", release.tag_name)))?;
        if crate::platform::process::game_running() {
            return Err(Error::Other("close F1 25 before updating".into()));
        }
        refuse_data_root(&self.root, &crate::data::paths::data_dir())?;
        let asset = release.platform_asset()?;
        let Some(_lock) = UpdateLock::try_acquire(&self.root) else {
            return Err(Error::Other(
                "another launcher or swofty update is updating this copy right now; wait for it to finish"
                    .into(),
            ));
        };
        self.clean_staging();
        let work = self
            .staging()
            .join(format!("{}-{}", std::process::id(), now_secs()));
        std::fs::create_dir_all(&work).map_err(io_err(&work))?;
        let result = self.stage_and_apply(release, asset, &work, progress);
        self.clean_staging();
        result.map(|_| version)
    }

    fn stage_and_apply(
        &self,
        release: &Release,
        asset: &Asset,
        staging: &Path,
        progress: &dyn Progress,
    ) -> Result<()> {
        let sums = match release.sums() {
            Some(s) => Some(parse_sums(&get_text(
                &s.browser_download_url,
                Duration::from_secs(60),
            )?)),
            None => None,
        };
        let archive = staging.join(&asset.name);
        let total = asset.size.max(1) as f32;
        let text = format!("Downloading {}", release.tag_name);
        let (len, sha256) = download_release(release, asset, &archive, &|n| {
            progress.report(0.9 * n as f32 / total, &text);
        })?;
        check_download(asset, len, &sha256, sums.as_ref())?;
        progress.report(0.9, "Extracting");
        let extracted = staging.join("new");
        extract(&archive, &extracted)?;
        let new = release_root(&extracted)?;
        if crate::platform::process::game_running() {
            return Err(Error::Other("close F1 25 before updating".into()));
        }
        refuse_data_root(&self.root, &crate::data::paths::data_dir())?;
        progress.report(0.95, "Replacing files");
        apply(&new, &self.root, &staging.join("old"))
    }
}

/// Held for the whole of an update, so another launcher's start-up cleanup or a `swofty update`
/// leaves its download and its half-applied swap alone. The operating system drops it when the
/// process ends, which is how a later cleanup tells a dead update's leftovers from a live one.
/// Where the file system cannot lock, updates go unguarded.
pub(super) struct UpdateLock {
    _file: Option<std::fs::File>,
}

impl UpdateLock {
    /// Take the lock, or `None` while another update holds it.
    pub(super) fn try_acquire(root: &Path) -> Option<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(LOCK_FILE));
        let Ok(file) = file else {
            return Some(Self { _file: None });
        };
        match file.try_lock() {
            Err(std::fs::TryLockError::WouldBlock) => None,
            Ok(()) | Err(std::fs::TryLockError::Error(_)) => Some(Self { _file: Some(file) }),
        }
    }
}

pub(super) struct Swap {
    target: PathBuf,
    saved: Option<PathBuf>,
}

pub(super) fn swap(new: Option<&Path>, target: &Path, saved: &Path) -> std::io::Result<Swap> {
    let had = target.symlink_metadata().is_ok();
    if had {
        std::fs::create_dir_all(saved.parent().expect("under the staging folder"))?;
        if cfg!(windows) || new.is_none() {
            std::fs::rename(target, saved)?;
        } else if std::fs::hard_link(target, saved).is_err() {
            std::fs::copy(target, saved)?;
        }
    }
    let swap = Swap {
        target: target.to_path_buf(),
        saved: had.then(|| saved.to_path_buf()),
    };
    let Some(new) = new else {
        return Ok(swap);
    };
    let moved = target
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::rename(new, target));
    if let Err(e) = moved {
        if had && cfg!(windows) {
            std::fs::rename(saved, target).ok();
        }
        return Err(e);
    }
    Ok(swap)
}

pub(super) const APPLYING: &str = "applying";

pub(super) fn recover(work: &Path, root: &Path) {
    let backup = work.join("old");
    if let Ok(files) = crate::addons::builtin::loader_files(&backup) {
        for (rel, saved) in files {
            restore(&saved, &root.join(&rel), work);
        }
    }
    let added = std::fs::read_to_string(work.join(APPLYING)).unwrap_or_default();
    for rel in added.lines().filter(|l| !l.is_empty()) {
        std::fs::remove_file(root.join(rel)).ok();
    }
}

pub(super) fn restore(saved: &Path, target: &Path, work: &Path) {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if std::fs::rename(saved, target).is_ok() {
        return;
    }
    let aside = work.with_file_name(format!("aside-{}-{}", std::process::id(), now_secs()));
    let name = target.file_name().map(|n| aside.join(n));
    if let Some(name) = name
        && std::fs::create_dir_all(&aside).is_ok()
        && std::fs::rename(target, &name).is_ok()
    {
        std::fs::rename(saved, target).ok();
    }
}

pub(super) fn roll_back(done: &[Swap]) {
    for s in done.iter().rev() {
        match &s.saved {
            Some(saved) => std::fs::rename(saved, &s.target).ok(),
            None => std::fs::remove_file(&s.target).ok(),
        };
    }
}

pub const DATA_IN_INSTALL: &str = "your addons and saves are in the launcher's folder, so updates cannot install. Run the installer and install to a new, empty folder: it moves only the launcher's files";

pub(super) fn holds_data(root: &Path, data: &Path) -> bool {
    resolved(data).starts_with(resolved(root))
}

pub fn refuse_data_root(root: &Path, data: &Path) -> Result<()> {
    let (install, folder) = (real_path(root), real_path(data));
    let why = if holds_data(root, data) {
        let linked = link::target(data)
            .map(|_| format!(" ({} links to it)", data.display()))
            .unwrap_or_default();
        let layout = if same_path(&install, &folder) {
            format!(
                "the install folder {} is also the data folder{linked}",
                install.display()
            )
        } else {
            format!(
                "the data folder {}{linked} is inside the install folder {}",
                folder.display(),
                install.display()
            )
        };
        Some(format!(
            "{layout}. Install the launcher to a new, empty folder with the installer: it moves only the launcher's files and leaves your addons and saves in {}",
            folder.display()
        ))
    } else {
        ["config.json", "profiles", "saves"]
            .iter()
            .find(|m| root.join(m).symlink_metadata().is_ok())
            .map(|m| {
                format!(
                    "the install folder {} holds {m}, so it looks like a data folder the launcher could not resolve (data folder resolved to {})",
                    install.display(),
                    folder.display()
                )
            })
    };
    match why {
        Some(why) => {
            let why = format!("refusing to update: {why}");
            eprintln!("swofty: {why}");
            Err(Error::Other(why))
        }
        None => Ok(()),
    }
}

pub(super) fn read_install_manifest(root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(root.join(INSTALL_MANIFEST)).ok()?;
    Some(
        text.lines()
            .filter(|l| !l.is_empty())
            .filter(|l| {
                !l.split('/').any(|c| c == ".." || c.is_empty())
                    && !l.starts_with('/')
                    && SWEPT_DIRS.iter().any(|d| l.starts_with(&format!("{d}/")))
            })
            .map(str::to_owned)
            .collect(),
    )
}

pub fn apply(new: &Path, root: &Path, backup: &Path) -> Result<()> {
    let files = crate::addons::builtin::loader_files(new)?;
    let previous = read_install_manifest(root);
    let mut stale: Vec<String> = Vec::new();
    match &previous {
        Some(prev) => stale.extend(
            prev.iter()
                .filter(|rel| !files.contains_key(*rel) && root.join(rel).symlink_metadata().is_ok())
                .cloned(),
        ),
        None => {
            eprintln!(
                "swofty: no {INSTALL_MANIFEST} from the previous install; sweeping only loader/, dxc/ and the bundled addons the new package holds"
            );
            let bundled: std::collections::BTreeSet<&str> = files
                .keys()
                .filter_map(|rel| rel.strip_prefix("addons/")?.split_once('/').map(|(id, _)| id))
                .collect();
            for id in bundled {
                let dir = root.join("addons").join(id);
                if dir.is_dir() {
                    stale.extend(
                        crate::addons::builtin::loader_files(&dir)?
                            .into_keys()
                            .map(|rel| format!("addons/{id}/{rel}"))
                            .filter(|rel| !files.contains_key(rel)),
                    );
                }
            }
            for sub in ["loader", crate::content::postfx::DXC_DIR] {
                let dir = root.join(sub);
                if dir.is_dir() {
                    stale.extend(
                        crate::addons::builtin::loader_files(&dir)?
                            .into_keys()
                            .map(|rel| format!("{sub}/{rel}"))
                            .filter(|rel| !files.contains_key(rel)),
                    );
                }
            }
        }
    }
    let steps = files
        .iter()
        .map(|(rel, src)| (rel.as_str(), Some(src.as_path())))
        .chain(stale.iter().map(|rel| (rel.as_str(), None)));
    let added: String = files
        .keys()
        .filter(|rel| root.join(rel).symlink_metadata().is_err())
        .map(|rel| format!("{rel}\n"))
        .collect();
    let journal = backup
        .parent()
        .map_or_else(|| backup.join(APPLYING), |w| w.join(APPLYING));
    std::fs::create_dir_all(backup).map_err(io_err(backup))?;
    std::fs::write(&journal, added).map_err(io_err(&journal))?;
    let mut done = Vec::new();
    for (rel, src) in steps {
        let target = root.join(rel);
        match swap(src, &target, &backup.join(rel)) {
            Ok(s) => done.push(s),
            Err(e) => {
                roll_back(&done);
                std::fs::remove_file(&journal).ok();
                return Err(Error::Io {
                    path: target,
                    source: e,
                });
            }
        }
    }
    std::fs::remove_file(&journal).map_err(io_err(&journal))?;
    let recorded: String = files
        .keys()
        .filter(|rel| SWEPT_DIRS.iter().any(|d| rel.starts_with(&format!("{d}/"))))
        .map(|rel| format!("{rel}\n"))
        .collect();
    if let Err(e) = std::fs::write(root.join(INSTALL_MANIFEST), recorded) {
        eprintln!("swofty: couldn't record {INSTALL_MANIFEST}: {e}");
    }
    Ok(())
}
