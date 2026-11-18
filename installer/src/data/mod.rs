use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use swofty_core::builtin::CORE_ID;
use swofty_core::data_move::{
    self, Location,
    find::{self, Found},
    journal_pending,
    link::{create as link, remove as unlink, target as link_target},
};
use swofty_core::disk;
use swofty_core::launcher::Progress;
use swofty_core::process::{same_folder, same_path};

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
            default: swofty_core::paths::platform_data_dir(),
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
            .filter(|e| !swofty_core::managed::is_managed(&e.file_name().to_string_lossy()))
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
    let bundled = [CORE_ID.to_owned(), swofty_core::managed::ID.to_owned()];
    find::candidates(&location, installs, &bundled)
}

pub fn adopt(data: &Data, found: &Path, installs: &[PathBuf]) -> Result<(), String> {
    let location = Location {
        default: data.default.clone(),
        env: None,
        install: data.install.clone(),
        games: Vec::new(),
    };
    let bundled = [CORE_ID.to_owned(), swofty_core::managed::ID.to_owned()];
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum Record {
    Link,
    Unlink,
    Env,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataMove {
    from: PathBuf,
    to: PathBuf,
    default: PathBuf,
    linked: bool,
    record: Record,
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
        swofty_core::data_move::history::record(&self.default, &self.from, "moved by the installer");
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

#[derive(Debug, PartialEq)]
enum Entry {
    Dir,
    File(u64),
    Link(PathBuf),
}

impl Entry {
    fn size(&self) -> u64 {
        match self {
            Entry::File(n) => *n,
            _ => 0,
        }
    }
}

fn listing(root: &Path) -> io::Result<BTreeMap<PathBuf, Entry>> {
    fn walk(path: &Path, rel: PathBuf, out: &mut BTreeMap<PathBuf, Entry>) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        let entry = if meta.file_type().is_symlink() {
            Entry::Link(std::fs::read_link(path)?)
        } else if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                walk(&entry.path(), rel.join(entry.file_name()), out)?;
            }
            Entry::Dir
        } else {
            Entry::File(meta.len())
        };
        out.insert(rel, entry);
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, PathBuf::new(), &mut out)?;
    Ok(out)
}

fn copy_tree(src: &Path, dst: &Path, copied: &mut dyn FnMut(u64)) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        copy_link(src, dst)
    } else if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()), copied)?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst)?;
        copied(meta.len());
        Ok(())
    }
}

#[cfg(windows)]
fn copy_link(src: &Path, dst: &Path) -> io::Result<()> {
    let target = std::fs::read_link(src)?;
    if std::fs::metadata(src).is_ok_and(|m| m.is_dir()) {
        link(&target, dst)
    } else {
        std::os::windows::fs::symlink_file(&target, dst)
    }
}

#[cfg(not(windows))]
fn copy_link(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)
}

fn remove(path: &Path) -> io::Result<()> {
    let meta = path.symlink_metadata()?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        unlink(path)
    }
}

#[cfg(test)]
mod tests;
