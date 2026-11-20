//! The processes that run an exe of a given name, from /proc on Linux and the process list on Windows, and
//! which process started which.

#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;

#[cfg(target_os = "linux")]
use super::times::started;
use super::{GameProcess, Origin};

#[cfg(target_os = "linux")]
pub(crate) fn named(name: &str) -> Vec<GameProcess> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let pid = e.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
            (comm.trim_end() == name && !is_dead(&e.path())).then(|| GameProcess {
                pid,
                started: started(pid),
                origin: Origin::default(),
            })
        })
        .collect()
}

/// A process that has exited and is only waiting for its parent to reap it (`Z`), or is gone (`X`).
#[cfg(target_os = "linux")]
pub(super) fn is_dead(proc_dir: &Path) -> bool {
    std::fs::read_to_string(proc_dir.join("stat")).is_ok_and(|s| state_is_dead(&s))
}

#[cfg(target_os = "linux")]
pub(super) fn state_is_dead(stat: &str) -> bool {
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .is_some_and(|state| matches!(state, "Z" | "X" | "x"))
}

#[cfg(target_os = "linux")]
pub fn descends_from(pid: u32, ancestor: u32) -> bool {
    std::iter::successors(parent(pid), |&p| parent(p))
        .take_while(|&p| p > 1)
        .any(|p| p == ancestor)
}

#[cfg(not(target_os = "linux"))]
pub fn descends_from(_: u32, _: u32) -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(super) fn parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

/// The processes whose exe file is `name`, read from the system's process
/// list; tasklist answers only when that list cannot be taken.
#[cfg(windows)]
pub(crate) fn named(name: &str) -> Vec<GameProcess> {
    win32::process_ids(name)
        .unwrap_or_else(|| tasklist(name))
        .into_iter()
        .map(|pid| GameProcess {
            pid,
            started: None,
            origin: Origin::default(),
        })
        .collect()
}

#[cfg(windows)]
pub(super) fn tasklist(name: &str) -> Vec<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let filter = format!("IMAGENAME eq {name}");
    let Ok(out) = Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split("\",\"").map(|c| c.trim_matches('"'));
            cols.next()?.eq_ignore_ascii_case(name).then_some(())?;
            cols.next()?.parse().ok()
        })
        .collect()
}

#[cfg(windows)]
pub(super) mod win32 {
    use std::ffi::c_void;

    const TH32CS_SNAPPROCESS: u32 = 0x2;
    const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;
    const MAX_PATH: usize = 260;

    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_process_id: u32,
        priority: i32,
        flags: u32,
        exe_file: [u16; MAX_PATH],
    }

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
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
        fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    /// The full path of the exe the process `pid` runs, or None when it cannot be opened.
    pub fn image_path(pid: u32) -> Option<std::path::PathBuf> {
        // SAFETY: the handle is checked before use and closed once; the buffer is `size` u16s long and
        // QueryFullProcessImageNameW sets `size` to the length it wrote.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 32_768];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0;
            CloseHandle(handle);
            ok.then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])))
        }
    }

    /// The pids of the processes whose exe file name is `name` in any case,
    /// or None when the process list could not be taken.
    pub fn process_ids(name: &str) -> Option<Vec<u32>> {
        // SAFETY: the snapshot handle is checked before use and closed once;
        // the entry is zeroed with its size set, as Process32FirstW requires,
        // and exe_file is read only up to its NUL inside the array.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
                return None;
            }
            let mut entry: ProcessEntry32W = std::mem::zeroed();
            entry.size = std::mem::size_of::<ProcessEntry32W>() as u32;
            let mut out = Vec::new();
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                let len = entry.exe_file.iter().position(|&c| c == 0).unwrap_or(MAX_PATH);
                if entry.threads > 0
                    && String::from_utf16_lossy(&entry.exe_file[..len]).eq_ignore_ascii_case(name)
                {
                    out.push(entry.process_id);
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            Some(out)
        }
    }
}

#[cfg(windows)]
pub(crate) fn named_paths(name: &str) -> Vec<PathBuf> {
    win32::process_ids(name)
        .unwrap_or_default()
        .into_iter()
        .filter_map(win32::image_path)
        .collect()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn named(_: &str) -> Vec<GameProcess> {
    Vec::new()
}
