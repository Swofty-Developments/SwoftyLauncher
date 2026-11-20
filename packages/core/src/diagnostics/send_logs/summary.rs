//! `summary.txt`: what the player said, the computer and the game, and a section per thing that can go wrong,
//! read from the files the bundle sends.

use std::path::{Path, PathBuf};

use super::collect::{Sources, files_under};
use super::crashes::{CrashFound, crash_excerpt};
use super::files::{file_name, modified, read_ends, read_head};
use super::system::{hardware, smoothness_lines, system_lines};
use super::{
    Bundle, LAUNCH_LOGS, LOADER_HEAD, OVERLAY_READ, PERF_LINES, PERF_READ, Report, has_session, perf,
    readable, size,
};
use crate::session::LOADER_LOG;

/// Whether a session's loader.log shows the loader running. One with only the
/// offline play stub's lines means version.dll never loaded in the game. A
/// loader the game folder refused logged to the data folder, and says why.
pub(super) fn loader_line(label: &str, log: &Path) -> Vec<String> {
    let Ok(head) = read_head(log, LOADER_HEAD) else {
        return Vec::new();
    };
    let head = String::from_utf8_lossy(&head);
    let state = match crate::diagnostics::quarantine::attached(&head) {
        Some(true) => "mod loader attached",
        Some(false) => "mod loader never attached (version.dll not loaded)",
        None => return Vec::new(),
    };
    let mut out = vec![format!("  {label:40} {state}")];
    out.extend(
        crate::diagnostics::quarantine::refused(&head)
            .map(|why| format!("  {:40} logged in the data folder: {why}", "")),
    );
    out
}

/// Which addons draw over the game, and what each session's `[overlay]` lines say.
impl Sources<'_> {
    pub(super) fn overlay_lines(&self, logs: &[(String, PathBuf)]) -> Vec<String> {
        let store = self.store;
        let mut out = Vec::new();
        let painters = crate::diagnostics::overlay_guard::painters(store, &store.active_profile());
        let names: Vec<String> = painters
            .iter()
            .map(|(id, name)| format!("{name} ({id})"))
            .collect();
        out.push(format!(
            "  Addons that draw over the game: {}",
            if names.is_empty() {
                "none on".to_owned()
            } else {
                names.join(", ")
            }
        ));
        for (label, log) in logs {
            let Ok(text) = read_ends(log, OVERLAY_READ, OVERLAY_READ) else {
                continue;
            };
            let state =
                crate::diagnostics::overlay_guard::SessionOverlay::scan(&String::from_utf8_lossy(&text))
                    .map_or_else(|| "no [overlay] lines".to_owned(), |o| o.describe());
            out.push(format!("  {label:40} {state}"));
        }
        out
    }
}

pub(super) const SAVES_LINES: usize = 60;

/// The `[saves]` lines that say where the game's profile save came from and which career slots
/// and containers it held, minus the per-call sandbox chatter.
pub(super) fn saves_excerpt(text: &str, cap: usize) -> Vec<String> {
    text.lines()
        .filter(|l| {
            (l.contains("[saves]") && !l.contains("[saves] sandbox:"))
                || l.contains("saves: Steam remote folder")
        })
        .take(cap)
        .map(|l| format!("  {}", l.trim_end()))
        .collect()
}

pub(super) fn saves_lines(logs: &[(String, PathBuf)]) -> Vec<String> {
    let mut out = Vec::new();
    for (label, log) in logs {
        let Ok(head) = read_head(log, OVERLAY_READ) else {
            continue;
        };
        let lines = saves_excerpt(&String::from_utf8_lossy(&head), SAVES_LINES);
        if lines.is_empty() && label.starts_with("state/") {
            continue;
        }
        out.push(label.clone());
        if lines.is_empty() {
            out.push("  no [saves] lines".to_owned());
        }
        out.extend(lines);
    }
    if out.is_empty() {
        out.push("  No loader log was found".to_owned());
    }
    out
}

/// The performance report the summary reads: the newest session's copy, unless the running game's report (in
/// the game folder, or in the data folder when the game folder refuses it) is newer.
pub(super) fn newest_perf(
    live: Option<(String, PathBuf)>,
    sessions: &[PathBuf],
) -> Option<(String, PathBuf)> {
    let kept = sessions.iter().map(|d| d.join("perf.log")).find(|p| p.is_file());
    match (live, kept) {
        (Some((_, live)), Some(kept)) if modified(&live) <= modified(&kept) => {
            Some((session_label(&kept), kept))
        }
        (Some(live), _) => Some(live),
        (None, Some(kept)) => Some((session_label(&kept), kept)),
        (None, None) => None,
    }
}

pub(super) fn session_label(file: &Path) -> String {
    let session = file.parent().map(file_name).unwrap_or_default();
    format!("sessions/{session}/{}", file_name(file))
}

/// The stock check's last verdict and the EA AntiCheat launcher files in the
/// game root, so a blocked player's bundle shows which file failed and what EA
/// rewrote.
impl Sources<'_> {
    pub(super) fn game_files_section(&self) -> String {
        let (store, game) = (self.store, self.game);
        let mut s = String::new();
        match crate::game_folder::stock::Report::read(&store.root) {
            Some(r) if !r.detail.is_empty() => {
                s.push_str(&format!("  last launch stopped: {}\n", r.title()));
                for f in &r.detail {
                    s.push_str(&format!(
                        "  {} (measured {}, size {}, Steam {}, modified {})\n",
                        f.rel,
                        f.measured,
                        f.size.map_or("missing".to_owned(), |n| n.to_string()),
                        f.steam,
                        f.modified.map_or("unknown".to_owned(), |t| format!("{t:.0}"))
                    ));
                }
            }
            Some(r) => s.push_str(&format!(
                "  last launch stopped: {} ({})\n",
                r.title(),
                r.files.join(", ")
            )),
            None => s.push_str("  No stock-check failure on record\n"),
        }
        let eac = game.map(eac_files).unwrap_or_default();
        if eac.is_empty() {
            s.push_str("  No EAAntiCheat.* files found in the game folder\n");
        }
        for line in eac {
            s.push_str(&format!("  {line}\n"));
        }
        if let Some(line) = game
            .and_then(crate::platform::run_as_admin::virtual_store)
            .and_then(|v| virtualised(&v))
        {
            s.push_str(&format!("  {line}\n"));
        }
        s
    }
}

/// The game files Windows virtualised for this user. A process it redirects
/// writes them to the VirtualStore, where F1 25 never reads them.
pub(super) fn virtualised(copy: &Path) -> Option<String> {
    const SHOWN: usize = 6;
    let files = files_under(copy, 6);
    let names: Vec<&str> = files.iter().take(SHOWN).map(|(rel, _)| rel.as_str()).collect();
    (!files.is_empty()).then(|| {
        format!(
            "Windows keeps {} file(s) written to the game folder in {}, where F1 25 doesn't read them: {}{}",
            files.len(),
            copy.display(),
            names.join(", "),
            if files.len() > SHOWN { ", ..." } else { "" }
        )
    })
}

pub(super) fn eac_files(game: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(game)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.to_ascii_lowercase().starts_with("eaanticheat.") {
                return None;
            }
            let meta = e.metadata().ok();
            let size = meta.as_ref().map_or("?".to_owned(), |m| m.len().to_string());
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or("?".to_owned(), |d| d.as_secs().to_string());
            Some(format!("{name} (size {size}, modified {modified})"))
        })
        .collect();
    out.sort();
    out
}

impl Bundle {
    pub(super) fn stub_blocked_line(&self, game: Option<&Path>) -> Option<String> {
        let logs: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.path.ends_with(crate::diagnostics::ea_launch::PROXY_LOG))
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .collect();
        let block = crate::offline::stub_blocked(game.unwrap_or(Path::new("")), &logs, 0, None)?;
        Some(format!(
            "offline: {} was blocked by antivirus (error {}); {}",
            block.name(),
            block.code,
            "the EA app log shows the virus error on createProcess"
        ))
    }

    pub(super) fn summary(
        &self,
        from: &Sources,
        report: &Report,
        stamp: &str,
        perf: Option<(String, PathBuf)>,
        crash: CrashFound,
        loader: &[String],
    ) -> String {
        let store = from.store;
        let mut s = String::new();
        let mut line = |k: &str, v: &str| s.push_str(&format!("{k:16} {v}\n"));
        line("Discord username", &self.name);
        for (k, v) in report.automatic.iter().flat_map(|a| a.summary_lines()) {
            line(k, &v);
        }
        if let Some(steps) = report.room.as_ref().and_then(super::automatic::steps_line) {
            line("Room steps", &steps);
        }
        line("Collected", &readable(stamp));
        line("Launcher", &self.launcher);
        line("OS", &self.os);
        line("Hardware", &hardware());
        if cfg!(windows) {
            line(
                "Administrator",
                if crate::platform::run_as_admin::elevated() {
                    "yes"
                } else {
                    "no"
                },
            );
        }
        line("F1 25", &self.game);
        if let Some(dir) = &report.game.dir {
            line("Game folder", &dir.display().to_string());
        }
        line("Data folder", &store.root.display().to_string());
        if !has_session(store) {
            line(
                "Game sessions",
                "none found: no launch log in state/ and no session in sessions/",
            );
        }
        s.push_str("\nWhat went wrong\n");
        s.push_str(if self.message.is_empty() {
            "(not said)"
        } else {
            &self.message
        });
        if let Some(ids) = crate::diagnostics::known_problems::PackageIds::read(&store.root) {
            let installed = store
                .addons()
                .map(|a| a.into_keys().collect())
                .unwrap_or_default();
            s.push_str(&format!("\n\nCar package ids\n  {}\n", ids.summary(&installed)));
            if let Some(d) = ids.diagnosis(&installed, &std::collections::BTreeMap::new()) {
                s.push_str(&format!("  found: {}\n", d.text()));
            }
        }
        s.push_str("\n\nMod loader\n");
        if let Some(r) = crate::diagnostics::quarantine::Report::read(&store.root) {
            s.push_str(&format!(
                "  last launch: {} ({})\n",
                r.title(),
                r.files.join(", ")
            ));
            if let Some(c) = &r.cause {
                s.push_str(&format!(
                    "  found: {} (the [loader] lines in state/wrapper-run.log say more)\n",
                    c.what()
                ));
            }
        }
        if let Some(f) = crate::diagnostics::launch::Failure::read(&store.root) {
            s.push_str(&format!("  last launch: {} ({})\n", f.title(), f.headline()));
            if let crate::diagnostics::launch::Cause::Blocked(b) = &f.cause {
                s.push_str(&format!(
                    "  blocked: {} (os error {})\n",
                    b.program.display(),
                    b.code
                ));
            }
            if let crate::diagnostics::launch::Cause::RunAsAdmin(r) = &f.cause {
                s.push_str(&format!(
                    "  set to run as administrator (os error 740): {}\n",
                    r.paths().join(", ")
                ));
            }
        }
        if loader.is_empty() {
            s.push_str("  No loader log was found\n");
        }
        for l in loader {
            s.push_str(l);
            s.push('\n');
        }
        s.push_str("\nOverlay\n");
        let mut logs = Vec::new();
        if let Some(game) = &report.game.dir
            && self.entries.iter().any(|e| e.path == "game/loader.log")
        {
            logs.push(("game/loader.log".to_owned(), game.join(LOADER_LOG)));
        }
        for dir in &from.sessions {
            logs.push((format!("sessions/{}", file_name(dir)), dir.join("loader.log")));
        }
        for l in from.overlay_lines(&logs) {
            s.push_str(&l);
            s.push('\n');
        }
        s.push_str("\nGraphics and memory\n");
        for l in system_lines(&logs) {
            s.push_str(&l);
            s.push('\n');
        }
        s.push_str("\nSmoothness\n");
        for l in smoothness_lines(&logs) {
            s.push_str(&l);
            s.push('\n');
        }
        s.push_str("\nCrash\n");
        match crash {
            CrashFound::Report(text) => {
                for l in crash_excerpt(text) {
                    s.push_str(&l);
                    s.push('\n');
                }
            }
            CrashFound::Unsaved(path) => s.push_str(&format!(
                "A crash report was started ({path}) but could not be saved, so none was found\n"
            )),
            CrashFound::FastFail(lines) => {
                s.push_str("No crash report was found (the game did not write one this session)\n");
                for l in lines {
                    s.push_str(&l);
                    s.push('\n');
                }
            }
            CrashFound::None => {
                s.push_str("No crash report was found (the game did not write one this session)\n")
            }
        }
        s.push_str("\nSaves\n");
        let mut sources = logs.clone();
        for name in LAUNCH_LOGS {
            sources.push((name.to_owned(), store.root.join(name)));
        }
        for l in saves_lines(&sources) {
            s.push_str(&l);
            s.push('\n');
        }
        s.push_str("\nGame files against Steam\n");
        s.push_str(&from.game_files_section());
        if let Some(line) = self.stub_blocked_line(report.game.dir.as_deref()) {
            s.push_str(&format!("  {line}\n"));
        }
        s.push_str("\nPerformance\n");
        match perf {
            Some((label, path)) => {
                let text = read_head(&path, PERF_READ).map(|d| String::from_utf8_lossy(&d).into_owned());
                match text {
                    Ok(text) => {
                        s.push_str(&format!("From {label}\n"));
                        for l in perf::excerpt(&text, PERF_LINES) {
                            s.push_str(&l);
                            s.push('\n');
                        }
                    }
                    Err(e) => s.push_str(&format!("{label} could not be read: {e}\n")),
                }
            }
            None => s.push_str(
                "No performance report was found (an older launcher, or the game never reached the menus)\n",
            ),
        }
        let slow = logs.iter().find_map(|(label, log)| {
            let head = read_head(log, OVERLAY_READ).ok()?;
            let d = crate::diagnostics::known_problems::slow_start(&String::from_utf8_lossy(&head), store)?;
            Some((label, d))
        });
        if let Some((label, d)) = slow {
            s.push_str(&format!("  start: {} ({label})\n", d.title));
        }
        s.push_str("\nAddons\n");
        s.push_str(&from.addon_lines());
        s.push_str("\nFiles\n");
        for e in &self.entries {
            s.push_str(&format!("  {:60} {:>9}", e.path, size(e.data.len() as u64)));
            if let Some(n) = &e.note {
                s.push_str(&format!("  ({n})"));
            }
            s.push('\n');
        }
        if !self.skipped.is_empty() {
            s.push_str("\nLeft out\n");
            for k in &self.skipped {
                s.push_str(&format!("  {k}\n"));
            }
        }
        s
    }
}
