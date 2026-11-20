//! The computer: the OS, the CPU and memory, and the graphics lines the loader logs.

use std::path::PathBuf;

use super::OVERLAY_READ;
use super::files::{read_head, read_tail};
use crate::platform::registry::{self, Ask};

pub(super) const SYSTEM_LINES: usize = 24;

pub(super) const SYSTEM_TAIL: u64 = 256 << 10;

const SMOOTH_TAIL: u64 = 8 << 20;

/// Each log's [`perf::smoothness`](super::perf::smoothness) lines, from the end of the log.
pub(super) fn smoothness_lines(logs: &[(String, PathBuf)]) -> Vec<String> {
    let mut out = Vec::new();
    for (label, log) in logs {
        let Ok((tail, _)) = read_tail(log, SMOOTH_TAIL) else {
            continue;
        };
        out.push(label.clone());
        out.extend(super::perf::smoothness(&String::from_utf8_lossy(&tail)));
    }
    if out.is_empty() {
        out.push("  No loader log was found".to_owned());
    }
    out
}

/// The `[system]` lines core logs at boot (each GPU's name, driver and VRAM, RAM and commit), and the
/// newest per-minute memory line from the end of the log.
pub(super) fn system_excerpt(head: &str, tail: &str, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = head
        .lines()
        .filter(|l| l.contains("[system] "))
        .take(cap)
        .map(|l| format!("  {}", l.trim_end()))
        .collect();
    if let Some(last) = tail
        .lines()
        .rev()
        .find(|l| l.contains("[perf] ") && l.contains(" memory: "))
    {
        out.push(format!("  newest minute: {}", last.trim_end()));
    }
    out
}

pub(super) fn system_lines(logs: &[(String, PathBuf)]) -> Vec<String> {
    let mut out = Vec::new();
    for (label, log) in logs {
        let (Ok(head), Ok((tail, _))) = (read_head(log, OVERLAY_READ), read_tail(log, SYSTEM_TAIL)) else {
            continue;
        };
        let lines = system_excerpt(
            &String::from_utf8_lossy(&head),
            &String::from_utf8_lossy(&tail),
            SYSTEM_LINES,
        );
        out.push(label.clone());
        if lines.is_empty() {
            out.push(
                "  no [system] lines (a core older than this launcher, or the game never booted)".to_owned(),
            );
        }
        out.extend(lines);
    }
    if out.is_empty() {
        out.push("  No loader log was found".to_owned());
    }
    out
}

pub fn os_description() -> String {
    if cfg!(windows) {
        return windows_version();
    }
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |k: &str| {
        release
            .lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    };
    let distro = field("PRETTY_NAME")
        .or_else(|| field("NAME"))
        .unwrap_or_else(|| std::env::consts::OS.to_owned());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|k| k.trim().to_owned())
        .unwrap_or_default();
    let flatpak = std::path::Path::new("/.flatpak-info").exists();
    let mut s = format!("Linux · {distro}");
    if !kernel.is_empty() {
        s.push_str(&format!(" (kernel {kernel})"));
    }
    if flatpak {
        s.push_str(" · Flatpak");
    }
    s
}

pub(super) fn windows_version() -> String {
    let ver = crate::platform::run_as_admin::windows_tool("cmd", &["/C", "ver"]).unwrap_or_default();
    let number = ver
        .split_once("Version ")
        .map(|(_, v)| v.trim().trim_end_matches(']').trim().to_owned())
        .unwrap_or_default();
    let build: u32 = number.split('.').nth(2).and_then(|b| b.parse().ok()).unwrap_or(0);
    let display = registry::read(
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        Ask::default(),
    )
    .and_then(|k| k.text("DisplayVersion"))
    .map(|v| v.trim().to_owned())
    .filter(|v| !v.is_empty());
    let name = if build >= 22_000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    match (number.is_empty(), display) {
        (true, _) => "Windows".into(),
        (false, Some(d)) => format!("{name} {d} ({number})"),
        (false, None) => format!("{name} ({number})"),
    }
}

pub(super) fn hardware() -> String {
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let cpu = if cfg!(windows) {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    } else {
        std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|c| {
            c.lines().find_map(|l| {
                l.strip_prefix("model name")?
                    .split_once(':')
                    .map(|(_, v)| v.trim().to_owned())
            })
        })
    }
    .unwrap_or_else(|| "unknown CPU".into());
    let memory = windows_memory()
        .map(|(ram, commit)| memory_text(ram, Some(commit)))
        .or_else(|| {
            let kb: u64 = std::fs::read_to_string("/proc/meminfo")
                .ok()?
                .lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .ok()?;
            Some(memory_text(kb * 1024, None))
        });
    format!("{cpu} ({threads} threads){}", memory.unwrap_or_default())
}

/// `ram` and Windows' commit limit (`commit`, RAM plus the page file), in bytes.
pub(super) fn memory_text(ram: u64, commit: Option<u64>) -> String {
    let gb = |b: u64| format!("{:.0} GB", b as f64 / 1024. / 1024. / 1024.);
    match commit {
        Some(c) => format!(
            ", {} memory, {} page file ({} commit limit)",
            gb(ram),
            gb(c.saturating_sub(ram)),
            gb(c)
        ),
        None => format!(", {} memory", gb(ram)),
    }
}

/// The PC's RAM and commit limit in bytes.
#[cfg(windows)]
pub(super) fn windows_memory() -> Option<(u64, u64)> {
    #[repr(C)]
    #[derive(Default)]
    struct MemoryStatus {
        length: u32,
        load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    }
    let mut status = MemoryStatus {
        length: std::mem::size_of::<MemoryStatus>() as u32,
        ..MemoryStatus::default()
    };
    // SAFETY: `status` is a MEMORYSTATUSEX with its length set, and it
    // outlives the call.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    ok.then_some((status.total_phys, status.total_page_file))
}

#[cfg(not(windows))]
pub(super) fn windows_memory() -> Option<(u64, u64)> {
    None
}
