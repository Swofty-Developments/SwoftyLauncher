use std::path::Path;

#[cfg(target_os = "linux")]
pub(crate) fn set_affinity(cpus: &[usize]) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for &c in cpus {
            libc::CPU_SET(c, &mut set);
        }
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn set_affinity(_: &[usize]) {}

#[cfg(target_os = "linux")]
pub(crate) fn become_subreaper() {
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn become_subreaper() {}

#[cfg(unix)]
pub(crate) fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).ok();
}

#[cfg(not(unix))]
pub(crate) fn link(_: &Path, _: &Path) {}
