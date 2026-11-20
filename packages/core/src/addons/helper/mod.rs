mod addon;
mod host_calls;
mod pool;
mod protocol;
mod reads;

#[cfg(all(test, unix))]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant, SystemTime};

use serde::de::DeserializeOwned;
use serde_json::Value;
use swofty_addon::helper::{API_VERSION, Call, Hello, HostCall, Incoming, RESTART, Reply, Request, Welcome};
use swofty_addon::launcher::{Claim, Cx, Progress, Summary};
use swofty_addon::manifest::{AddonManifest, Launcher};
use swofty_addon::view::{Body, Button, Notice, Style, Tone, View};

use crate::platform::app_control::Block;

use host_calls::{host_call, host_calls};
use protocol::{JsonResult, Live, Run, reply_range, send};

pub use pool::{Helpers, Pause};
pub use reads::HookRun;

type Result<T> = swofty_addon::Result<T>;

pub const LOG_FILE: &str = "helper.log";

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub quick: Duration,
    pub long: Duration,
    pub start: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            quick: Duration::from_secs(5),
            long: Duration::from_secs(300),
            start: Duration::from_secs(60),
        }
    }
}

enum Fault {
    Silent,
    Broke(String),
}

impl From<String> for Fault {
    fn from(e: String) -> Self {
        Self::Broke(e)
    }
}

impl Fault {
    fn text(self, wait: Duration) -> String {
        match self {
            Self::Silent => format!("did not answer within {} s", wait.as_secs_f32()),
            Self::Broke(e) => e,
        }
    }
}

struct StartLog(Option<std::fs::File>);

impl StartLog {
    fn create(path: &Path) -> Self {
        Self(std::fs::File::create(path).ok())
    }

    fn stderr(&self) -> Stdio {
        self.0
            .as_ref()
            .and_then(|f| f.try_clone().ok())
            .map_or_else(Stdio::null, Stdio::from)
    }

    fn note(&self, text: &str) {
        let Some(mut file) = self.0.as_ref() else {
            return;
        };
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        writeln!(
            file,
            "{} launcher: {text}",
            crate::diagnostics::send_logs::utc_stamp(secs)
        )
        .ok();
    }
}

pub struct HelperAddon {
    manifest: AddonManifest,
    spec: Launcher,
    dir: PathBuf,
    timeouts: Timeouts,
    run: Mutex<Run>,
    summaries: Mutex<HashMap<String, Option<Summary>>>,
    stale: Arc<Mutex<Option<Stale>>>,
    paused: Arc<AtomicUsize>,
    settled: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stale {
    All,
    Sections(BTreeSet<String>),
}

impl Stale {
    pub fn covers(&self, section: &str) -> bool {
        match self {
            Self::All => true,
            Self::Sections(s) => s.contains(section),
        }
    }

    fn add(slot: &mut Option<Stale>, section: Option<String>) {
        *slot = match (slot.take(), section) {
            (Some(Self::All), _) | (_, None) => Some(Self::All),
            (Some(Self::Sections(mut s)), Some(section)) => {
                s.insert(section);
                Some(Self::Sections(s))
            }
            (None, Some(section)) => Some(Self::Sections(BTreeSet::from([section]))),
        };
    }
}

// An invalidate notification may arrive at any time, between calls too, so the
// reader thread takes it out of the line stream before any exchange sees it.
fn invalidation(line: &str) -> Option<Option<String>> {
    if !line.contains("invalidate") {
        return None;
    }
    let message: Value = serde_json::from_str(line).ok()?;
    if message.get("id").is_some() || message["method"] != "invalidate" {
        return None;
    }
    Some(message["params"]["section"].as_str().map(str::to_owned))
}

impl HelperAddon {
    pub fn new(manifest: AddonManifest, dir: impl Into<PathBuf>) -> Option<Self> {
        let spec = manifest.launcher.clone()?;
        Some(Self {
            manifest,
            spec,
            dir: dir.into(),
            timeouts: Timeouts::default(),
            run: Mutex::new(Run::Idle),
            summaries: Mutex::new(HashMap::new()),
            stale: Arc::default(),
            paused: Arc::default(),
            settled: reads::SETTLED,
        })
    }

    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    #[cfg(all(test, unix))]
    fn settled_after(mut self, settled: Duration) -> Self {
        self.settled = settled;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn running(&self) -> bool {
        matches!(*self.lock(), Run::Live(_))
    }

    /// Running and idle, answered without waiting for a call or a start in progress.
    pub fn ready(&self) -> bool {
        match self.run.try_lock() {
            Ok(run) => matches!(*run, Run::Live(_)),
            Err(TryLockError::Poisoned(e)) => matches!(*e.into_inner(), Run::Live(_)),
            Err(TryLockError::WouldBlock) => false,
        }
    }

    pub fn failure(&self) -> Option<String> {
        match &*self.lock() {
            Run::Dead(e) => Some(e.clone()),
            _ => None,
        }
    }

    pub fn stop(&self) {
        *self.lock() = Run::Idle;
    }

    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst) > 0
    }

    /// The section's view, or None while another call holds the plugin, such as an
    /// import or a build, instead of waiting for it to finish.
    pub fn try_view(&self, cx: &Cx, section: &str) -> Option<Result<View>> {
        let run = match self.run.try_lock() {
            Ok(run) => run,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        if let Run::Dead(e) = &*run {
            return Some(Ok(self.dead_view(e.clone())));
        }
        if self.paused() && !matches!(*run, Run::Live(_)) {
            return None;
        }
        let call = Call::View {
            section: section.into(),
        };
        Some(
            self.call_with(run, cx, call, self.timeouts.quick, None)
                .or_else(|e| self.failure().map(|e| self.dead_view(e)).ok_or(e)),
        )
    }

    /// Sends a tick to a running, idle plugin. None when it isn't running or another
    /// call holds it; a tick never starts the plugin or waits for it.
    pub fn try_tick(&self, cx: &Cx, section: &str) -> Option<Result<()>> {
        let run = match self.run.try_lock() {
            Ok(run) => run,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        if !matches!(*run, Run::Live(_)) {
            return None;
        }
        let call = Call::Tick {
            section: section.into(),
        };
        Some(
            self.call_with::<serde::de::IgnoredAny>(run, cx, call, self.timeouts.quick, None)
                .map(drop),
        )
    }

    pub fn busy(&self) -> bool {
        matches!(self.run.try_lock(), Err(TryLockError::WouldBlock))
    }

    pub fn take_stale(&self) -> Option<Stale> {
        self.stale.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn mark_stale(&self, section: Option<&str>) {
        let mut stale = self.stale.lock().unwrap_or_else(|e| e.into_inner());
        Stale::add(&mut stale, section.map(str::to_owned));
    }

    fn lock(&self) -> MutexGuard<'_, Run> {
        self.run.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn call<T: DeserializeOwned>(
        &self,
        cx: &Cx,
        call: Call,
        wait: Duration,
        progress: Option<&dyn Progress>,
    ) -> Result<T> {
        self.call_with(self.lock(), cx, call, wait, progress)
    }

    fn call_with<T: DeserializeOwned>(
        &self,
        run: MutexGuard<'_, Run>,
        cx: &Cx,
        call: Call,
        wait: Duration,
        progress: Option<&dyn Progress>,
    ) -> Result<T> {
        self.call_asking(run, cx, call, wait, progress, None)
    }

    /// [`Self::call_with`], keeping in `asked` each host call the helper sends while it answers.
    fn call_asking<T: DeserializeOwned>(
        &self,
        mut run: MutexGuard<'_, Run>,
        cx: &Cx,
        call: Call,
        wait: Duration,
        progress: Option<&dyn Progress>,
        asked: Option<&mut Vec<reads::Asked>>,
    ) -> Result<T> {
        if let Run::Dead(e) = &*run {
            return Err(swofty_addon::Error::Other(e.clone()));
        }
        if matches!(*run, Run::Idle) {
            if self.paused() {
                return Err(swofty_addon::Error::Other(format!(
                    "{} is stopped while the game folder is being put back",
                    self.name()
                )));
            }
            *run = Run::Live(self.start(cx).map_err(|e| self.fail(&mut run, e))?);
        }
        let Run::Live(live) = &mut *run else {
            unreachable!("started above");
        };
        let exchange = self.exchange(live, cx, call, wait, progress, asked);
        match exchange {
            Ok(Ok(value)) => serde_json::from_str(value.get())
                .map_err(|e| self.fail(&mut run, format!("sent a bad result: {e}"))),
            Ok(Err(e)) => Err(swofty_addon::Error::Other(format!("{}: {e}", self.name()))),
            Err(e) => Err(self.fail(&mut run, e)),
        }
    }

    fn fail(&self, run: &mut Run, e: String) -> swofty_addon::Error {
        let e = format!("{} {e}", self.name());
        *run = Run::Dead(e.clone());
        swofty_addon::Error::Other(e)
    }

    // Runs `<program> claims <path>` on its own, apart from the live session, since
    // a drop is routed before any profile is at hand. None when it fails or stalls,
    // and the extensions in imports decide instead.
    fn ask_claims(&self, path: &Path) -> Option<Claim> {
        let program = self.dir.join(self.spec.program()?);
        executable(&program).ok()?;
        let mut command = Command::new(&program);
        command
            .arg("claims")
            .arg(path)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).ok();
            tx.send(line).ok();
        });
        let line = rx.recv_timeout(self.timeouts.quick);
        if line.is_err() {
            child.kill().ok();
        }
        child.wait().ok();
        match line.ok()?.trim() {
            "yes" => Some(Claim::Yes),
            "maybe" => Some(Claim::Maybe),
            "no" => Some(Claim::No),
            _ => None,
        }
    }

    fn removed(&self, rel: &str) -> String {
        let then = match crate::updates::managed::is_managed(&self.manifest.id) {
            true => "press Play again to download it again".to_owned(),
            false => format!("reinstall {}", self.name()),
        };
        format!(
            "could not start: its program {rel} is missing, most likely removed by an antivirus. Allow {} in your \
             antivirus, then {then}",
            self.dir.display()
        )
    }

    fn still_starting(&self) -> String {
        format!(
            "was still starting after {} s: an antivirus may be scanning it. Allow {} in your antivirus, then \
             press Restart",
            self.timeouts.start.as_secs_f32(),
            self.dir.display()
        )
    }

    fn start(&self, cx: &Cx) -> std::result::Result<Live, String> {
        let rel = self
            .spec
            .program()
            .ok_or_else(|| format!("has no helper for {}", std::env::consts::OS))?;
        let program = self.dir.join(rel);
        executable(&program).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound if self.dir.is_dir() => self.removed(rel),
            _ => format!("could not start {}: {e}", program.display()),
        })?;
        let log = StartLog::create(&cx.cache_dir.join(LOG_FILE));
        let mut command = Command::new(&program);
        command
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log.stderr());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        log.note(&format!("starting {rel}"));
        let spawned = Instant::now();
        let mut child = command.spawn().map_err(|e| {
            log.note(&format!("could not start: {e}"));
            match Block::of(&program, &e) {
                Some(b) => format!("could not start: {}", b.text()),
                None => format!("could not start {}: {e}", program.display()),
            }
        })?;
        let pid = child.id();
        log.note(&format!(
            "pid {pid} started in {} ms",
            spawned.elapsed().as_millis()
        ));
        let (stdin, stdout) = match (child.stdin.take(), child.stdout.take()) {
            (Some(i), Some(o)) => (i, o),
            _ => return Err("has no pipes".into()),
        };
        let (tx, lines) = channel();
        let stale = self.stale.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(section) = invalidation(&line) {
                    Stale::add(&mut stale.lock().unwrap_or_else(|e| e.into_inner()), section);
                    continue;
                }
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut live = Live {
            child,
            stdin,
            lines,
            next: 0,
            profile: cx.profile.clone(),
        };
        let shook = self.handshake(&mut live, cx, &log, pid);
        let after = spawned.elapsed().as_secs_f32();
        match &shook {
            Ok(()) => log.note(&format!("pid {pid} said hello after {after:.1} s")),
            Err(e) => log.note(&format!("pid {pid} {e} ({after:.1} s after it started)")),
        }
        shook.map(|()| live)
    }

    fn handshake(
        &self,
        live: &mut Live,
        cx: &Cx,
        log: &StartLog,
        pid: u32,
    ) -> std::result::Result<(), String> {
        let hello = Call::Hello(hello(cx));
        let id = self.request(live, cx, hello)?;
        let first = self.timeouts.quick.min(self.timeouts.start);
        let answer = match self.answer(live, cx, id, first, None, None) {
            Err(Fault::Silent) => {
                log.note(&format!(
                    "pid {pid} has not said hello after {} s, waiting up to {} s",
                    first.as_secs_f32(),
                    self.timeouts.start.as_secs_f32()
                ));
                self.answer(
                    live,
                    cx,
                    id,
                    self.timeouts.start.saturating_sub(first),
                    None,
                    None,
                )
            }
            answer => answer,
        };
        let welcome = answer
            .map_err(|e| match e {
                Fault::Silent => self.still_starting(),
                Fault::Broke(e) => e,
            })?
            .map_err(|e| format!("refused hello: {e}"))?;
        let welcome: Welcome =
            serde_json::from_str(welcome.get()).map_err(|e| format!("sent a bad hello: {e}"))?;
        if welcome.api != API_VERSION {
            return Err(format!(
                "speaks launcher API {}; this launcher speaks {API_VERSION}",
                welcome.api
            ));
        }
        Ok(())
    }

    fn exchange(
        &self,
        live: &mut Live,
        cx: &Cx,
        call: Call,
        wait: Duration,
        progress: Option<&dyn Progress>,
        asked: Option<&mut Vec<reads::Asked>>,
    ) -> std::result::Result<std::result::Result<JsonResult, String>, String> {
        let id = self.request(live, cx, call)?;
        self.answer(live, cx, id, wait, progress, asked)
            .map_err(|e| e.text(wait))
    }

    fn request(&self, live: &mut Live, cx: &Cx, call: Call) -> std::result::Result<u64, String> {
        if live.profile != *cx.profile && !matches!(call, Call::Hello(_)) {
            live.profile = cx.profile.clone();
            send(
                live,
                &Request {
                    id: None,
                    call: Call::Profile(cx.profile.clone()),
                },
            )?;
        }
        let id = live.next;
        live.next += 1;
        send(live, &Request { id: Some(id), call })?;
        Ok(id)
    }

    fn answer(
        &self,
        live: &mut Live,
        cx: &Cx,
        id: u64,
        wait: Duration,
        progress: Option<&dyn Progress>,
        mut asked: Option<&mut Vec<reads::Asked>>,
    ) -> std::result::Result<std::result::Result<JsonResult, String>, Fault> {
        loop {
            let line = match live.lines.recv_timeout(wait) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => return Err(Fault::Silent),
                Err(RecvTimeoutError::Disconnected) => {
                    let status = live
                        .child
                        .wait()
                        .map_or_else(|e| e.to_string(), |s| s.to_string());
                    return Err(Fault::Broke(format!("stopped ({status})")));
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Some((got, range)) = reply_range(&line) {
                if got != id {
                    return Err(Fault::Broke("answered a request it was not sent".into()));
                }
                return Ok(Ok(JsonResult { line, range }));
            }
            let message: Incoming =
                serde_json::from_str(&line).map_err(|e| format!("sent a bad message: {e}"))?;
            match message {
                Incoming::Reply(Reply::Ok { id: got, result }) if got == id => {
                    let line = result.to_string();
                    let range = 0..line.len();
                    return Ok(Ok(JsonResult { line, range }));
                }
                Incoming::Reply(Reply::Err { id: got, error }) if got == id => return Ok(Err(error)),
                Incoming::Reply(_) => return Err(Fault::Broke("answered a request it was not sent".into())),
                Incoming::Call(Request {
                    id: None,
                    call: HostCall::Progress { done, text },
                }) => {
                    if let Some(p) = progress {
                        p.report(done, &text);
                    }
                }
                Incoming::Call(Request {
                    id: None,
                    call: HostCall::Tasks(list),
                }) => {
                    cx.tasks(&list);
                }
                Incoming::Call(Request {
                    id: None,
                    call: HostCall::Invalidate(i),
                }) => Stale::add(
                    &mut self.stale.lock().unwrap_or_else(|e| e.into_inner()),
                    i.section,
                ),
                Incoming::Call(Request { id: Some(host), call }) => {
                    let kept = asked.is_some().then(|| call.clone());
                    let answer = host_call(cx, call);
                    if let (Some(asked), Some(call)) = (asked.as_deref_mut(), kept) {
                        asked.push(reads::Asked {
                            call,
                            answer: answer.clone(),
                        });
                    }
                    let reply = match answer {
                        Ok(result) => Reply::Ok { id: host, result },
                        Err(error) => Reply::Err { id: host, error },
                    };
                    send(live, &reply)?;
                }
                Incoming::Call(Request { id: None, .. }) => {
                    return Err(Fault::Broke("sent a host call without an id".into()));
                }
            }
        }
    }

    fn dead_view(&self, error: String) -> View {
        let mut restart = Button::new(RESTART, "Restart");
        restart.style = Style::Primary;
        View {
            subtitle: String::new(),
            actions: Vec::new(),
            notices: vec![Notice {
                text: error,
                tone: Tone::Bad,
                action: Some(restart),
            }],
            body: Body::Empty {
                title: format!("{} is not running", self.name()),
                text: "Restart it to load this page.".into(),
                action: None,
            },
            footer: None,
            tick: false,
        }
    }
}

/// The hello a session for `cx` starts with.
fn hello(cx: &Cx) -> Hello {
    Hello {
        api: API_VERSION,
        addon: cx.addon.to_owned(),
        data_dir: cx.data_dir.to_path_buf(),
        cache_dir: cx.cache_dir.to_path_buf(),
        profile: cx.profile.clone(),
        launcher: Some(env!("CARGO_PKG_VERSION").into()),
        host_calls: host_calls(cx),
        capabilities: vec![
            swofty_addon::helper::VIEW_CHART.into(),
            swofty_addon::helper::VIEW_CELL_IMAGE_DATA.into(),
            swofty_addon::helper::BUILD_READS.into(),
        ],
    }
}

#[cfg(unix)]
fn executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    if perms.mode() & 0o111 == 0 {
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn executable(path: &Path) -> std::io::Result<()> {
    std::fs::metadata(path).map(|_| ())
}
