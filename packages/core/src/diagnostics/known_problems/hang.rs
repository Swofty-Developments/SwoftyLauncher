use std::collections::BTreeMap;

use super::Diagnosis;
use super::crash_record::{Fatal, crash_lines, module_at, raw_crash_lines, record_at, stamp};
use super::evidence::Evidence;
use super::graphics::{DRIVERS, FRAME_GEN, FRAME_GEN_OFF, driver_steps, move_out, tools};
use super::stall::boot_stall;
use super::wording::{SEND_LOGS, name};

/// F1 25's hang watchdog limit in milliseconds, which it holds in r15 when it
/// closes the game.
pub(super) const WATCHDOG_MS: &str = "r15=0000000000007530";

/// How long before the watchdog fired the game had surely stopped moving on.
pub(super) const STALL_SECS: f64 = 28.0;

/// The earliest before the watchdog fired that the freeze can have started.
pub(super) const FREEZE_SECS: f64 = 36.0;

/// The least time between two log lines of one addon on one thread, across
/// the freeze, that counts as the addon keeping that thread busy.
pub(super) const BUSY_GAP_SECS: f64 = 20.0;

/// The lines an addon logs in the stalled time to count as the one busy.
pub(super) const BUSY_LINES: usize = 5;

/// Whether the game's own hang watchdog closed it: it writes to address 1
/// with its 30 s limit in r15.
pub(super) fn hang_watchdog(e: &Evidence, f: &Fatal) -> bool {
    let trap = |lines: &mut dyn Iterator<Item = &str>| {
        let (mut write, mut rax, mut limit) = (false, false, false);
        for l in lines {
            let t = l.trim_start();
            write |= t == "writing 0x1" || t.starts_with("writing 0x1:");
            rax |= t.starts_with("rax=0000000000000001 ");
            limit |= l.contains(WATCHDOG_MS);
        }
        write && rax && limit
    };
    let report = e.report.as_deref().is_some_and(|r| {
        let mut section = "";
        trap(&mut r.lines().filter(|l| {
            if let Some(name) = l.strip_prefix("== ") {
                section = name;
            }
            section == "Exception" || section == "Registers"
        }))
    });
    trap(&mut f.record.iter().map(String::as_str)) || report
}

/// The addon that kept a game thread busy while the game froze: one whose
/// consecutive lines on a thread are far apart across the stall, or the one
/// that logged the most in it.
pub(super) fn busy_addon(e: &Evidence, f: &Fatal, built_in: bool) -> Option<String> {
    let end = stamp(raw_crash_lines(&e.loader).get(f.at?)?)?;
    let ids: Vec<&String> = if e.enabled.is_empty() {
        e.names.keys().collect()
    } else {
        e.enabled.iter().collect()
    };
    let ids: Vec<&String> = ids
        .into_iter()
        .filter(|id| e.builtin.contains(*id) == built_in)
        .collect();
    let addon_of = |rest: &str| -> Option<&String> {
        let tag = rest.strip_prefix('[')?.split(']').next()?;
        if let Some((_, api)) = rest.split_once("] api: ") {
            let id = api.split(' ').next()?;
            return ids.iter().find(|i| i.as_str() == id).copied();
        }
        ids.iter()
            .find(|id| id.as_str() == tag || id.rsplit('.').next() == Some(tag))
            .copied()
    };
    let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
    let mut last: BTreeMap<&str, (f64, &String)> = BTreeMap::new();
    let mut gap = None;
    for line in e.loader.lines() {
        let Some(t) = stamp(line) else { continue };
        if t > end {
            break;
        }
        let Some((thread, rest)) = line.get(24..).and_then(|r| r.split_once("] ")) else {
            continue;
        };
        match addon_of(rest) {
            Some(a) => {
                if t >= end - STALL_SECS {
                    *counts.entry(a).or_default() += 1;
                    if last
                        .get(thread)
                        .is_some_and(|(at, b)| *b == a && t - at > BUSY_GAP_SECS && end - at <= FREEZE_SECS)
                    {
                        gap = Some(a);
                    }
                }
                last.insert(thread, (t, a));
            }
            None => {
                last.remove(thread);
            }
        }
    }
    gap.or_else(|| {
        counts
            .into_iter()
            .filter(|(_, n)| *n >= BUSY_LINES)
            .max_by_key(|(_, n)| *n)
            .map(|(a, _)| a)
    })
    .cloned()
}

/// A graphics driver error the game hit in the minute before it froze: the
/// module, its vendor and the frame generation on its stack.
pub(super) fn driver_error_before(
    e: &Evidence,
    f: &Fatal,
) -> Option<(String, &'static str, Option<&'static str>)> {
    let at = f.at?;
    let crash = crash_lines(&e.loader);
    let raw = raw_crash_lines(&e.loader);
    let end = stamp(raw.get(at)?)?;
    (0..at).rev().find_map(|i| {
        let l = crash[i];
        if !l.contains("first-chance") || stamp(raw[i]).is_none_or(|t| end - t > 2.0 * STALL_SECS) {
            return None;
        }
        let module = module_at(&l[l.find(" at ")? + 4..])?;
        let vendor = DRIVERS.iter().find(|(p, _)| module.starts_with(p))?.1;
        let frame_gen = record_at(&crash, i)
            .filter_map(module_at)
            .find_map(|m| FRAME_GEN.iter().find(|(p, _)| m.starts_with(p)).map(|(_, w)| *w));
        Some((module, vendor, frame_gen))
    })
}

pub(super) fn hang(e: &Evidence, f: &Fatal, mods: &[(&'static str, Vec<String>)]) -> Diagnosis {
    let mut what =
        "F1 25 stopped responding for 30 seconds, so its own watchdog closed it. It froze rather than \
                    crashed."
            .to_owned();
    let mut steps = Vec::new();
    if let Some(addon) = busy_addon(e, f, false) {
        let addon = name(e, &addon);
        what.push_str(&format!(" {addon} kept the game busy while it froze."));
        steps.push(format!("Turn off {addon} in your profile, then play again."));
    } else if let Some(addon) = busy_addon(e, f, true) {
        let addon = name(e, &addon);
        what.push_str(&format!(
            " The freeze happened inside the launcher's own code ({addon}, which is built in and can't be turned off)."
        ));
        steps.push(SEND_LOGS.to_owned());
    } else if let Some((addon, _)) = boot_stall(e) {
        what.push_str(&format!(
            " It never got past Loading... after Press any button, which {addon} is known to cause."
        ));
        steps.push(format!("Turn off {addon} in your profile, then play again."));
    } else if let Some((module, vendor, frame_gen)) = driver_error_before(e, f) {
        let during = frame_gen.map(|w| format!(" during {w}")).unwrap_or_default();
        what.push_str(&format!(
            " Just before, F1 25 hit an error in {module}, part of the {vendor} graphics driver{during}."
        ));
        if frame_gen.is_some() {
            steps.push(FRAME_GEN_OFF.to_owned());
        }
        steps.extend(
            driver_steps()
                .into_iter()
                .filter(|s| frame_gen.is_none() || !s.starts_with("Turn off frame generation")),
        );
    }
    if !mods.is_empty() {
        what.push_str(&format!(
            " Your game folder has {} installed, which hooks F1 25's graphics.",
            tools(mods)
        ));
        steps.insert(0, move_out(mods));
    }
    if steps.is_empty() {
        steps.push(
            "Press Play again. If F1 25 freezes at the same place again, turn off the addons you turned on last."
                .to_owned(),
        );
        steps.push(
            "Close other programs while you play, especially ones that record or draw over the screen."
                .to_owned(),
        );
    }
    Diagnosis {
        id: "hang_watchdog",
        title: "F1 25 froze and closed itself".to_owned(),
        what,
        steps,
    }
}
