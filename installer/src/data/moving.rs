use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::data::files::{Entry, copy_tree, listing, remove};
use crate::data::{CHECKING, COPYING, Data, ENV, FOLDER, MOVING};
use swofty_core::data::relocate::{
    journal_pending,
    link::{create as link, remove as unlink},
};
use swofty_core::launcher::Progress;
use swofty_core::platform::disk;
use swofty_core::platform::process::{same_folder, same_path};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Record {
    Link,
    Unlink,
    Env,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataMove {
    from: PathBuf,
    pub(super) to: PathBuf,
    pub(super) default: PathBuf,
    linked: bool,
    pub(super) record: Record,
}

pub fn plan(data: &Data, to: &Path) -> Result<Option<DataMove>, String> {
    if journal_pending(&data.default) {
        return Err(
            "A data folder move from the launcher is not finished. Open Swofty Launcher to finish it, then run the installer again."
                .into(),
        );
    }
    if to.is_absolute() && to.parent().is_none() {
        return Err(format!(
            "{} is a whole drive. Choose a folder on it, such as {}.",
            to.display(),
            to.join(FOLDER).display()
        ));
    }
    data.apart(to)?;
    let linked = data.link();
    let (from, record) = match (&data.env, &linked) {
        (Some(env), _) if same_folder(env, to) => return Ok(None),
        (Some(env), _) if !data.movable() => {
            return Err(format!(
                "{ENV} sets the data folder to {}. Change the variable to move it.",
                env.display()
            ));
        }
        (Some(env), _) => (env.clone(), Record::Env),
        (None, Some(target)) if same_path(to, &data.default) => (target.clone(), Record::Unlink),
        (None, Some(target)) if same_folder(to, target) => return Ok(None),
        (None, None) if same_folder(to, &data.default) => return Ok(None),
        (None, _) => (
            linked.clone().unwrap_or_else(|| data.default.clone()),
            Record::Link,
        ),
    };
    if record != Record::Env
        && let Some(target) = &linked
        && !target.exists()
    {
        return Err(format!(
            "{} links to {}, which is missing. Reconnect that drive or folder, or restore it, before moving your data.",
            data.default.display(),
            target.display()
        ));
    }
    if !to.is_absolute() {
        return Err("Choose a full path for the data folder.".into());
    }
    let within = |dir: &Path| to.ancestors().any(|a| same_path(a, dir));
    if within(&from) || (record == Record::Link && within(&data.default)) {
        return Err(format!(
            "Choose a data folder outside {}.",
            if within(&from) { &from } else { &data.default }.display()
        ));
    }
    if record != Record::Unlink {
        vacant(to)?;
    }
    Ok(Some(DataMove {
        from,
        to: to.to_path_buf(),
        default: data.default.clone(),
        linked: linked.is_some(),
        record,
    }))
}

fn vacant(to: &Path) -> Result<(), String> {
    let Ok(meta) = to.symlink_metadata() else {
        return Ok(());
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "{} is not a folder. Choose an empty folder or a new one.",
            to.display()
        ));
    }
    match std::fs::read_dir(to).map(|mut d| d.next().is_none()) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "{} is not empty. Choose an empty folder or a new one.",
            to.display()
        )),
        Err(e) => Err(format!("Could not read {}: {e}", to.display())),
    }
}

pub struct Ops {
    pub rename: fn(&Path, &Path) -> io::Result<()>,
    pub blocked: fn() -> Option<String>,
    pub free: fn(&Path) -> Option<u64>,
    pub set_env: fn(&Path) -> io::Result<()>,
}

impl Ops {
    pub fn system() -> Self {
        Self {
            rename: |a, b| std::fs::rename(a, b),
            blocked: crate::running::data_block,
            free: disk::free_space,
            set_env,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Moved {
    Renamed,
    Copied,
    Created,
}

impl DataMove {
    pub fn env(&self) -> Option<&Path> {
        (self.record == Record::Env).then_some(self.to.as_path())
    }

    pub fn describe(&self) -> String {
        let (from, to, default) = (self.from.display(), self.to.display(), self.default.display());
        let fresh = self.from.symlink_metadata().is_err();
        match self.record {
            Record::Link if fresh => format!("Creates {to} for your data and links {default} there."),
            Record::Link => format!("Moves your data from {from} to {to} and links {default} there."),
            Record::Unlink => format!("Moves your data from {from} back to {default} and removes the link."),
            Record::Env => format!("Moves your data from {from} to {to} and sets {ENV} to {to}."),
        }
    }

    pub fn run(&self, ops: &Ops, progress: &dyn Progress) -> Result<(), String> {
        if let Some(why) = (ops.blocked)() {
            return Err(why);
        }
        swofty_core::data::relocate::history::record(&self.default, &self.from, "moved by the installer");
        progress.report(0.0, MOVING);
        match self.record {
            Record::Link => self.relink(ops, progress),
            Record::Unlink => self.unlink(ops, progress),
            Record::Env => self.set_env(ops, progress),
        }
    }

    fn relink(&self, ops: &Ops, progress: &dyn Progress) -> Result<(), String> {
        let moved = transfer(&self.from, &self.to, ops, progress)?;
        let aside = (!self.linked && moved == Moved::Copied).then(|| aside(&self.default));
        let cleared = if self.linked {
            unlink(&self.default)
                .map_err(|e| format!("Could not remove the link at {}: {e}", self.default.display()))
        } else if let Some(aside) = &aside {
            std::fs::rename(&self.default, aside)
                .map_err(|e| format!("Could not set {} aside: {e}", self.default.display()))
        } else {
            Ok(())
        };
        let linked = cleared.and_then(|()| {
            if let Some(parent) = self.default.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            link(&self.to, &self.default).map_err(|e| {
                format!(
                    "Could not link {} to {}: {e}",
                    self.default.display(),
                    self.to.display()
                )
            })
        });
        if let Err(e) = linked {
            if self.default.symlink_metadata().is_err() {
                match &aside {
                    Some(aside) => std::fs::rename(aside, &self.default).ok(),
                    None if self.linked => link(&self.from, &self.default).ok(),
                    None => None,
                };
            }
            undo(moved, &self.from, &self.to);
            return Err(e);
        }
        if moved == Moved::Copied {
            discard(aside.as_deref().unwrap_or(&self.from), &self.to)?;
        }
        Ok(())
    }

    fn unlink(&self, ops: &Ops, progress: &dyn Progress) -> Result<(), String> {
        unlink(&self.default)
            .map_err(|e| format!("Could not remove the link at {}: {e}", self.default.display()))?;
        let moved = match transfer(&self.from, &self.to, ops, progress) {
            Ok(moved) => moved,
            Err(e) => {
                link(&self.from, &self.default).ok();
                return Err(e);
            }
        };
        if moved == Moved::Copied {
            discard(&self.from, &self.to)?;
        }
        Ok(())
    }

    fn set_env(&self, ops: &Ops, progress: &dyn Progress) -> Result<(), String> {
        let moved = transfer(&self.from, &self.to, ops, progress)?;
        if let Err(e) = (ops.set_env)(&self.to) {
            undo(moved, &self.from, &self.to);
            return Err(format!("Could not set {ENV} to {}: {e}", self.to.display()));
        }
        if moved == Moved::Copied {
            discard(&self.from, &self.to)?;
        }
        Ok(())
    }
}

fn transfer(from: &Path, to: &Path, ops: &Ops, progress: &dyn Progress) -> Result<Moved, String> {
    if from.symlink_metadata().is_err() {
        std::fs::create_dir_all(to).map_err(|e| format!("Could not create {}: {e}", to.display()))?;
        return Ok(Moved::Created);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    let existed = to.symlink_metadata().is_ok();
    if existed {
        std::fs::remove_dir(to).map_err(|e| format!("Could not replace the empty {}: {e}", to.display()))?;
    }
    let restore = || {
        if existed {
            std::fs::create_dir(to).ok();
        }
    };
    match (ops.rename)(from, to) {
        Ok(()) => return Ok(Moved::Renamed),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {}
        Err(e) => {
            restore();
            return Err(format!(
                "Could not move {} to {}: {e}",
                from.display(),
                to.display()
            ));
        }
    }
    copy_verified(from, to, ops, progress).inspect_err(|_| {
        remove(to).ok();
        restore();
    })?;
    Ok(Moved::Copied)
}

fn copy_verified(from: &Path, to: &Path, ops: &Ops, progress: &dyn Progress) -> Result<(), String> {
    let total: u64 = listing(from)
        .map_err(|e| format!("Could not read {}: {e}", from.display()))?
        .values()
        .map(Entry::size)
        .sum();
    if let Some(free) = (ops.free)(to)
        && free < total
    {
        return Err(format!(
            "Not enough free space for the data folder in {}: it needs {} GB and {} GB is free.",
            to.display(),
            disk::gb(total),
            disk::gb(free)
        ));
    }
    progress.report(0.0, COPYING);
    let mut done = 0u64;
    copy_tree(from, to, &mut |n| {
        done += n;
        progress.report(done as f32 / total.max(1) as f32, COPYING);
    })
    .map_err(|e| format!("Could not copy {} to {}: {e}", from.display(), to.display()))?;
    progress.report(1.0, CHECKING);
    let same = listing(from)
        .ok()
        .zip(listing(to).ok())
        .is_some_and(|(a, b)| a == b);
    if !same {
        return Err(format!(
            "The copy in {} does not match {}. Your data stays in {}.",
            to.display(),
            from.display(),
            from.display()
        ));
    }
    Ok(())
}

fn undo(moved: Moved, from: &Path, to: &Path) {
    match moved {
        Moved::Renamed => std::fs::rename(to, from).ok(),
        Moved::Copied => remove(to).ok(),
        Moved::Created => std::fs::remove_dir(to).ok(),
    };
}

fn discard(old: &Path, to: &Path) -> Result<(), String> {
    remove(old).map_err(|e| {
        format!(
            "Moved your data folder to {}, but could not delete the old copy in {}: {e}. Delete it yourself.",
            to.display(),
            old.display()
        )
    })
}

fn aside(default: &Path) -> PathBuf {
    let name = default
        .file_name()
        .map_or_else(|| FOLDER.into(), |n| n.to_string_lossy().into_owned());
    (1..)
        .map(|i| match i {
            1 => default.with_file_name(format!("{name}.old")),
            i => default.with_file_name(format!("{name}.old-{i}")),
        })
        .find(|p| p.symlink_metadata().is_err())
        .expect("an unused name")
}

#[cfg(windows)]
fn set_env(to: &Path) -> io::Result<()> {
    crate::win32::set_user_env(ENV, to)
}

#[cfg(not(windows))]
fn set_env(_: &Path) -> io::Result<()> {
    Err(io::Error::other(format!("set {ENV} where you set it before")))
}
