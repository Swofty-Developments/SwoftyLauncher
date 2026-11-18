use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use swofty_core::platform::process::same_folder;
use swofty_core::updates::release::Install;

use crate::existing::roots::{remove_stale, running_roots};
use crate::existing::{DATA_ENTRIES, RELEASE, is_install};
use crate::shortcut;

#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub(super) from: PathBuf,
    pub(super) to: PathBuf,
    pub(super) entries: Vec<PathBuf>,
}

pub fn plan(from: &Path, to: &Path, data: &Path) -> Result<Move, String> {
    if !is_install(from) {
        return Err(format!("There is no install in {} to move.", from.display()));
    }
    if is_install(to) {
        return Err(format!("{} already holds an install.", to.display()));
    }
    let owned = data_inside(from, data);
    let entries: Vec<PathBuf> = RELEASE
        .iter()
        .filter(|e| !(owned && DATA_ENTRIES.contains(e)))
        .map(PathBuf::from)
        .filter(|e| from.join(e).symlink_metadata().is_ok())
        .collect();
    for rel in &entries {
        let (src, dst) = (from.join(rel), to.join(rel));
        if to.starts_with(&src) {
            return Err(format!("Choose a folder outside {}.", src.display()));
        }
        if dst.symlink_metadata().is_ok() {
            return Err(format!(
                "{} is already there. Choose an empty folder.",
                dst.display()
            ));
        }
    }
    Ok(Move {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        entries,
    })
}

fn data_inside(from: &Path, data: &Path) -> bool {
    same_folder(from, data) || {
        let resolved = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        resolved(data).starts_with(resolved(from))
    }
}

impl Move {
    pub fn run(&self, desktop: bool) -> Result<(), String> {
        if running_roots().iter().any(|r| same_folder(r, &self.from)) {
            return Err("Close the Swofty Launcher before moving it.".into());
        }
        if swofty_core::platform::process::game_running() {
            return Err("Close F1 25 before moving the launcher.".into());
        }
        Install {
            root: self.from.clone(),
        }
        .cleanup();
        self.relocate(|from, to| std::fs::rename(from, to))?;
        let exe = Install {
            root: self.to.clone(),
        }
        .launcher();
        shortcut::create(&self.to, &exe, desktop);
        remove_stale(&self.from);
        Ok(())
    }

    pub(super) fn relocate(
        &self,
        rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<(), String> {
        let created = !self.to.exists();
        std::fs::create_dir_all(&self.to)
            .map_err(|e| format!("Could not create {}: {e}", self.to.display()))?;
        let mut done: Vec<(&Path, bool)> = Vec::new();
        for rel in &self.entries {
            let (src, dst) = (self.from.join(rel), self.to.join(rel));
            let moved = dst
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| match rename(&src, &dst) {
                    Ok(()) => Ok(false),
                    Err(e) if e.kind() == ErrorKind::CrossesDevices => copy(&src, &dst).map(|_| true),
                    Err(e) => Err(e),
                });
            match moved {
                Ok(copied) => done.push((rel.as_path(), copied)),
                Err(e) => {
                    self.undo(&done, created);
                    return Err(format!("Could not move {}: {e}", src.display()));
                }
            }
        }
        if let Err(e) = self.verify(&done) {
            self.undo(&done, created);
            return Err(e);
        }
        for (rel, copied) in &done {
            if *copied {
                remove(&self.from.join(rel)).ok();
            }
        }
        prune(&self.from, &self.entries);
        std::fs::remove_dir(&self.from).ok();
        Ok(())
    }

    fn verify(&self, done: &[(&Path, bool)]) -> Result<(), String> {
        for (rel, copied) in done {
            let (src, dst) = (self.from.join(rel), self.to.join(rel));
            let whole = if *copied {
                sizes(&src).is_ok_and(|a| sizes(&dst).is_ok_and(|b| a == b))
            } else {
                dst.symlink_metadata().is_ok() && src.symlink_metadata().is_err()
            };
            if !whole {
                return Err(format!(
                    "{} did not move whole to {}.",
                    src.display(),
                    dst.display()
                ));
            }
        }
        if !is_install(&self.to) {
            return Err(format!(
                "The moved install in {} is not complete.",
                self.to.display()
            ));
        }
        Ok(())
    }

    pub(super) fn undo(&self, done: &[(&Path, bool)], created: bool) {
        for (rel, copied) in done.iter().rev() {
            let (src, dst) = (self.from.join(rel), self.to.join(rel));
            if *copied {
                remove(&dst).ok();
            } else {
                std::fs::rename(&dst, &src).ok();
            }
        }
        prune(&self.to, &self.entries);
        if created {
            std::fs::remove_dir(&self.to).ok();
        }
    }
}

fn prune(base: &Path, entries: &[PathBuf]) {
    for rel in entries {
        let path = base.join(rel);
        for dir in path.ancestors().skip(1).take_while(|d| *d != base) {
            std::fs::remove_dir(dir).ok();
        }
    }
}

pub(super) fn copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    fn tree(src: &Path, dst: &Path) -> std::io::Result<()> {
        if std::fs::metadata(src)?.is_dir() {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                tree(&entry.path(), &dst.join(entry.file_name()))?;
            }
            Ok(())
        } else {
            std::fs::copy(src, dst).map(drop)
        }
    }
    let copied = tree(src, dst);
    if copied.is_err() {
        remove(dst).ok();
    }
    copied
}

fn remove(path: &Path) -> std::io::Result<()> {
    if path.symlink_metadata()?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn sizes(path: &Path) -> std::io::Result<BTreeMap<PathBuf, u64>> {
    fn walk(path: &Path, rel: PathBuf, out: &mut BTreeMap<PathBuf, u64>) -> std::io::Result<()> {
        let meta = std::fs::metadata(path)?;
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                walk(&entry.path(), rel.join(entry.file_name()), out)?;
            }
        } else {
            out.insert(rel, meta.len());
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(path, PathBuf::new(), &mut out)?;
    Ok(out)
}
