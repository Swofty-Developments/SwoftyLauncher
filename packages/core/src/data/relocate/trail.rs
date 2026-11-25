use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

pub(super) const FILE: &str = "swofty-data-move.log";
const CAP: u64 = 64 << 10;
const KEEP: usize = 200;

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(super) fn note(default: &Path, text: &str) {
    if let Ok(mut lines) = LINES.lock() {
        if lines.len() >= KEEP {
            lines.remove(0);
        }
        lines.push(text.to_owned());
    }
    let path = default.with_file_name(FILE);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let full = std::fs::metadata(&path).is_ok_and(|m| m.len() > CAP);
    std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(&path)
        .and_then(|mut f| {
            writeln!(
                f,
                "{} [{}] {text}",
                crate::diagnostics::send_logs::utc_stamp(secs),
                std::process::id()
            )
        })
        .ok();
}

pub(super) fn timed<T, E: std::fmt::Display>(
    default: &Path,
    what: &str,
    run: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    note(default, &format!("data move: {what}"));
    let began = Instant::now();
    let result = run();
    let took = began.elapsed().as_secs_f32();
    match &result {
        Ok(_) => note(default, &format!("data move: {what} done in {took:.1} s")),
        Err(e) => note(
            default,
            &format!("data move: {what} failed after {took:.1} s: {e}"),
        ),
    }
    result
}

/// The lines recorded since startup, for the launcher's own log once it is safe to open.
pub fn take() -> Vec<String> {
    LINES
        .lock()
        .map(|mut l| std::mem::take(&mut *l))
        .unwrap_or_default()
}
