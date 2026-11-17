use std::path::Path;
use std::time::Duration;

use swofty_core::session::STOP_FILE;

use crate::time::now_secs;

pub(crate) fn install_ctrl_c() {
    #[cfg(target_os = "linux")]
    unsafe {
        extern "C" fn on_int(_: libc::c_int) {
            STOP.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        libc::signal(libc::SIGINT, on_int as *const () as libc::sighandler_t);
    }
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn stop_requested() -> bool {
    STOP.swap(false, std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn end_on_signals() {
    #[cfg(target_os = "linux")]
    unsafe {
        extern "C" fn on_end(_: libc::c_int) {
            ENDING.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(sig, on_end as *const () as libc::sighandler_t);
        }
    }
}

static ENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn ending() -> bool {
    ENDING.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) const STOP_GRACE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopPhase {
    Running,
    Closing,
    GiveUp,
}

pub(crate) fn stop_phase(requested: Option<f64>, now: f64, signalled: bool) -> StopPhase {
    match requested {
        Some(at) if now - at > STOP_GRACE.as_secs_f64() => StopPhase::GiveUp,
        Some(_) => StopPhase::Closing,
        None if signalled => StopPhase::Closing,
        None => StopPhase::Running,
    }
}

pub(crate) fn stop_requested_at(dir: &Path) -> Option<f64> {
    let file = dir.join(STOP_FILE);
    if !file.exists() {
        return None;
    }
    let at = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| s.trim().parse().ok());
    Some(at.unwrap_or_else(now_secs))
}

pub(crate) fn session_stop(dir: &Path) -> StopPhase {
    stop_phase(stop_requested_at(dir), now_secs(), ending())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_stop_request_closes_the_game_then_gives_up_waiting_for_it() {
        assert_eq!(stop_phase(None, 100.0, false), StopPhase::Running);
        assert_eq!(stop_phase(None, 100.0, true), StopPhase::Closing);
        assert_eq!(stop_phase(Some(90.0), 100.0, false), StopPhase::Closing);
        assert_eq!(stop_phase(Some(80.0), 100.0, false), StopPhase::Closing);
        assert_eq!(stop_phase(Some(79.0), 100.0, false), StopPhase::GiveUp);
        assert_eq!(stop_phase(Some(79.0), 100.0, true), StopPhase::GiveUp);
    }

    #[test]
    fn reads_when_the_launcher_asked_the_session_to_stop() {
        let dir = std::env::temp_dir().join(format!("swofty-stop-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(stop_requested_at(&dir), None);
        assert_eq!(session_stop(&dir), StopPhase::Running);
        std::fs::write(dir.join(STOP_FILE), "12.5\n").unwrap();
        assert_eq!(stop_requested_at(&dir), Some(12.5));
        assert_eq!(session_stop(&dir), StopPhase::GiveUp);
        std::fs::write(dir.join(STOP_FILE), "garbage").unwrap();
        assert_eq!(session_stop(&dir), StopPhase::Closing);
        std::fs::remove_dir_all(&dir).ok();
    }
}
