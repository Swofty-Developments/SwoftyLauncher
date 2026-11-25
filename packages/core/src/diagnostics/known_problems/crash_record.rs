use super::evidence::Evidence;

/// The stack frames, nearest first, that count as the crash's own.
pub(super) const NEAR_FRAMES: usize = 8;

pub(super) fn report_name(loader: &str) -> Option<String> {
    crate::diagnostics::crash_report::logged_name(loader)
}

/// The fatal exception as the loader log and the crash report describe it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Fatal {
    /// The module the exception happened in, lower case.
    pub(super) module: Option<String>,
    /// The modules on the crashing thread's stack, nearest first, lower case.
    pub(super) stack: Vec<String>,
    /// The loader's record of the exception: registers, code and stack.
    pub(super) record: Vec<String>,
    /// The crashing thread's stack frames as written, nearest first.
    pub(super) frames: Vec<String>,
    /// Where the record starts among the loader's crash lines.
    pub(super) at: Option<usize>,
}

impl Fatal {
    pub(super) fn near(&self) -> impl Iterator<Item = &str> {
        self.module
            .iter()
            .chain(self.stack.iter().take(NEAR_FRAMES))
            .map(String::as_str)
    }
}

/// The module in `text` before its first `+0x`, as in `F1_25.exe+0x39182a4`.
pub(super) fn module_at(text: &str) -> Option<String> {
    let at = text.find("+0x")?;
    let name = text[..at].rsplit([' ', '\t']).next()?;
    (!name.is_empty() && !name.starts_with("0x")).then(|| name.to_ascii_lowercase())
}

/// The loader's crash lines, without their time and thread.
pub(super) fn crash_lines(loader: &str) -> Vec<&str> {
    loader
        .lines()
        .filter_map(|l| Some(&l[l.find("[crash] ")? + "[crash] ".len()..]))
        .collect()
}

/// The exception code in a first-chance line, as in `ACCESS_VIOLATION (0xc0000005)`.
pub(super) fn exception_code(line: &str) -> Option<u32> {
    line.match_indices("(0x").find_map(|(i, _)| {
        let hex = &line[i + 3..];
        u32::from_str_radix(&hex[..hex.find(')')?], 16).ok()
    })
}

pub(super) fn record_at<'a>(crash: &[&'a str], i: usize) -> impl Iterator<Item = &'a str> {
    crash[i + 1..].iter().copied().take_while(|l| l.starts_with("  "))
}

pub(super) fn fatal(e: &Evidence) -> Fatal {
    let mut f = Fatal::default();
    let crash = crash_lines(&e.loader);
    let unhandled = crash
        .iter()
        .find(|l| l.starts_with("unhandled ") && l.contains("reached the top-level filter"));
    f.module = unhandled.and_then(|l| module_at(&l[l.find(" at ")? + 4..]));
    let address = unhandled.and_then(|l| {
        let at = &l[l.find(" at ")? + 4..];
        Some(at.split_whitespace().next()?.to_owned())
    });
    f.at = match &address {
        Some(address) => crash
            .iter()
            .position(|l| l.contains("first-chance") && l.contains(address.as_str())),
        None if e.report.is_none() => crash
            .iter()
            .rposition(|l| l.contains("first-chance"))
            .filter(|&i| e.code.is_some() && exception_code(crash[i]) == e.code),
        None => None,
    };
    if let Some(i) = f.at {
        if f.module.is_none() {
            f.module = crash[i]
                .find(" at ")
                .and_then(|at| module_at(&crash[i][at + 4..]));
        }
        f.record = record_at(&crash, i).map(str::to_owned).collect();
        f.frames = f
            .record
            .iter()
            .filter(|l| l.trim_start().starts_with('#'))
            .cloned()
            .collect();
        f.stack = f.frames.iter().filter_map(|l| module_at(l)).collect();
    }
    if let Some(report) = &e.report {
        let mut section = "";
        let mut stack = Vec::new();
        let mut frames = Vec::new();
        for line in report.lines() {
            if let Some(name) = line.strip_prefix("== ") {
                section = name;
                continue;
            }
            if section == "Exception" && f.module.is_none() {
                if let Some(at) = line.find(" at ") {
                    f.module = module_at(&line[at + 4..]);
                }
            } else if section.starts_with("Stack (") {
                stack.extend(line.split_whitespace().nth(3).and_then(module_at));
                if line.trim_start().starts_with('#') {
                    frames.push(line.to_owned());
                }
            }
        }
        if f.stack.is_empty() {
            f.stack = stack;
        }
        if f.frames.is_empty() {
            f.frames = frames;
        }
    }
    f
}

/// The modules the loader and the offline play hook saw in the game process.
pub(super) fn loaded_modules(loader: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in loader.lines() {
        let Some(rest) = ["[offline] module: ", "[loader] module: "]
            .iter()
            .find_map(|tag| Some(&line[line.find(tag)? + tag.len()..]))
        else {
            continue;
        };
        let name = rest.split(" @").next().unwrap_or(rest).trim();
        if !name.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
        }
    }
    out
}

/// The loaded modules with the address they are loaded at, as in
/// `[offline] module: RTSSHooks64.dll @ 0x180000000`.
pub(super) fn module_bases(loader: &str) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    for line in loader.lines() {
        let Some(rest) = ["[offline] module: ", "[loader] module: "]
            .iter()
            .find_map(|tag| Some(&line[line.find(tag)? + tag.len()..]))
        else {
            continue;
        };
        let Some((name, base)) = rest.split_once(" @ 0x") else {
            continue;
        };
        let hex = base
            .split(|c: char| !c.is_ascii_hexdigit())
            .next()
            .unwrap_or_default();
        if let Ok(base) = u64::from_str_radix(hex, 16) {
            out.push((name.trim().to_ascii_lowercase(), base));
        }
    }
    out
}

/// The register values in the loader's record of the exception, as in
/// `rsi=000000018024056d rdi=00007ffd675e7776` and `r8 =7ffffffffffffffc`.
pub(super) fn record_registers(record: &[String]) -> Vec<u64> {
    record
        .iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split_whitespace())
        .filter_map(|t| {
            let (_, hex) = t.split_once('=')?;
            (hex.len() == 16).then(|| u64::from_str_radix(hex, 16).ok())?
        })
        .collect()
}

/// Whether the crash's lines (and the crash report) hold any of `codes`, the
/// HRESULTs the game hands its fatal abort, as in `rdi=00000000887a0005`.
pub(super) fn crash_holds(e: &Evidence, codes: &[&str]) -> bool {
    let hit = |l: &str| {
        let l = l.to_ascii_lowercase();
        codes.iter().any(|c| l.contains(c))
    };
    e.loader
        .lines()
        .filter(|l| l.contains("[crash]") || l.contains("[overlay] the GPU was removed"))
        .any(hit)
        || e.report.as_deref().is_some_and(|r| r.lines().any(hit))
}

/// Whether the game ran under Wine or Proton, where vkd3d reports its own
/// failures (a pipeline it can't build) with the same codes as a Windows
/// driver out of memory or lost.
pub(super) fn wine(e: &Evidence, loaded: &[String]) -> bool {
    e.report
        .as_deref()
        .is_some_and(|r| r.lines().any(|l| l.starts_with("runtime") && l.contains("Wine")))
        || loaded.iter().any(|m| {
            let m = m.to_ascii_lowercase();
            m.starts_with("winex11") || m.starts_with("winewayland") || m.starts_with("winevulkan")
        })
}

/// When a loader line was written, in seconds, from its `2026-10-27 15:00:08.518` start.
pub(super) fn stamp(line: &str) -> Option<f64> {
    let mut date = line.get(..10)?.split('-').map(|n| n.parse::<u32>().ok());
    let (y, m, d) = (date.next()??, date.next()??, date.next()??);
    let mut time = line.get(11..23)?.split([':', '.']).map(|n| n.parse::<u32>().ok());
    let (h, min, sec, ms) = (time.next()??, time.next()??, time.next()??, time.next()??);
    let day = f64::from((y * 12 + m) * 31 + d);
    Some(day * 86400.0 + f64::from(h * 3600 + min * 60 + sec) + f64::from(ms) / 1000.0)
}

/// The loader's crash lines as written, with their time.
pub(super) fn raw_crash_lines(loader: &str) -> Vec<&str> {
    loader.lines().filter(|l| l.contains("[crash] ")).collect()
}

/// When the game's fatal exception happened, from its first-chance line or
/// else the line where it reached the top-level filter.
pub(super) fn crash_time(e: &Evidence, f: &Fatal) -> Option<f64> {
    crash_line(e, f).and_then(stamp)
}

/// The loader line of the game's fatal exception ([`crash_time`]).
pub(super) fn crash_line<'a>(e: &'a Evidence, f: &Fatal) -> Option<&'a str> {
    f.at.and_then(|at| raw_crash_lines(&e.loader).get(at).copied())
        .or_else(|| {
            e.loader
                .lines()
                .find(|l| l.contains("[crash] unhandled ") && l.contains("reached the top-level filter"))
        })
}

/// The loader lines of the session the game crashed in: from its last session load start before the crash to the
/// crash line. A log with no session load keeps everything before the crash; one with no crash line keeps all of it.
pub(super) fn crashed_session<'a>(e: &'a Evidence, f: &Fatal) -> &'a str {
    let loader = e.loader.as_str();
    let end = crash_line(e, f).map_or(loader.len(), |l| l.as_ptr() as usize - loader.as_ptr() as usize);
    let before = &loader[..end];
    let start = before
        .match_indices(super::weekend::SESSION_LOAD)
        .filter(|(at, _)| {
            before[*at..]
                .lines()
                .next()
                .is_some_and(|l| l.trim_end().ends_with("-> Entering"))
        })
        .last()
        .map_or(0, |(at, _)| at);
    &before[start..]
}

/// The thread that wrote a loader line, from the `[  384]` after its time.
pub(super) fn thread(line: &str) -> Option<u32> {
    let rest = line.get(23..)?.trim_start().strip_prefix('[')?;
    rest.split_once(']')?.0.trim().parse().ok()
}

/// The game function a stack frame names, as `Sess_Loader_SetupTrackAssets`
/// in `#5  F1_25.exe+0x2d48bb7 .xtls Sess_Loader_SetupTrackAssets+0x47`. None
/// for a frame the symbols don't name (`fn base+…`) and for other modules.
pub(super) fn game_function(frame: &str) -> Option<&str> {
    let mut words = frame
        .split_whitespace()
        .skip_while(|w| !w.to_ascii_lowercase().starts_with("f1_25.exe+0x"))
        .skip(2);
    let name = words.next()?.split("+0x").next()?;
    (!name.is_empty() && name != "fn").then_some(name)
}
