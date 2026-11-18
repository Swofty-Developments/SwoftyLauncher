use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use swofty_core::diagnostics::crash::{Event, LogScan, Symbols, fatal_name};
use swofty_core::session::{LogMark, SessionRecord};
use swofty_core::{GAME_EXE, STEAM_APP_ID};

use crate::console::output::say;

const KB: &str = include_str!("../../../../../re/db/function_names.tsv");

pub(crate) struct Follower {
    path: PathBuf,
    fallback: Option<PathBuf>,
    pos: u64,
    buf: String,
    stale: Option<Option<LogMark>>,
}

impl Follower {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            fallback: None,
            pos: 0,
            buf: String::new(),
            stale: None,
        }
    }

    pub(crate) fn from_end(path: PathBuf) -> Self {
        let pos = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self {
            pos,
            ..Self::new(path)
        }
    }

    fn open(&self) -> Option<std::fs::File> {
        std::fs::File::open(&self.path)
            .ok()
            .or_else(|| std::fs::File::open(self.fallback.as_ref()?).ok())
    }

    pub(crate) fn replacing(path: PathBuf, before: Option<LogMark>) -> Self {
        Self {
            stale: Some(before),
            ..Self::new(path)
        }
    }

    pub(crate) fn lines(&mut self) -> Vec<String> {
        if let Some(before) = &self.stale {
            if !LogMark::replaced(&self.path, before.as_ref()) {
                return Vec::new();
            }
            self.stale = None;
        }
        let Some(mut f) = self.open() else {
            return Vec::new();
        };
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        if size < self.pos {
            self.pos = 0;
        }
        let mut chunk = Vec::new();
        if f.seek(SeekFrom::Start(self.pos)).is_err() || f.read_to_end(&mut chunk).is_err() {
            return Vec::new();
        }
        self.pos += chunk.len() as u64;
        self.buf.push_str(&String::from_utf8_lossy(&chunk));
        let mut out: Vec<String> = self.buf.split('\n').map(str::to_owned).collect();
        self.buf = out.pop().unwrap_or_default();
        out
    }
}

pub(crate) struct Follow {
    dir: PathBuf,
    loader: Option<Follower>,
    wine: Follower,
    scan: LogScan,
    symbols: Symbols,
    pub(crate) last_loader: Option<String>,
}

impl Follow {
    pub(crate) fn new(dir: &Path, loader: Option<(PathBuf, Option<LogMark>)>, symbols: Symbols) -> Self {
        let loader = Some(match loader {
            Some((path, before)) => Follower {
                fallback: Some(dir.join("loader.log")),
                ..Follower::replacing(path, before)
            },
            None => Follower::new(dir.join("loader.log")),
        });
        let wine = Follower::new(dir.join(format!("steam-{STEAM_APP_ID}.log")));
        Self {
            dir: dir.into(),
            loader,
            wine,
            scan: LogScan::default(),
            symbols,
            last_loader: None,
        }
    }

    pub(crate) fn poll(&mut self) {
        for line in self.loader.as_mut().map(Follower::lines).unwrap_or_default() {
            if line.trim().is_empty() || line.contains("   section ") {
                continue;
            }
            let text = line.split_once("] ").map_or(line.as_str(), |(_, t)| t).to_owned();
            say(
                if text.starts_with("[core]") {
                    "core"
                } else {
                    "loader"
                },
                &text,
            );
            self.last_loader = Some(text);
        }
        for line in self.wine.lines() {
            match self.scan.feed(&line) {
                Some(Event::Fatal {
                    code,
                    addr,
                    first: true,
                }) => {
                    let name = fatal_name(code).unwrap_or("?");
                    say(
                        "crash",
                        &format!(
                            "{name} ({code:08x}) at {}",
                            self.scan.describe(addr, &self.symbols)
                        ),
                    );
                }
                Some(Event::Crash(text)) => say("crash", &text),
                Some(Event::Error(text)) => say("wine", &text.chars().take(240).collect::<String>()),
                _ => {}
            }
        }
    }

    pub(crate) fn summary(&self) -> Result<()> {
        if let Some(failure) = self.scan.start_failure {
            println!();
            say("info", &format!("logs in {}", self.dir.display()));
            bail!("{failure}");
        }
        let read = |f: &str| {
            std::fs::read_to_string(self.dir.join(f))
                .ok()
                .map(|s| s.trim().to_owned())
        };
        let code = read("exit_code").unwrap_or_else(|| "?".into());
        let secs = |f| read(f).and_then(|s| s.parse::<f64>().ok());
        let dur = match (secs("game_start"), secs("game_end")) {
            (Some(a), Some(b)) => format!("{:.1}s", b - a),
            _ => "?".into(),
        };
        let end = SessionRecord::read(&self.dir)
            .end
            .map(|e| format!(" ({})", e.end.as_str()))
            .unwrap_or_default();
        println!();
        say(
            "info",
            &format!(
                "game exited with code {code} after {dur}{end}; logs in {}",
                self.dir.display()
            ),
        );
        match &self.last_loader {
            Some(l) => say("info", &format!("last loader line: {l}")),
            None => {
                // The follower waits for the loader's rotation at attach, so
                // when the loader never attaches the session launcher's own
                // lines sit unnoticed in the log. Report them directly.
                let stub: Vec<String> = self
                    .loader
                    .as_ref()
                    .and_then(|f| {
                        let mut text = String::new();
                        f.open()?.read_to_string(&mut text).ok()?;
                        Some(text)
                    })
                    .map(|t| {
                        t.lines()
                            .filter(|l| l.contains("[offline]"))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                if stub.is_empty() {
                    say(
                        "info",
                        "no loader log for this session: the loader never attached",
                    );
                } else {
                    say("info", "the loader never attached; the session launcher reports:");
                    for l in stub.iter().rev().take(8).rev() {
                        say("loader", l);
                    }
                }
            }
        }
        for (i, line) in self.scan.summary(&self.symbols).into_iter().enumerate() {
            say(
                if self.scan.exceptions.is_empty() && i == 0 {
                    "info"
                } else {
                    "crash"
                },
                &line,
            );
        }
        if !self.scan.errors.is_empty() {
            let total: usize = self.scan.errors.values().sum();
            say(
                "info",
                &format!(
                    "{} distinct wine err: lines ({total} total)",
                    self.scan.errors.len()
                ),
            );
        }
        Ok(())
    }
}

pub(crate) fn symbols(game: &Path, exe: Option<&Path>) -> Symbols {
    let mut s = Symbols::parse_kb(KB);
    let exe = exe.map(Path::to_path_buf).unwrap_or_else(|| game.join(GAME_EXE));
    s.load_pdata(&exe).ok();
    s
}

#[cfg(all(test, unix))]
mod tests {
    use swofty_core::session::LOADER_LOG;

    use crate::commands::run::tests::{STALE, session_dir, temp_log};

    use super::*;

    #[test]
    fn ignores_a_stale_loader_log_and_follows_its_replacement() {
        let dir = session_dir("rotate");
        let log = dir.join("game").join(LOADER_LOG);
        std::fs::write(&log, STALE).unwrap();
        let mut follower = Follower::replacing(log.clone(), LogMark::read(&log));
        assert!(follower.lines().is_empty());
        assert!(follower.lines().is_empty());
        std::fs::rename(&log, log.with_file_name("loader.prev.log")).unwrap();
        std::fs::write(
            &log,
            "2026-10-17 22:12:08.004 [  420] swofty loader (version.dll proxy) attached, pid 416\npar",
        )
        .unwrap();
        assert_eq!(
            follower.lines(),
            ["2026-10-17 22:12:08.004 [  420] swofty loader (version.dll proxy) attached, pid 416"]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reports_a_game_process_proton_could_not_start() {
        let dir = session_dir("run-process");
        let log = dir.join("game").join(LOADER_LOG);
        std::fs::write(&log, STALE).unwrap();
        let before = LogMark::read(&log);
        std::fs::write(
            dir.join(format!("steam-{STEAM_APP_ID}.log")),
            include_str!("../../../../../packages/core/tests/fixtures/steam-run-process-731.txt"),
        )
        .unwrap();
        std::fs::write(dir.join("exit_code"), "1\n").unwrap();
        let mut follow = Follow::new(&dir, Some((log, before)), Symbols::default());
        follow.poll();
        assert_eq!(follow.last_loader, None);
        let err = follow.summary().unwrap_err().to_string();
        assert!(
            err.starts_with("Proton could not start the game process (CreateProcess error 731). "),
            "{err}"
        );
        assert!(err.ends_with("Nothing from SwoftyMod ran."), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relays_the_wrappers_lines_under_their_own_tags() {
        let log = temp_log("relay");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "old\n").unwrap();
        let mut relay = Follower::from_end(log.clone());
        std::fs::write(
            &log,
            "old\n[info] rolled the profile's game files back\nswofty: oops\n",
        )
        .unwrap();
        assert_eq!(
            relay.lines(),
            ["[info] rolled the profile's game files back", "swofty: oops"]
        );
        std::fs::remove_dir_all(log.ancestors().nth(2).unwrap()).ok();
    }
}
