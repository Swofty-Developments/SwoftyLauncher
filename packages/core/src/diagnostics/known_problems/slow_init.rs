use crate::store::Store;

use super::Diagnosis;
use super::evidence::Evidence;
use super::stall::minutes;
use super::wording::{list, name};

/// What the core logs after an addon init that took over a second: the whole milliseconds, then the addon's id, or
/// the dll's file name when settings.json names no addon that owns it.
const SLOW_INIT: &str = "[perf] slow addon init ";

/// The init time, summed over an addon's passes, from which the start counts as held up.
pub(super) const SLOW_INIT_MS: u64 = 5000;

/// The core's own dlls, which the launcher installs and the player doesn't choose.
const CORE_DLLS: &[&str] = &["swofty_core.dll", "swofty_gamelog.dll"];

/// The folder under the game folder that native plugins are installed in, one folder per addon id.
const ADDONS_DIR: &str = "\\swofty\\addons\\";

/// What the loader logs on the thread that runs an addon's init, before and after it, from which the init time
/// of a core that logs no `[perf] slow addon init` line is read.
const INIT_CALLED: &str = ", calling SwoftyAddonInit";
const INIT_RETURNED: &str = ": SwoftyAddonInit returned ";

/// Each addon's init time in milliseconds, summed over its passes, in the order the log first names them: from
/// the `[perf] slow addon init` lines, or from the loader's own lines around each init on a core older than them.
pub(super) fn addon_inits(loader: &str) -> Vec<(String, u64)> {
    match loader.contains(SLOW_INIT) {
        true => slow_inits(loader),
        false => timed_inits(loader),
    }
}

/// Each name the `[perf] slow addon init` lines give, with its init time in milliseconds summed over its passes,
/// in the order the log first names them.
pub(super) fn slow_inits(loader: &str) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    for line in loader.lines() {
        let Some(at) = line.find(SLOW_INIT) else {
            continue;
        };
        let Some((ms, addon)) = line[at + SLOW_INIT.len()..].split_once(" ms: ") else {
            continue;
        };
        let (Ok(ms), addon) = (ms.parse::<u64>(), addon.trim_end_matches('\r')) else {
            continue;
        };
        if addon.is_empty() {
            continue;
        }
        match out.iter_mut().find(|(n, _)| n == addon) {
            Some((_, total)) => *total += ms,
            None => out.push((addon.to_owned(), ms)),
        }
    }
    out
}

/// A loader.log line's time of day in milliseconds, its thread and its message.
fn stamped(line: &str) -> Option<(u64, &str, &str)> {
    let clock = line.get(11..23)?;
    let mut parts = clock.split([':', '.']).map(|n| n.parse::<u64>().ok());
    let (h, m, s, ms) = (parts.next()??, parts.next()??, parts.next()??, parts.next()??);
    let rest = line.get(23..)?.trim_start().strip_prefix('[')?;
    let (thread, message) = rest.split_once(']')?;
    Some((
        ((h * 60 + m) * 60 + s) * 1000 + ms,
        thread.trim(),
        message.trim_start(),
    ))
}

/// Each dll's init time in milliseconds, summed over its passes, from the time between the loader's
/// `addon <dll>: loaded at <address>, calling SwoftyAddonInit` and `addon <dll>: SwoftyAddonInit returned` lines
/// on the same thread.
pub(super) fn timed_inits(loader: &str) -> Vec<(String, u64)> {
    const DAY_MS: u64 = 24 * 60 * 60 * 1000;
    let mut called: Vec<(String, String, u64)> = Vec::new();
    let mut out: Vec<(String, u64)> = Vec::new();
    for (at, thread, message) in loader.lines().filter_map(stamped) {
        let Some(rest) = message.strip_prefix("addon ") else {
            continue;
        };
        if rest.contains(INIT_CALLED)
            && let Some((dll, _)) = rest.split_once(": loaded at ")
        {
            called.retain(|(t, d, _)| !(t == thread && d == dll));
            called.push((thread.to_owned(), dll.to_owned(), at));
        } else if let Some((dll, _)) = rest.split_once(INIT_RETURNED)
            && let Some(i) = called.iter().position(|(t, d, _)| t == thread && d == dll)
        {
            let (_, dll, start) = called.remove(i);
            let ms = (at + DAY_MS - start) % DAY_MS;
            match out.iter_mut().find(|(n, _)| *n == dll) {
                Some((_, total)) => *total += ms,
                None => out.push((dll, ms)),
            }
        }
    }
    out
}

/// The addon id the session's loader names as the source of `dll`, from its `addon <dll>: loading <path>` line.
fn loaded_from(loader: &str, dll: &str) -> Option<String> {
    let needle = format!("addon {}: loading ", dll.to_ascii_lowercase());
    loader.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let path = &line[lower.find(&needle)? + needle.len()..];
        let path = path.replace('/', "\\");
        let at = path.to_ascii_lowercase().find(ADDONS_DIR)? + ADDONS_DIR.len();
        let id = path[at..].split('\\').next()?;
        (!id.is_empty()).then(|| id.to_owned())
    })
}

/// The name a slow init is shown with, or `None` for an init that is the launcher's own.
fn shown(e: &Evidence, logged: &str) -> Option<String> {
    let lower = logged.to_ascii_lowercase();
    if CORE_DLLS.contains(&lower.as_str()) {
        return None;
    }
    let id = match lower.ends_with(".dll") {
        true => e
            .plugins
            .get(&lower)
            .cloned()
            .or_else(|| loaded_from(&e.loader, logged)),
        false => Some(logged.to_owned()),
    };
    match id {
        Some(id) if e.builtin.contains(&id) => None,
        Some(id) => Some(name(e, &id)),
        None => Some(format!("the plugin {logged}")),
    }
}

/// The addons whose init held up F1 25's start by [`SLOW_INIT_MS`] or more, slowest first.
pub(super) fn slow_addons(e: &Evidence) -> Vec<(String, u64)> {
    let mut slow: Vec<(String, u64)> = addon_inits(&e.loader)
        .into_iter()
        .filter(|(_, ms)| *ms >= SLOW_INIT_MS)
        .filter_map(|(logged, ms)| Some((shown(e, &logged)?, ms)))
        .collect();
    slow.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    slow
}

/// The addon that held up F1 25's start the longest, when one took [`SLOW_INIT_MS`] or more to start up.
pub(crate) fn slow_addon_init(e: &Evidence) -> Option<Diagnosis> {
    let slow = slow_addons(e);
    let ((addon, ms), rest) = slow.split_first()?;
    let time = minutes(*ms as f64 / 1000.0);
    let others = match rest {
        [] => "The other addons in your profile started in moments.".to_owned(),
        [_] => format!("{} was also slow to start.", rest[0].0),
        _ => {
            let names: Vec<String> = rest.iter().map(|(n, _)| n.clone()).collect();
            format!("{} were also slow to start.", list(&names))
        }
    };
    Some(Diagnosis {
        id: "slow_addon_init",
        title: format!("{addon} held up F1 25's start by {time}"),
        what: format!(
            "{addon} took {time} to start up while F1 25 loaded, and F1 25 waited for it before it went on. {others}"
        ),
        steps: vec![
            format!(
                "If F1 25 takes too long to start, turn off {addon} in your profile and press Play again to compare."
            ),
            format!("Check for an update to {addon} on the Addons page."),
        ],
    })
}

/// The addon that held up the start of the session that wrote `loader`, as Send Logs reads it.
pub(crate) fn slow_start(loader: &str, store: &Store) -> Option<Diagnosis> {
    if !loader.contains(SLOW_INIT) && !loader.contains(INIT_CALLED) {
        return None;
    }
    let addons = store.addons().unwrap_or_default();
    let names = addons
        .iter()
        .map(|(id, m)| (id.clone(), m.name.clone()))
        .collect();
    let e = Evidence {
        loader: loader.to_owned(),
        names,
        ..Default::default()
    }
    .with_profile(store, &addons);
    slow_addon_init(&e)
}
