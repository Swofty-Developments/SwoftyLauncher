use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use swofty_core::data::relocate::link::create as link;
use swofty_core::data::relocate::link::remove as unlink;

#[derive(Debug, PartialEq)]
pub(super) enum Entry {
    Dir,
    File(u64),
    Link(PathBuf),
}

impl Entry {
    pub(super) fn size(&self) -> u64 {
        match self {
            Entry::File(n) => *n,
            _ => 0,
        }
    }
}

pub(super) fn listing(root: &Path) -> io::Result<BTreeMap<PathBuf, Entry>> {
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

pub(super) fn copy_tree(src: &Path, dst: &Path, copied: &mut dyn FnMut(u64)) -> io::Result<()> {
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

pub(super) fn remove(path: &Path) -> io::Result<()> {
    let meta = path.symlink_metadata()?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        unlink(path)
    }
}
