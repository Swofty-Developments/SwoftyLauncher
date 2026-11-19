//! Bringing a store's client up before a launch starts the game: Steam signed in for a Steam copy, the
//! EA app running for an EA app copy. The lines are the ones `swofty run` logs under the store's name.

use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::platform::ea_app;
use crate::platform::steam::{self, client};

/// How a launch may bring a store's client up.
#[derive(Debug, Clone, Copy)]
pub struct ClientStart<'a> {
    /// The game folder the launch plays.
    pub game: &'a Path,
    /// `launch.start_store` is on and the launch has no `--no-start-store`.
    pub allowed: bool,
    /// `launch.ea_launch_command`: how the EA app is started and handed the launch link.
    pub ea_command: Option<&'a str>,
}

/// What [`super::LaunchPlatform::ensure_client`] found or did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientReady {
    /// The client was up already, or the session itself runs it.
    Running,
    /// The client was started and is ready.
    Started,
    /// The client was started, but no sign-in was seen in time; the launch goes ahead.
    Unconfirmed,
    /// The client is closed and starting it is turned off; the launch goes ahead.
    LeftClosed,
    /// The store's launch link starts the client.
    ByLink,
    /// The folder is not this store's copy, so its client plays no part.
    NotNeeded,
}

/// What a client start tells the player on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientNote {
    /// A line for the session log, under the store's name.
    Log(String),
    /// The launcher's status line while the launch waits; it goes when the wait ends.
    Stage(&'static str),
    /// Windows refused to start this program for the launch (error 740): it is set to run as
    /// administrator and the launcher isn't. The launch stops.
    RunAsAdmin(std::path::PathBuf),
}

const EA_START_TIMEOUT: Duration = Duration::from_secs(90);

/// A link handed over while the EA app is still starting can be dropped: under Proton one sent a
/// second after EADesktop.exe appeared never arrived.
const EA_SETTLE: Duration = Duration::from_secs(10);

/// Steam's side of a client start, with what it reads from the computer passed in.
pub(super) struct SteamClient<'a> {
    pub in_library: bool,
    pub steam_session: bool,
    pub detect: &'a mut dyn FnMut() -> client::Status,
    /// How Steam is started, found only when it has to be.
    pub starter: Box<dyn FnOnce() -> Option<client::Starter> + 'a>,
    pub wait: client::Wait,
}

impl<'a> SteamClient<'a> {
    /// Starts Steam whatever folder the game is in, looking for the client beside the game's Steam
    /// install and `game`.
    fn starting(game: Option<&'a Path>, detect: &'a mut dyn FnMut() -> client::Status) -> Self {
        SteamClient {
            in_library: true,
            steam_session: false,
            detect,
            starter: Box::new(move || {
                let root = steam::find_game().map(|g| g.steam_root);
                let hints: Vec<&Path> = root.as_deref().into_iter().chain(game).collect();
                client::starter(&hints)
            }),
            wait: client::Wait::default(),
        }
    }

    pub(super) fn for_launch(start: ClientStart<'a>, detect: &'a mut dyn FnMut() -> client::Status) -> Self {
        SteamClient {
            in_library: steam::in_steam_library(start.game),
            steam_session: client::steam_session(),
            ..Self::starting(Some(start.game), detect)
        }
    }

    pub(super) fn ensure(
        self,
        allowed: bool,
        note: &mut dyn FnMut(ClientNote),
    ) -> Result<ClientReady, String> {
        let log = |note: &mut dyn FnMut(ClientNote), text: String| note(ClientNote::Log(text));
        if !self.in_library {
            log(
                note,
                "the game folder isn't in a Steam library; not checking Steam".into(),
            );
            return Ok(ClientReady::NotNeeded);
        }
        if self.steam_session {
            log(note, "Steam runs this session; not checking it".into());
            return Ok(ClientReady::Running);
        }
        let detect = self.detect;
        if !allowed {
            let status = detect();
            log(note, format!("detected: {status}"));
            if status.state.usable() {
                return Ok(ClientReady::Running);
            }
            log(
                note,
                "starting Steam is turned off (launch.start_store or --no-start-store); open Steam if F1 25 \
                 doesn't start"
                    .into(),
            );
            return Ok(ClientReady::LeftClosed);
        }
        let starter = (self.starter)();
        let timeout = self.wait.timeout;
        let note = std::cell::RefCell::new(note);
        let ready = client::ensure(
            detect,
            || {
                let starter = starter.as_ref().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Steam was not found on this computer",
                    )
                })?;
                (note.borrow_mut())(ClientNote::Log(format!("starting Steam: {starter}")));
                starter.start()
            },
            self.wait,
            |report| {
                let mut note = note.borrow_mut();
                match report {
                    client::Report::Detected(status) => note(ClientNote::Log(format!("detected: {status}"))),
                    client::Report::Changed(status, after) if status.state == client::State::Ready => {
                        note(ClientNote::Log(format!(
                            "ready after {:.1} s ({})",
                            after.as_secs_f64(),
                            status.detail()
                        )))
                    }
                    client::Report::Changed(status, after) => note(ClientNote::Log(format!(
                        "{status} at {:.1} s",
                        after.as_secs_f64()
                    ))),
                    client::Report::Stage(s) => note(ClientNote::Stage(s.text())),
                }
            },
        )
        .map_err(|e| e.to_string())?;
        let note = note.into_inner();
        Ok(match ready {
            client::Ready::Running => ClientReady::Running,
            client::Ready::Started => ClientReady::Started,
            client::Ready::Unconfirmed => {
                log(
                    note,
                    format!("no sign-in seen after {} s; launching anyway", timeout.as_secs()),
                );
                ClientReady::Unconfirmed
            }
        })
    }
}

impl steam::Steam {
    /// Brings Steam up and signed in whatever folder the game is in, as `swofty steam start` does.
    pub fn start_client(
        &self,
        game: Option<&Path>,
        note: &mut dyn FnMut(ClientNote),
    ) -> Result<ClientReady, String> {
        let mut detect = client::status;
        SteamClient::starting(game, &mut detect).ensure(true, note)
    }
}

/// What the EA app's start is, after the setting and the computer are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EaPolicy {
    NoHandoff,
    Running,
    Start,
    Off,
}

pub(super) fn ea_policy(handoff: bool, allowed: bool, running: bool) -> EaPolicy {
    match (handoff, running, allowed) {
        (false, _, _) => EaPolicy::NoHandoff,
        (true, true, _) => EaPolicy::Running,
        (true, false, true) => EaPolicy::Start,
        (true, false, false) => EaPolicy::Off,
    }
}

/// The EA app's side of a client start, with what it reads from the computer passed in.
pub(super) struct EaClient<'a> {
    pub handoff: bool,
    pub running: &'a dyn Fn() -> bool,
    /// The command that opens the EA app, and whether its exit code says it failed to; found only
    /// when the EA app has to be started.
    pub opener: Box<dyn FnOnce() -> Option<(Command, bool)> + 'a>,
    pub timeout: Duration,
    pub settle: Duration,
    pub poll: Duration,
}

impl EaClient<'_> {
    pub(super) fn for_launch(start: ClientStart<'_>) -> EaClient<'static> {
        let handoff = ea_app::handoff(start.ea_command);
        EaClient {
            handoff: handoff.is_some(),
            running: &ea_app::running,
            opener: Box::new(move || {
                handoff
                    .as_ref()
                    .and_then(|h| ea_app::desktop_opener(h, crate::platform::run_as_admin::elevated()))
            }),
            timeout: EA_START_TIMEOUT,
            settle: EA_SETTLE,
            poll: client::POLL_INTERVAL,
        }
    }

    pub(super) fn ensure(
        self,
        allowed: bool,
        note: &mut dyn FnMut(ClientNote),
    ) -> Result<ClientReady, String> {
        match ea_policy(self.handoff, allowed, (self.running)()) {
            EaPolicy::NoHandoff => Err(ea_app::no_handoff().to_owned()),
            EaPolicy::Running => {
                note(ClientNote::Log("detected: running".into()));
                Ok(ClientReady::Running)
            }
            EaPolicy::Off => Err(
                "the EA app is closed and starting it is turned off (launch.start_store or \
                                  --no-start-store); open the EA app and press Play again"
                    .into(),
            ),
            EaPolicy::Start => {
                let Some((mut open, exit_tells)) = (self.opener)() else {
                    note(ClientNote::Log(
                        "detected: closed; the launch link starts it".into(),
                    ));
                    return Ok(ClientReady::ByLink);
                };
                note(ClientNote::Log("detected: closed; starting the EA app".into()));
                note(ClientNote::Stage("Starting the EA app…"));
                let mut child = detach(&mut open).map_err(|e| {
                    if crate::platform::run_as_admin::needs_elevation(&e) {
                        note(ClientNote::RunAsAdmin(std::path::PathBuf::from(
                            open.get_program(),
                        )));
                    }
                    format!("could not start the EA app ({e}); open the EA app and press Play again")
                })?;
                let since = Instant::now();
                match wait_for_app(&mut child, exit_tells, self.running, self.poll, || {
                    since.elapsed() >= self.timeout
                }) {
                    AppStart::Running => {
                        note(ClientNote::Log(format!(
                            "running after {:.1} s",
                            since.elapsed().as_secs_f64()
                        )));
                        std::thread::sleep(self.settle);
                        Ok(ClientReady::Started)
                    }
                    AppStart::CommandFailed(exit) => {
                        note(ClientNote::Log(format!(
                            "the command did not open the EA app ({exit}); the launch link starts it"
                        )));
                        Ok(ClientReady::ByLink)
                    }
                    AppStart::TimedOut => Err(format!(
                        "the EA app didn't start within {} s; open the EA app and press Play again",
                        self.timeout.as_secs()
                    )),
                }
            }
        }
    }
}

enum AppStart {
    Running,
    CommandFailed(ExitStatus),
    TimedOut,
}

/// Waits for EADesktop.exe after `child` was asked to open the EA app. A command that exits with an
/// error opened nothing, so waiting is pointless, unless its exit code says nothing about the EA app
/// (`exit_tells` false).
fn wait_for_app(
    child: &mut Child,
    exit_tells: bool,
    running: &dyn Fn() -> bool,
    poll: Duration,
    timed_out: impl Fn() -> bool,
) -> AppStart {
    loop {
        if running() {
            return AppStart::Running;
        }
        if let Ok(Some(exit)) = child.try_wait()
            && exit_tells
            && !exit.success()
        {
            return AppStart::CommandFailed(exit);
        }
        if timed_out() {
            return AppStart::TimedOut;
        }
        std::thread::sleep(poll);
    }
}

fn detach(c: &mut Command) -> std::io::Result<Child> {
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(c, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(c, 0x0000_0008 | 0x0000_0200);
    c.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::steam::client::{Signal, State, Status};

    fn status(state: State) -> Status {
        Status {
            state,
            pid: None,
            signal: Signal::Unknown,
        }
    }

    fn quick() -> client::Wait {
        client::Wait {
            timeout: Duration::from_millis(200),
            poll: Duration::from_millis(10),
        }
    }

    fn steam<'a>(detect: &'a mut dyn FnMut() -> Status) -> SteamClient<'a> {
        SteamClient {
            in_library: true,
            steam_session: false,
            detect,
            starter: Box::new(|| None),
            wait: quick(),
        }
    }

    fn run(c: SteamClient<'_>, allowed: bool) -> (Result<ClientReady, String>, Vec<ClientNote>) {
        let mut notes = Vec::new();
        let got = c.ensure(allowed, &mut |n| notes.push(n));
        (got, notes)
    }

    #[test]
    fn steam_is_checked_only_for_a_steam_copy_outside_a_steam_session() {
        let mut detect = || panic!("not asked");
        let (got, notes) = run(
            SteamClient {
                in_library: false,
                ..steam(&mut detect)
            },
            true,
        );
        assert_eq!(got, Ok(ClientReady::NotNeeded));
        assert_eq!(
            notes,
            [ClientNote::Log(
                "the game folder isn't in a Steam library; not checking Steam".into()
            )]
        );
        let mut detect = || panic!("not asked");
        let (got, notes) = run(
            SteamClient {
                steam_session: true,
                ..steam(&mut detect)
            },
            true,
        );
        assert_eq!(got, Ok(ClientReady::Running));
        assert_eq!(
            notes,
            [ClientNote::Log("Steam runs this session; not checking it".into())]
        );
    }

    #[test]
    fn a_closed_steam_stays_closed_when_starting_it_is_off() {
        let mut detect = || status(State::Stopped);
        let (got, notes) = run(steam(&mut detect), false);
        assert_eq!(got, Ok(ClientReady::LeftClosed));
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(matches!(&notes[1], ClientNote::Log(l) if l.starts_with("starting Steam is turned off")));
        let mut detect = || status(State::Ready);
        assert_eq!(run(steam(&mut detect), false).0, Ok(ClientReady::Running));
    }

    #[test]
    fn a_running_steam_is_used_and_a_missing_one_is_explained() {
        let mut detect = || status(State::Ready);
        let (got, notes) = run(steam(&mut detect), true);
        assert_eq!(got, Ok(ClientReady::Running));
        assert!(matches!(&notes[0], ClientNote::Log(l) if l.starts_with("detected: ")));
        let mut detect = || status(State::Stopped);
        let (got, _) = run(steam(&mut detect), true);
        assert_eq!(
            got,
            Err("could not start Steam (Steam was not found on this computer); open Steam and press Play again".into())
        );
    }

    #[test]
    fn a_steam_waiting_on_its_sign_in_counts_as_running() {
        let mut detect = || status(State::Unconfirmed);
        let (got, notes) = run(steam(&mut detect), true);
        assert_eq!(got, Ok(ClientReady::Running));
        assert!(
            !notes
                .iter()
                .any(|n| matches!(n, ClientNote::Log(l) if l.starts_with("no sign-in")))
        );
    }

    fn ea(running: &dyn Fn() -> bool, opener: Option<(Command, bool)>) -> EaClient<'_> {
        EaClient {
            handoff: true,
            running,
            opener: Box::new(move || opener),
            timeout: Duration::from_millis(300),
            settle: Duration::ZERO,
            poll: Duration::from_millis(10),
        }
    }

    fn run_ea(c: EaClient<'_>, allowed: bool) -> (Result<ClientReady, String>, Vec<ClientNote>) {
        let mut notes = Vec::new();
        let got = c.ensure(allowed, &mut |n| notes.push(n));
        (got, notes)
    }

    #[test]
    fn the_ea_app_policy_follows_the_link_the_setting_and_the_app() {
        assert_eq!(ea_policy(false, true, true), EaPolicy::NoHandoff);
        assert_eq!(ea_policy(true, false, true), EaPolicy::Running);
        assert_eq!(ea_policy(true, true, false), EaPolicy::Start);
        assert_eq!(ea_policy(true, false, false), EaPolicy::Off);
        let (got, _) = run_ea(
            EaClient {
                handoff: false,
                ..ea(&|| false, None)
            },
            true,
        );
        assert_eq!(got, Err(ea_app::no_handoff().to_owned()));
        let (got, notes) = run_ea(ea(&|| true, None), false);
        assert_eq!(got, Ok(ClientReady::Running));
        assert_eq!(notes, [ClientNote::Log("detected: running".into())]);
        let (got, _) = run_ea(ea(&|| false, None), false);
        assert!(
            got.unwrap_err()
                .starts_with("the EA app is closed and starting it is turned off")
        );
        let (got, notes) = run_ea(ea(&|| false, None), true);
        assert_eq!(got, Ok(ClientReady::ByLink));
        assert_eq!(
            notes,
            [ClientNote::Log(
                "detected: closed; the launch link starts it".into()
            )]
        );
    }

    #[cfg(unix)]
    #[test]
    fn starting_the_ea_app_waits_for_it_or_leaves_it_to_the_link() {
        let failing = || {
            let mut c = Command::new("sh");
            c.args(["-c", "exit 3"]);
            c
        };
        let (got, notes) = run_ea(ea(&|| false, Some((failing(), true))), true);
        assert_eq!(got, Ok(ClientReady::ByLink));
        assert!(
            matches!(notes.last(), Some(ClientNote::Log(l)) if l.starts_with("the command did not open the EA app"))
        );
        let (got, _) = run_ea(ea(&|| false, Some((failing(), false))), true);
        assert_eq!(
            got,
            Err("the EA app didn't start within 0 s; open the EA app and press Play again".into())
        );
        let (got, _) = run_ea(ea(&|| false, Some((Command::new("true"), true))), true);
        assert_eq!(
            got,
            Err("the EA app didn't start within 0 s; open the EA app and press Play again".into())
        );
        let calls = std::cell::Cell::new(0);
        let running = || {
            calls.set(calls.get() + 1);
            calls.get() > 2
        };
        let (got, notes) = run_ea(ea(&running, Some((failing(), false))), true);
        assert_eq!(got, Ok(ClientReady::Started));
        assert_eq!(notes[1], ClientNote::Stage("Starting the EA app…"));
    }

    #[cfg(unix)]
    /// The notes as the session log shows them, with the timings and exit codes as `#`.
    fn lines(notes: &[ClientNote]) -> Vec<String> {
        notes
            .iter()
            .map(|n| match n {
                ClientNote::Log(l) => l
                    .chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c })
                    .collect(),
                ClientNote::Stage(s) => format!("stage: {s}"),
                ClientNote::RunAsAdmin(p) => format!("run as admin: {}", p.display()),
            })
            .collect()
    }

    #[cfg(unix)]
    fn states(seen: Vec<State>) -> impl FnMut() -> Status {
        let mut seen = seen.into_iter();
        let mut last = State::Stopped;
        move || {
            if let Some(s) = seen.next() {
                last = s;
            }
            status(last)
        }
    }

    #[cfg(unix)]
    #[test]
    fn starting_steam_logs_the_lines_the_cli_has_always_logged() {
        let starter = || Some(client::Starter::Exe("true".into()));
        let mut detect = states(vec![
            State::Stopped,
            State::Starting,
            State::SigningIn,
            State::Ready,
        ]);
        let (got, notes) = run(
            SteamClient {
                starter: Box::new(starter),
                wait: client::Wait {
                    timeout: Duration::from_secs(5),
                    poll: Duration::from_millis(1),
                },
                ..steam(&mut detect)
            },
            true,
        );
        assert_eq!(got, Ok(ClientReady::Started));
        assert_eq!(
            lines(&notes),
            [
                "detected: not running",
                "stage: Starting Steam…",
                "starting Steam: true -silent",
                "starting (the client hasn't registered yet) at #.# s",
                "waiting for sign-in (no sign-in signal) at #.# s",
                "stage: Waiting for Steam to sign in…",
                "ready after #.# s (no sign-in signal)",
            ]
        );
        let mut detect = states(vec![State::Unconfirmed]);
        let (got, notes) = run(
            SteamClient {
                wait: client::Wait {
                    timeout: Duration::from_millis(30),
                    poll: Duration::from_millis(1),
                },
                ..steam(&mut detect)
            },
            true,
        );
        assert_eq!(got, Ok(ClientReady::Running));
        assert_eq!(
            lines(&notes),
            ["detected: waiting for sign-in (no sign-in signal)"]
        );
        let mut detect = states(vec![State::Starting, State::Unconfirmed]);
        let (got, notes) = run(
            SteamClient {
                wait: client::Wait {
                    timeout: Duration::from_millis(30),
                    poll: Duration::from_millis(1),
                },
                ..steam(&mut detect)
            },
            true,
        );
        assert_eq!(got, Ok(ClientReady::Unconfirmed));
        assert_eq!(
            lines(&notes),
            [
                "detected: starting (the client hasn't registered yet)",
                "stage: Starting Steam…",
                "waiting for sign-in (no sign-in signal) at #.# s",
                "stage: Waiting for Steam to sign in…",
                "no sign-in seen after # s; launching anyway",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn starting_the_ea_app_logs_the_lines_the_cli_has_always_logged() {
        let command = |code: u8| {
            let mut c = Command::new("sh");
            c.args(["-c", &format!("exit {code}")]);
            c
        };
        let calls = std::cell::Cell::new(0);
        let running = || {
            calls.set(calls.get() + 1);
            calls.get() > 2
        };
        let (got, notes) = run_ea(ea(&running, Some((command(0), true))), true);
        assert_eq!(got, Ok(ClientReady::Started));
        assert_eq!(
            lines(&notes),
            [
                "detected: closed; starting the EA app",
                "stage: Starting the EA app…",
                "running after #.# s",
            ]
        );
        let (got, notes) = run_ea(ea(&|| false, Some((command(3), true))), true);
        assert_eq!(got, Ok(ClientReady::ByLink));
        assert_eq!(
            lines(&notes),
            [
                "detected: closed; starting the EA app",
                "stage: Starting the EA app…",
                "the command did not open the EA app (exit status: #); the launch link starts it",
            ]
        );
    }
}
