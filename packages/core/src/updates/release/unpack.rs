//! Unpacking a downloaded release archive.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{Error, Result, io_err};

pub(super) fn safe_path(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => out.push(p),
        }
    }
    Some(out)
}

pub fn extract(archive: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(io_err(to))?;
    let file = std::fs::File::open(archive).map_err(io_err(archive))?;
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let raw = entry.name()?.into_owned();
            let Some(rel) = safe_path(&raw) else {
                return Err(Error::Other(format!("{name}: unsafe path {raw}")));
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let path = to.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&path).map_err(io_err(&path))?;
                continue;
            }
            write_entry(&mut entry, &path, None)?;
        }
        return Ok(());
    }
    if !name.ends_with(".tar.gz") {
        return Err(Error::Other(format!("{name}: not a .zip or .tar.gz archive")));
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(io_err(archive))? {
        let mut entry = entry.map_err(io_err(archive))?;
        let raw = entry
            .path()
            .map_err(io_err(archive))?
            .to_string_lossy()
            .into_owned();
        let Some(rel) = safe_path(&raw) else {
            return Err(Error::Other(format!("{name}: unsafe path {raw}")));
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let path = to.join(rel);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&path).map_err(io_err(&path))?;
        } else if kind.is_file() {
            let mode = entry.header().mode().ok();
            write_entry(&mut entry, &path, mode)?;
        }
    }
    Ok(())
}

pub(super) fn write_entry(entry: &mut impl Read, path: &Path, mode: Option<u32>) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let mut out = std::fs::File::create(path).map_err(io_err(path))?;
    std::io::copy(entry, &mut out).map_err(io_err(path))?;
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o755))
            .map_err(io_err(path))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

pub fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

pub fn release_root(extracted: &Path) -> Result<PathBuf> {
    let complete = |dir: &Path| {
        dir.join("bin").join(exe("swofty-launcher")).is_file()
            && dir.join("bin").join(exe("swofty")).is_file()
            && crate::addons::builtin::is_loader(&dir.join("loader"))
    };
    if complete(extracted) {
        return Ok(extracted.to_path_buf());
    }
    let dirs: Vec<PathBuf> = std::fs::read_dir(extracted)
        .map_err(io_err(extracted))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    match dirs.as_slice() {
        [one] if complete(one) => Ok(one.clone()),
        _ => Err(Error::Other(
            "the archive does not hold bin/ with the launcher and the command line, and loader/".into(),
        )),
    }
}
