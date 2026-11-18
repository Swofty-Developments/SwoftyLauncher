//! When a process started.

use std::time::SystemTime;
#[cfg(any(windows, test))]
use std::time::UNIX_EPOCH;

#[cfg(target_os = "linux")]
pub(crate) fn started(pid: u32) -> Option<SystemTime> {
    const TICKS_PER_SEC: f64 = 100.0;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let ticks: f64 = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let age = (uptime - ticks / TICKS_PER_SEC).max(0.0);
    SystemTime::now().checked_sub(std::time::Duration::from_secs_f64(age))
}

#[cfg(windows)]
pub(crate) fn started(pid: u32) -> Option<SystemTime> {
    use std::ffi::c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut u64,
            exit: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let [mut creation, mut exit, mut kernel, mut user] = [0u64; 4];
    // SAFETY: the handle is checked before use and closed once, and each
    // FILETIME the call fills is a u64, the same size with a wider alignment.
    let ok = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let ok = GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(process);
        ok
    };
    (ok != 0).then(|| filetime(creation)).flatten()
}

/// A FILETIME: 100 ns ticks since 1601-01-01 UTC.
#[cfg(any(windows, test))]
pub(super) fn filetime(ticks: u64) -> Option<SystemTime> {
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let since = ticks.checked_sub(UNIX_EPOCH_TICKS)?;
    UNIX_EPOCH.checked_add(std::time::Duration::from_micros(since / 10))
}
