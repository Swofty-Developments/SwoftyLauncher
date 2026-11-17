use std::path::Path;

use anyhow::{Context, Result, bail};
use swofty_core::manifest::AddonManifest;
use swofty_core::store::Store;

pub fn link(store: &Store, profile: &str, folder: &Path) -> Result<String> {
    let m = AddonManifest::load(folder).map_err(anyhow::Error::msg)?;
    let target = store.addons_dir().join(&m.id);
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        let linked = meta.file_type().is_symlink() || std::fs::read_link(&target).is_ok();
        if !linked {
            bail!(
                "{} is installed as a copy; remove it first with swofty addons remove {}",
                m.id,
                m.id
            );
        }
        remove_link(&target)?;
    }
    std::fs::create_dir_all(store.addons_dir())?;
    make_link(folder, &target)?;
    let mut p = store.profile(profile)?;
    store.registry()?.set_enabled(&mut p, &m.id, true)?;
    store.save_profile(&p)?;
    Ok(m.id)
}

#[cfg(unix)]
fn make_link(folder: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(folder, target).with_context(|| format!("linking {}", target.display()))
}

#[cfg(windows)]
fn make_link(folder: &Path, target: &Path) -> Result<()> {
    junction::create(folder, target)
}

#[cfg(any(windows, test))]
pub fn junction_target(canonical: &str) -> Result<String> {
    if canonical.starts_with(r"\\?\UNC\") || canonical.starts_with(r"\\") && !canonical.starts_with(r"\\?\") {
        bail!("{canonical} is a network folder, and a junction can only point at a local one");
    }
    let path = canonical
        .strip_prefix(r"\\?\")
        .unwrap_or(canonical)
        .replace('/', "\\");
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
        bail!("{canonical} isn't a full path on a drive");
    }
    Ok(path)
}

#[cfg(any(windows, test))]
pub fn mount_point_buffer(target: &str) -> Result<Vec<u8>> {
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const MAXIMUM_REPARSE_DATA_BUFFER_SIZE: usize = 16 * 1024;
    let substitute: Vec<u16> = format!(r"\??\{target}").encode_utf16().collect();
    let print: Vec<u16> = target.encode_utf16().collect();
    let path_bytes = (substitute.len() + 1 + print.len() + 1) * 2;
    let data_len = 8 + path_bytes;
    if 8 + data_len > MAXIMUM_REPARSE_DATA_BUFFER_SIZE {
        bail!("{target} is too long for a junction");
    }
    let mut buf = Vec::with_capacity(8 + data_len);
    buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend_from_slice(&(data_len as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&((substitute.len() * 2) as u16).to_le_bytes());
    buf.extend_from_slice(&((substitute.len() * 2 + 2) as u16).to_le_bytes());
    buf.extend_from_slice(&((print.len() * 2) as u16).to_le_bytes());
    for c in substitute.iter().chain([&0]).chain(print.iter()).chain([&0]) {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    Ok(buf)
}

#[cfg(windows)]
mod junction {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use anyhow::{Context, Result, bail};

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut core::ffi::c_void,
            code: u32,
            input: *const core::ffi::c_void,
            input_len: u32,
            output: *mut core::ffi::c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    pub fn create(folder: &Path, at: &Path) -> Result<()> {
        let canonical = std::fs::canonicalize(folder).with_context(|| {
            format!(
                "couldn't make a junction at {}: {} can't be read",
                at.display(),
                folder.display()
            )
        })?;
        let target = super::junction_target(&canonical.to_string_lossy())
            .with_context(|| format!("couldn't make a junction at {}", at.display()))?;
        let buf = super::mount_point_buffer(&target)?;
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "couldn't make a junction at {}: its folder can't be made",
                    at.display()
                )
            })?;
        }
        std::fs::create_dir(at).with_context(|| format!("couldn't make a junction at {}", at.display()))?;
        let result = (|| -> Result<()> {
            let dir = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(at)
                .context("the new folder can't be opened")?;
            let mut returned = 0u32;
            // SAFETY: the handle is an open directory that outlives the call, and buf is a complete
            // REPARSE_DATA_BUFFER of buf.len() bytes; no output buffer or OVERLAPPED is passed.
            let ok = unsafe {
                DeviceIoControl(
                    dir.as_raw_handle(),
                    FSCTL_SET_REPARSE_POINT,
                    buf.as_ptr().cast(),
                    buf.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                bail!(
                    "setting its reparse point failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            Ok(())
        })();
        if let Err(e) = result {
            std::fs::remove_dir(at).ok();
            return Err(e.context(format!(
                "couldn't make a junction at {} pointing at {target}",
                at.display()
            )));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn remove_link(target: &Path) -> Result<()> {
    std::fs::remove_file(target).with_context(|| format!("{}", target.display()))
}

#[cfg(windows)]
fn remove_link(target: &Path) -> Result<()> {
    std::fs::remove_dir(target).with_context(|| format!("{}", target.display()))
}
