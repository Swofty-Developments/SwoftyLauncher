//! Gathering a bundle's files: the data folder's settings and logs, the running game's logs and reports, the
//! kept sessions and the crash reports, each read within the bundle's limits.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::crashes::{CrashFound, dumps_listing, is_crash_file};
use super::files::{Cut, file_name, kept_head, read_cut, same_contents};
use super::redact::{redact_env, redact_json};
use super::summary::{loader_line, newest_perf};
use super::system::os_description;
use super::{
    Bundle, CAREER_EDITOR, CRASHES_SENT, DUMP_CAP, Entry, LOADER_DIR, Limits, MIN_TAIL, NET_TRACE,
    NET_TRACE_CAP, PRINTS_SENT, Report, SESSION_DEPTH, SESSIONS, file_safe, now_secs, size, utc_stamp,
};
use crate::session::{LOADER_FALLBACK, LOADER_LOG, PERF_FALLBACK, PERF_LOG};
use crate::store::Store;

/// What a bundle is read from: the data folder, the game folder the report names, and the newest sessions
/// the data folder kept.
pub(super) struct Sources<'a> {
    pub(super) store: &'a Store,
    pub(super) game: Option<&'a Path>,
    pub(super) sessions: Vec<PathBuf>,
}

impl<'a> Sources<'a> {
    pub(super) fn new(store: &'a Store, game: Option<&'a Path>) -> Self {
        Self {
            store,
            game,
            sessions: newest_sessions(store, SESSIONS),
        }
    }

    /// The `.log` files in `state/`, the launch logs first.
    fn state_logs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(self.store.root.join("state"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| x == "log")
                    && std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
            })
            .collect();
        out.sort_by_key(|p| {
            let name = file_name(p);
            (
                !matches!(name.as_str(), "launcher-run.log" | "wrapper-run.log"),
                name,
            )
        });
        out
    }

    fn helper_logs(&self) -> Vec<(String, PathBuf)> {
        let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(self.store.root.join("cache/addons"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    e.path().join(crate::addons::helper::LOG_FILE),
                )
            })
            .filter(|(_, log)| log.is_file())
            .collect();
        out.sort();
        out
    }

    /// The newest `.log` files addons keep in `addon-data/<id>/logs/`, such as
    /// the Simplified Importer's converter logs.
    fn addon_logs(&self) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(self.store.root.join("addon-data"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let id = e.file_name().to_string_lossy().into_owned();
            let mut logs: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(e.path().join("logs"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|f| f.path())
                .filter(|p| p.extension().is_some_and(|x| x == "log") && p.is_file())
                .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
                .collect();
            logs.sort_by_key(|l| std::cmp::Reverse(l.0));
            out.extend(logs.into_iter().take(ADDON_LOGS).map(|(_, p)| (id.clone(), p)));
        }
        out.sort();
        out
    }
}

struct Collector {
    limits: Limits,
    used: u64,
    entries: Vec<Entry>,
    skipped: Vec<String>,
}

impl Collector {
    fn budget(&self) -> u64 {
        self.limits.total.saturating_sub(self.used)
    }

    fn push(&mut self, path: String, data: Vec<u8>, note: Option<String>) {
        self.used += data.len() as u64;
        self.entries.push(Entry { path, data, note });
    }

    /// A whole binary file (the minidump), up to `cap` and the remaining
    /// budget; too big to fit means it is left out with a note, never cut.
    fn binary(&mut self, path: String, from: &Path, cap: u64) {
        let len = match std::fs::metadata(from) {
            Ok(m) => m.len(),
            Err(e) => return self.skipped.push(format!("{path}: {e}")),
        };
        let room = cap.min(self.budget());
        if len > room {
            return self.skipped.push(format!(
                "{path} ({}): too big to send (limit {}), left out",
                size(len),
                size(room)
            ));
        }
        match std::fs::read(from) {
            Ok(data) => self.push(path, data, Some(size(len))),
            Err(e) => self.skipped.push(format!("{path}: {e}")),
        }
    }

    fn ea_diagnostics(&mut self) {
        if !cfg!(windows) {
            return;
        }
        let mut taken = Vec::new();
        for log in crate::diagnostics::ea_launch::newest_logs(
            &crate::diagnostics::ea_launch::log_dirs(),
            crate::diagnostics::ea_launch::LOG_FILES,
        ) {
            let name = crate::diagnostics::ea_launch::log_entry_name(&log, &mut taken);
            self.file_capped(
                format!("ea-app/{name}"),
                &log,
                None,
                crate::diagnostics::ea_launch::LOG_TAIL,
            );
        }
        match crate::diagnostics::ea_launch::application_events() {
            Ok(text) => self.push("windows/application-events.txt".into(), text.into_bytes(), None),
            Err(e) => self.skipped.push(format!("windows/application-events.txt: {e}")),
        }
    }

    fn file(&mut self, path: String, from: &Path, redact: Option<fn(&str) -> String>) {
        self.file_capped(path, from, redact, self.limits.file);
    }

    fn file_capped(&mut self, path: String, from: &Path, redact: Option<fn(&str) -> String>, file_cap: u64) {
        let cap = file_cap.min(self.budget());
        let len = match std::fs::metadata(from) {
            Ok(m) => m.len(),
            Err(e) => return self.skipped.push(format!("{path}: {e}")),
        };
        if cap < len && cap < MIN_TAIL.min(self.limits.file) {
            return self.skipped.push(format!(
                "{path} ({}): the bundle reached its {} limit",
                size(len),
                size(self.limits.total)
            ));
        }
        let head = kept_head(from, cap);
        match read_cut(from, cap, head) {
            Ok((data, cut)) => {
                let redacted = redact
                    .map(|f| f(&String::from_utf8_lossy(&data)))
                    .filter(|r| r.as_bytes() != data.as_slice());
                let changed = redacted.is_some();
                let mut out = Vec::new();
                let kept = match cut {
                    Cut::None => None,
                    Cut::Tail => {
                        out.extend_from_slice(
                            format!("[swofty: kept the last {} of {}]\n", size(cap), size(len)).as_bytes(),
                        );
                        Some(format!("last {} of {}", size(cap), size(len)))
                    }
                    Cut::Middle { head, tail } => Some(format!(
                        "first {} and last {} of {}",
                        size(head),
                        size(tail),
                        size(len)
                    )),
                };
                let note = match (kept, changed) {
                    (Some(kept), true) => Some(format!("{kept}, secrets redacted")),
                    (Some(kept), false) => Some(kept),
                    (None, true) => Some("secrets redacted".into()),
                    (None, false) => None,
                };
                out.extend_from_slice(redacted.as_ref().map_or(data.as_slice(), |r| r.as_bytes()));
                self.push(path, out, note);
            }
            Err(e) => self.skipped.push(format!("{path}: {e}")),
        }
    }
}

pub(super) const STALLS_SENT: usize = 2;

/// The newest load-stall reports (`stall-<time>.txt`, a sortable local stamp) the core left in `dir`, newest first.
pub(super) fn newest_stalls(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| crate::diagnostics::crash_report::is_stall_name(&file_name(p)) && p.is_file())
        .collect();
    found.sort_by_key(|p| std::cmp::Reverse(file_name(p)));
    found.truncate(n);
    found
}

pub fn newest_sessions(store: &Store, n: usize) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(store.sessions_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| file_name(p).starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(file_name(p)));
    dirs.truncate(n);
    dirs
}

pub(super) fn files_under(dir: &Path, depth: usize) -> Vec<(String, PathBuf)> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut items: Vec<_> = rd.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let Ok(t) = e.file_type() else { continue };
            let path = e.path();
            if t.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((rel, path));
            } else if t.is_dir() && depth > 1 {
                walk(root, &path, depth - 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, depth, &mut out);
    out
}

/// Converter and other addon logs sent per addon, newest first.
pub(super) const ADDON_LOGS: usize = 5;

impl Bundle {
    pub fn collect(store: &Store, report: &Report) -> Self {
        Self::collect_with(store, report, Limits::default(), &utc_stamp(now_secs()))
    }

    pub fn collect_with(store: &Store, report: &Report, limits: Limits, stamp: &str) -> Self {
        let mut c = Collector {
            limits,
            used: 0,
            entries: Vec::new(),
            skipped: Vec::new(),
        };
        if let Some(account) = crate::online::server::sign_in::load(&store.root) {
            crate::online::multiplayer::client::know_secret(&account.token);
        }
        match std::fs::read(store.config_path()) {
            Ok(data) => {
                let (text, note) = match serde_json::from_slice::<Value>(&data) {
                    Ok(mut v) => {
                        let n = redact_json(&mut v);
                        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                        (text, (n > 0).then(|| "secrets redacted".to_owned()))
                    }
                    Err(e) => (
                        format!("config.json is not valid JSON ({e}); its contents were left out"),
                        Some("unreadable, left out".to_owned()),
                    ),
                };
                c.push("config.json".into(), text.into_bytes(), note);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => c.skipped.push("config.json: none".into()),
            Err(e) => c.skipped.push(format!("config.json: {e}")),
        }
        let from = Sources::new(store, report.game.dir.as_deref());
        let addons = serde_json::to_string_pretty(&from.addons_json()).unwrap_or_default();
        c.push("addons.json".into(), addons.into_bytes(), None);
        let conflicts = crate::profile::conflicts::path(store, &store.active_profile());
        if conflicts.is_file() {
            c.file("conflicts.json".into(), &conflicts, None);
        }
        if let Ok(p) = store.profile(&store.active_profile()) {
            for id in p
                .enabled()
                .filter(|id| crate::dbedit::package::is_package(store, id))
            {
                let source = store.addons_dir().join(id).join(crate::dbedit::package::MARKER);
                c.file(format!("db-packages/{id}.json"), &source, None);
            }
        }
        let sessions = &from.sessions;
        let mut loader = Vec::new();
        let mut live_perf = None;
        if let Some(game) = &report.game.dir {
            for (name, rel) in [("loader.log", LOADER_LOG), ("perf.log", PERF_LOG)] {
                let live = game.join(rel);
                if !live.is_file()
                    || sessions
                        .first()
                        .is_some_and(|d| same_contents(&live, &d.join(name)))
                {
                    continue;
                }
                c.file(format!("game/{name}"), &live, None);
                if name == "perf.log" {
                    live_perf = Some((format!("game/{name}"), live));
                } else {
                    loader.extend(loader_line(&format!("game/{name}"), &live));
                }
            }
        }
        for name in crate::stdlib::reports::SENT {
            if let Some(file) = crate::stdlib::reports::found(&store.root, report.game.dir.as_deref(), name) {
                c.file(format!("game/registry/{name}"), &file, None);
            }
        }
        let careers = crate::stdlib::careers::data_path(&store.root);
        let careers = if careers.is_file() {
            Some(careers)
        } else {
            report
                .game
                .dir
                .as_ref()
                .map(|g| g.join(crate::stdlib::careers::GAME_PATH))
                .filter(|p| p.is_file())
        };
        if let Some(careers) = careers {
            c.file("game/careers/index.json".into(), &careers, None);
        }
        if let Some(game) = &report.game.dir {
            let status = game.join(crate::online::multiplayer::client::GAME_STATUS_FILE);
            if status.is_file() {
                c.file("game/netplay.json".into(), &status, None);
            }
            let trace = game.join(NET_TRACE);
            if trace.is_file() {
                c.file_capped("game/net-trace.log".into(), &trace, None, NET_TRACE_CAP);
            }
            for stall in newest_stalls(&game.join(LOADER_DIR), STALLS_SENT) {
                c.file(format!("game/{}", file_name(&stall)), &stall, None);
            }
        }
        for stall in newest_stalls(
            &crate::diagnostics::crash_report::fallback_dir(&store.root),
            STALLS_SENT,
        ) {
            c.file(format!("state/{}", file_name(&stall)), &stall, None);
        }
        if let Some(room) = &report.room {
            let mut room = room.clone();
            redact_json(&mut room);
            let text = crate::online::multiplayer::client::scrub(
                &serde_json::to_string_pretty(&room).unwrap_or_default(),
            );
            c.push("multiplayer/session.json".into(), text.into_bytes(), None);
        }
        let fallback = store.root.join(LOADER_FALLBACK);
        if !sessions
            .first()
            .is_some_and(|d| same_contents(&fallback, &d.join("loader.log")))
        {
            loader.extend(loader_line(LOADER_FALLBACK, &fallback));
        }
        let perf_fallback = store.root.join(PERF_FALLBACK);
        if live_perf.is_none()
            && perf_fallback.is_file()
            && !sessions
                .first()
                .is_some_and(|d| same_contents(&perf_fallback, &d.join("perf.log")))
        {
            live_perf = Some((PERF_FALLBACK.to_owned(), perf_fallback));
        }
        for (id, log) in from.helper_logs() {
            c.file(
                format!("addons/{id}/{}", crate::addons::helper::LOG_FILE),
                &log,
                None,
            );
        }
        for (id, log) in from.addon_logs() {
            c.file(format!("addons/{id}/logs/{}", file_name(&log)), &log, None);
        }
        if let Some(snapshot) = report
            .game
            .dir
            .as_ref()
            .and_then(|d| crate::addons::storage::newest(&store.root, d, CAREER_EDITOR, "snapshot.json"))
        {
            c.file("game/career-editor/snapshot.json".into(), &snapshot, None);
        }
        for log in from.state_logs() {
            let name = file_name(&log);
            let redact = name
                .starts_with("multiplayer")
                .then_some(crate::online::multiplayer::client::scrub as fn(&str) -> String);
            c.file(format!("state/{name}"), &log, redact);
        }
        for print in crate::online::multiplayer::client::newest_prints(&store.root, PRINTS_SENT) {
            c.file(
                format!(
                    "{}/{}",
                    crate::online::multiplayer::client::PRINTS_DIR,
                    file_name(&print)
                ),
                &print,
                Some(crate::online::multiplayer::client::scrub_json as fn(&str) -> String),
            );
        }
        c.ea_diagnostics();
        let not_stock = store.root.join("state/not-stock.json");
        if not_stock.is_file() {
            c.file("state/not-stock.json".into(), &not_stock, None);
        }
        let blocked = store.root.join("state/blocked.json");
        if blocked.is_file() {
            c.file("state/blocked.json".into(), &blocked, None);
        }
        for dir in sessions {
            let session = file_name(dir);
            loader.extend(loader_line(
                &format!("sessions/{session}"),
                &dir.join("loader.log"),
            ));
            for (rel, file) in files_under(dir, SESSION_DEPTH) {
                // The crash report and its minidump are collected whole below, the dump as binary;
                // the generic walk reads files as redacted text tails, which would corrupt the dump.
                if is_crash_file(&file) {
                    continue;
                }
                let redact = (rel == "env.txt").then_some(redact_env as fn(&str) -> String);
                c.file(format!("sessions/{session}/{rel}"), &file, redact);
            }
        }
        let crashes = from.crashes(CRASHES_SENT);
        let crash = crashes.first().cloned();
        let mut crash_report = None;
        for (i, c0) in crashes.iter().enumerate() {
            let name = file_name(&c0.report);
            let dir = if i == 0 { "crash" } else { "crash/earlier" };
            c.file(format!("{dir}/{name}"), &c0.report, None);
            if i == 0 {
                if let Some(dump) = &c0.dump {
                    c.binary(format!("crash/{}", file_name(dump)), dump, DUMP_CAP);
                }
                crash_report = std::fs::read_to_string(&c0.report).ok();
            }
        }
        if let Some(listing) = dumps_listing(&crashes) {
            c.push("crash/dumps.txt".into(), listing.into_bytes(), None);
        }
        let found = match (crash_report.as_deref(), crash.is_none()) {
            (Some(text), _) => CrashFound::Report(text),
            (None, true) => from.logged_report().map_or_else(
                || {
                    let lines = sessions
                        .first()
                        .map(|d| crate::diagnostics::fast_fail::session_lines(d))
                        .unwrap_or_default();
                    if lines.is_empty() {
                        CrashFound::None
                    } else {
                        CrashFound::FastFail(lines)
                    }
                },
                CrashFound::Unsaved,
            ),
            (None, false) => CrashFound::None,
        };
        let os = os_description();
        let mut bundle = Self {
            stem: format!("swofty-logs-{}-{stamp}", file_safe(&report.name)),
            name: report.name.clone(),
            message: report.message.clone(),
            launcher: crate::updates::release::current_version().to_string(),
            os,
            game: report.game.describe(),
            entries: c.entries,
            skipped: c.skipped,
        };
        let perf = newest_perf(live_perf, sessions);
        let summary = bundle.summary(&from, report, stamp, perf, found, &loader);
        bundle.entries.insert(
            0,
            Entry {
                path: "summary.txt".into(),
                data: summary.into_bytes(),
                note: None,
            },
        );
        bundle
    }
}
