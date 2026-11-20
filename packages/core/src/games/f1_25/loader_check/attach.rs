use std::time::Duration;

use crate::GAME_EXE;
use crate::diagnostics::launch::WINDOW;

use super::{Attached, Proof};

/// The loader's last attach line in `log`, when the log is this launch's:
/// the line names one of the `running` games, or the log is `fresh`, begun
/// by a loader after the game started. An attach line an earlier launch left
/// names a game that is gone, in a log nothing moved aside.
pub fn attached(log: &str, running: &[u32], fresh: bool) -> Option<Attached> {
    let lines: Vec<&str> = log.lines().collect();
    let (at, pid) = lines.iter().enumerate().rev().find_map(|(i, l)| {
        if crate::diagnostics::quarantine::attached(l) != Some(true) {
            return None;
        }
        let rest = &l[l.find("attached, pid ")? + "attached, pid ".len()..];
        let pid = rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
        Some((i, pid))
    })?;
    let proof = if running.contains(&pid) {
        Proof::Running
    } else if fresh {
        Proof::Rotated
    } else {
        return None;
    };
    let start = stamp(lines[at]);
    let after = |l: &str| Some(Duration::from_millis(stamp(l)?.checked_sub(start?)?));
    let window = lines[at..].iter().find(|l| WINDOW.iter().any(|w| l.contains(w)));
    let last = lines[at + 1..].iter().rev().find(|l| !l.trim().is_empty());
    Some(Attached {
        pid,
        at: start.and(lines[at].get(11..23)).map(str::to_owned),
        window: window.is_some(),
        window_after: window.and_then(|l| after(l)),
        last: last.map(|l| body(l).chars().take(160).collect()),
        last_after: last.and_then(|l| after(l)),
        ran: None,
        proof,
    })
}

/// A loader.log line's `YYYY-MM-DD HH:MM:SS.mmm` stamp as milliseconds, for
/// the time between two lines; the stamps are in the PC's time zone.
fn stamp(line: &str) -> Option<u64> {
    let s = line.get(..23)?.as_bytes();
    let shape = b"dddd-dd-dd dd:dd:dd.ddd";
    if !s.iter().zip(shape).all(|(c, k)| match k {
        b'd' => c.is_ascii_digit(),
        k => c == k,
    }) {
        return None;
    }
    let n = |from: usize, to: usize| line[from..to].parse::<u64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days since 1 March of year 0, as in the civil-from-days algorithm.
    let y = if m <= 2 { y.checked_sub(1)? } else { y };
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    Some((((days * 24 + n(11, 13)?) * 60 + n(14, 16)?) * 60 + n(17, 19)?) * 1000 + n(20, 23)?)
}

/// A loader.log line without its stamp and thread id.
fn body(line: &str) -> &str {
    let line = line.trim_end();
    if stamp(line).is_none() {
        return line;
    }
    let rest = line[23..].trim_start();
    match rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((tid, text)) if tid.trim().chars().all(|c| c.is_ascii_digit()) => text.trim_start(),
        _ => rest,
    }
}

fn secs(d: Duration) -> String {
    match d.as_millis() {
        ms if ms < 10_000 => format!("{:.1} s", d.as_secs_f64()),
        _ => format!("{} s", d.as_secs()),
    }
}

/// The pid in the offline play stub's last "game created (pid N)" line.
pub fn created_pid(log: &str) -> Option<u32> {
    log.lines().rev().find_map(|l| {
        let rest = &l[l.find("game created (pid ")? + "game created (pid ".len()..];
        rest[..rest.find(')')?].trim().parse().ok()
    })
}

impl Attached {
    pub(super) fn line(&self) -> String {
        let mut text = format!(
            "loader.log: the loader attached{}, pid {} ({}), and {}",
            self.at.as_ref().map(|t| format!(" at {t}")).unwrap_or_default(),
            self.pid,
            match self.proof {
                Proof::Running => format!("a running {GAME_EXE}"),
                Proof::Rotated => "in the log it began this launch".to_owned(),
            },
            match (self.window, self.window_after) {
                (true, Some(d)) => format!("reached the window stage {} after it attached", secs(d)),
                (true, None) => "reached the window stage".to_owned(),
                (false, _) => "is waiting for the game window: no window stage line yet".to_owned(),
            }
        );
        if let Some(d) = self.ran {
            text.push_str(&format!("; the game was started {} ago", secs(d)));
        }
        if let Some(last) = &self.last {
            text.push_str(&format!(
                "; its last line{}: {last}",
                self.last_after
                    .map(|d| format!(", {} after it attached", secs(d)))
                    .unwrap_or_default()
            ));
        }
        text
    }
}
