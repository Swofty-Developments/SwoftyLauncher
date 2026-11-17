//! Why a session failed: crash reports and exit codes, launches that started nothing, loader files an
//! antivirus took, the crash causes triage keeps finding, and the Send Logs bundle.

pub mod crash;
pub mod crash_report;
pub mod ea_launch;
pub mod fast_fail;
pub mod known_problems;
pub mod launch;
pub mod overlay_guard;
pub mod phases;
pub mod quarantine;
pub mod send_logs;
pub mod session_end;
