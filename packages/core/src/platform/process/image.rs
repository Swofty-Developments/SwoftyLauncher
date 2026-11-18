//! Which copy of the game a running F1_25.exe is: its exe file, and under Wine the Windows path it was
//! started with and its prefix.

#[cfg(any(target_os = "linux", test))]
use std::path::PathBuf;

use super::Origin;
#[cfg(any(target_os = "linux", test))]
use super::paths::exe_folder;

#[cfg(target_os = "linux")]
pub(super) fn origin(pid: u32) -> Origin {
    use std::io::BufRead;
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let image = std::fs::File::open(proc.join("maps")).ok().and_then(|f| {
        std::io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .find_map(|l| mapped_exe(&l))
    });
    let command = std::fs::read(proc.join("cmdline")).ok().and_then(|raw| {
        let first = raw.split(|b| *b == 0).next()?;
        (!first.is_empty()).then(|| String::from_utf8_lossy(first).into_owned())
    });
    let prefix = std::fs::read(proc.join("environ"))
        .ok()
        .and_then(|raw| wine_prefix(&raw));
    Origin {
        image,
        command,
        prefix,
    }
}

/// The game's exe in a `/proc/<pid>/maps` line: Wine maps the exe's headers
/// from the file, under the path the kernel resolved when it was opened.
#[cfg(any(target_os = "linux", test))]
pub(super) fn mapped_exe(line: &str) -> Option<String> {
    let path = &line[line.find('/')?..];
    exe_folder(path).map(|_| path.to_owned())
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn wine_prefix(environ: &[u8]) -> Option<PathBuf> {
    let var = |name: &str| {
        environ.split(|b| *b == 0).find_map(|kv| {
            let value = kv.strip_prefix(name.as_bytes())?.strip_prefix(b"=")?;
            (!value.is_empty()).then(|| PathBuf::from(String::from_utf8_lossy(value).into_owned()))
        })
    };
    var("WINEPREFIX").or_else(|| var("STEAM_COMPAT_DATA_PATH").map(|d| d.join("pfx")))
}

#[cfg(windows)]
pub(super) fn origin(pid: u32) -> Origin {
    Origin {
        image: image_path(pid),
        ..Origin::default()
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(super) fn origin(_: u32) -> Origin {
    Origin::default()
}

#[cfg(windows)]
pub(super) fn image_path(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let mut name = vec![0u16; 32_768];
    let mut len = name.len() as u32;
    // SAFETY: the handle is checked before use and closed once, and `len`
    // tells the call how many u16s `name` holds.
    let ok = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let ok = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len);
        CloseHandle(process);
        ok
    };
    (ok != 0).then(|| String::from_utf16_lossy(&name[..len as usize]))
}
