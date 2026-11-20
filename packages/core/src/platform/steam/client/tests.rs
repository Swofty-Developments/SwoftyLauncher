use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
#[cfg(unix)]
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::linux::*;
use super::registry::*;
use super::start::*;
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("swofty-steamc-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".steam")).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn proc(pid: u32, name: &str, exe: &str, started: Option<SystemTime>) -> Proc {
    Proc {
        pid,
        ns_pid: pid,
        name: name.into(),
        exe: exe.into(),
        started,
    }
}

fn active(pid: u32, user: u32) -> String {
    format!(
        r#""Registry" {{ "HKLM" {{ "Software" {{ "Valve" {{ "Steam" {{ "SteamPID" "{pid}" }} }} }} }}
               "HKCU" {{ "Software" {{ "Valve" {{ "Steam" {{ "ActiveProcess" {{
                 "pid" "{pid}" "ActiveUser" "{user}" }} }} }} }} }} }}"#
    )
}

const CLIENT: &str = "/home/u/.local/share/Steam/ubuntu12_32/steam";

#[test]
fn linux_needs_a_live_client_behind_the_pid_file() {
    let home = Home::new("pidfile");
    let earlier = Some(SystemTime::now() - Duration::from_secs(60));
    assert_eq!(linux_status(&home.0, &[]), Status::STOPPED);
    home.write(".steam/steam.pid", "4242\n");
    assert_eq!(linux_status(&home.0, &[]), Status::STOPPED);
    let reused = [proc(4242, "bash", "/bin/bash", earlier)];
    assert_eq!(linux_status(&home.0, &reused).state, State::Stopped);
    let bootstrap = [proc(4300, "steam", "/bin/bash", earlier)];
    assert_eq!(
        linux_status(&home.0, &bootstrap),
        Status::new(State::Starting, 4300, Signal::Unknown)
    );
    let client = [proc(4242, "steam", CLIENT, earlier)];
    assert_eq!(
        linux_status(&home.0, &client),
        Status::new(State::Unconfirmed, 4242, Signal::NoLogin)
    );
    home.write(".steam/steam/config/loginusers.vdf", r#""users" { }"#);
    assert_eq!(linux_status(&home.0, &client).state, State::Ready);
}

#[test]
fn linux_reads_the_signed_in_account_when_steam_publishes_it() {
    let home = Home::new("active");
    let later = Some(SystemTime::now() + Duration::from_secs(3600));
    let client = [proc(77, "steam", CLIENT, later)];
    home.write(".steam/registry.vdf", &active(77, 0));
    assert_eq!(linux_status(&home.0, &client).state, State::SigningIn);
    home.write(".steam/registry.vdf", &active(77, 123_456));
    assert_eq!(linux_status(&home.0, &client).state, State::Ready);
    home.write(".steam/registry.vdf", &active(78, 123_456));
    home.write(".steam/steam.pid", "77");
    assert_eq!(
        linux_status(&home.0, &client).state,
        State::Unconfirmed,
        "a stale ActiveProcess from another client says nothing"
    );
}

#[test]
fn linux_dates_the_last_sign_in_against_the_client_start() {
    let home = Home::new("login");
    let now = SystemTime::now();
    home.write(".steam/steam.pid", "9");
    home.write(
        ".local/share/Steam/config/loginusers.vdf",
        r#""users" { "76561198000000001" { "MostRecent" "1" "Timestamp" "1000" } }"#,
    );
    let file = home.0.join(".local/share/Steam/config/loginusers.vdf");
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1000))
        .unwrap();
    let at = |secs: u64| {
        [proc(
            9,
            "steam",
            CLIENT,
            Some(UNIX_EPOCH + Duration::from_secs(secs)),
        )]
    };
    assert_eq!(linux_status(&home.0, &at(990)).state, State::Ready);
    assert_eq!(linux_status(&home.0, &at(1004)).state, State::Ready);
    assert_eq!(linux_status(&home.0, &at(2000)).state, State::Unconfirmed);
    let stamp = now.duration_since(UNIX_EPOCH).unwrap().as_secs().to_string();
    home.write(
        ".local/share/Steam/config/loginusers.vdf",
        &format!(r#""users" {{ "1" {{ "Timestamp" "{stamp}" }} }}"#),
    );
    assert_eq!(linux_status(&home.0, &at(2000)).state, State::Ready);
    assert_eq!(
        linux_status(&home.0, &[proc(9, "steam", CLIENT, None)]).state,
        State::Ready
    );
}

#[test]
fn linux_finds_flatpak_steam_by_its_sandbox_pid() {
    let home = Home::new("flatpak");
    home.write(".var/app/com.valvesoftware.Steam/.steam/steam.pid", "2");
    home.write(
        ".var/app/com.valvesoftware.Steam/.steam/registry.vdf",
        &active(2, 5),
    );
    let sandboxed = Proc {
        ns_pid: 2,
        ..proc(
            51_000,
            "steam",
            "/home/u/.var/app/com.valvesoftware.Steam/.local/share/Steam/ubuntu12_32/steam",
            None,
        )
    };
    assert_eq!(
        linux_status(&home.0, std::slice::from_ref(&sandboxed)),
        Status::new(State::Ready, 51_000, Signal::ActiveUser(5))
    );
    home.write(".steam/steam.pid", "2");
    let native = proc(60_000, "steam", "/bin/sh", None);
    assert_eq!(
        linux_status(&home.0, &[native.clone(), sandboxed]).pid,
        Some(51_000),
        "a native pid file never matches a sandbox pid"
    );
    std::fs::remove_dir_all(home.0.join(".var")).unwrap();
    let unfiled = proc(61_000, "steam", CLIENT, None);
    assert_eq!(
        linux_status(&home.0, &[native, unfiled]),
        Status::new(State::Ready, 61_000, Signal::Unknown),
        "the client binary counts without a pid file"
    );
}

struct FakeRegistry(BTreeMap<&'static str, String>);

impl Registry for FakeRegistry {
    fn query(&self, key: &str) -> Option<String> {
        Some(self.0.get(key).cloned().unwrap_or_else(|| {
            "ERROR: The system was unable to find the specified registry key or value.\r\n".into()
        }))
    }
}

fn active_process(pid: u32, user: u32) -> String {
    format!(
        "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\\ActiveProcess\r\n    pid    REG_DWORD    {pid:#x}\r\n    \
             SteamClientDll    REG_SZ    C:\\Program Files (x86)\\Steam\\steamclient.dll\r\n    \
             ActiveUser    REG_DWORD    {user:#x}\r\n    Universe    REG_SZ    Public\r\n\r\n"
    )
}

fn win(reg: &FakeRegistry, steam_pids: &[u32]) -> Status {
    windows_status(reg, steam_pids, &|_| None, &[])
}

#[test]
fn windows_matches_active_process_to_a_running_steam_exe() {
    let none = FakeRegistry(BTreeMap::new());
    assert_eq!(win(&none, &[]), Status::STOPPED);
    assert_eq!(win(&none, &[880]).state, State::Starting);
    let reg = |pid, user| FakeRegistry([(ACTIVE_PROCESS_KEY, active_process(pid, user))].into());
    assert_eq!(win(&reg(0, 0), &[]), Status::STOPPED);
    assert_eq!(win(&reg(0, 0), &[880]).state, State::Starting);
    assert_eq!(
        win(&reg(0x1a2b, 0), &[0x1a2b]),
        Status::new(State::SigningIn, 0x1a2b, Signal::ActiveUser(0))
    );
    assert_eq!(win(&reg(0x1a2b, 98_765), &[0x1a2b]).state, State::Ready);
    assert_eq!(
        win(&reg(0x1a2b, 98_765), &[]).state,
        State::Stopped,
        "a crashed client leaves its pid behind"
    );
    let unnamed = FakeRegistry([(ACTIVE_PROCESS_KEY, "    pid    REG_DWORD    0x10\r\n".to_owned())].into());
    assert_eq!(win(&unnamed, &[16]).state, State::Unconfirmed);
}

#[test]
fn windows_takes_a_sign_in_loginusers_records_over_active_user_0() {
    let root = Home::new("winlogin");
    let now = SystemTime::now();
    let secs = |t: SystemTime| t.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let login = |t: SystemTime| {
        root.write(
            "config/loginusers.vdf",
            &format!(
                r#""users" {{ "76561198000000001" {{ "MostRecent" "1" "Timestamp" "{}" }}
                                 "76561198000000002" {{ "MostRecent" "0" "Timestamp" "1000" }} }}"#,
                secs(t)
            ),
        )
    };
    let registered = |name: &str, path: PathBuf| {
        FakeRegistry(
            [
                (ACTIVE_PROCESS_KEY, active_process(0x1a2b, 0)),
                (
                    STEAM_KEY,
                    format!("    {name}    REG_SZ    {}\r\n", path.display()),
                ),
            ]
            .into(),
        )
    };
    let reg = registered("SteamPath", root.0.clone());
    let at = |ago: u64| move |_: u32| Some(now - Duration::from_secs(ago));
    let pids = [0x1a2b];

    assert_eq!(
        windows_status(&reg, &pids, &at(120), &[]),
        Status::new(State::SigningIn, 0x1a2b, Signal::ActiveUser(0)),
        "no loginusers.vdf"
    );
    login(now - Duration::from_secs(60));
    assert_eq!(
        windows_status(&reg, &pids, &at(120), &[]),
        Status::new(State::Ready, 0x1a2b, Signal::LoginOverActiveUser)
    );
    assert_eq!(
        windows_status(&reg, &pids, &at(30), &[]).state,
        State::SigningIn,
        "a sign-in before this steam.exe started"
    );
    assert_eq!(
        windows_status(&reg, &pids, &|_| None, &[]).state,
        State::SigningIn,
        "a steam.exe it cannot date"
    );
    let other = registered("SteamPath", root.0.join("elsewhere"));
    assert_eq!(
        windows_status(&other, &pids, &at(120), &[]).state,
        State::SigningIn
    );
    assert_eq!(
        windows_status(&other, &pids, &at(120), std::slice::from_ref(&root.0)).state,
        State::Ready
    );
    let exe = registered("SteamExe", root.0.join("steam.exe"));
    assert_eq!(windows_status(&exe, &pids, &at(120), &[]).state, State::Ready);

    login(now - Duration::from_secs(5));
    assert_eq!(
        windows_status(&reg, &pids, &at(120), &[]).state,
        State::SigningIn,
        "ActiveUser gets time to follow a fresh sign-in"
    );
    root.write("config/loginusers.vdf", r#""users" { "1" { "MostRecent" "1" } }"#);
    assert_eq!(
        windows_status(&reg, &pids, &at(120), &[]).state,
        State::SigningIn,
        "the file changing is no sign-in"
    );
    login(now - Duration::from_secs(60));
    assert_eq!(
        windows_status(&reg, &[], &at(120), &[]),
        Status::STOPPED,
        "a crashed client leaves its pid behind"
    );
    let signed_in = FakeRegistry(
        [
            (ACTIVE_PROCESS_KEY, active_process(0x1a2b, 98_765)),
            (
                STEAM_KEY,
                format!("    SteamPath    REG_SZ    {}\r\n", root.0.display()),
            ),
        ]
        .into(),
    );
    assert_eq!(
        windows_status(&signed_in, &pids, &at(120), &[]).signal,
        Signal::ActiveUser(98_765)
    );
}

#[test]
fn windows_starts_the_registered_steam_exe() {
    let reg = |listing: &str| FakeRegistry([(STEAM_KEY, listing.to_owned())].into());
    let both = "HKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    \
                    SteamExe    REG_SZ    d:/steam/steam.exe\r\n    SteamPath    REG_SZ    d:/other\r\n";
    assert_eq!(
        windows_starter(&reg(both), &[]),
        Starter::Exe("d:/steam/steam.exe".into())
    );
    assert_eq!(
        windows_starter(&reg("    SteamPath    REG_SZ    e:/steam\r\n"), &[]),
        Starter::Exe(PathBuf::from("e:/steam").join("steam.exe"))
    );
    let root = Home::new("winroot");
    std::fs::write(root.0.join("steam.exe"), "MZ").unwrap();
    assert_eq!(
        windows_starter(&FakeRegistry(BTreeMap::new()), std::slice::from_ref(&root.0)),
        Starter::Exe(root.0.join("steam.exe"))
    );
    assert_eq!(windows_starter(&FakeRegistry(BTreeMap::new()), &[]), Starter::Url);
    assert!(Starter::Url.command(&["-applaunch"]).is_none());
}

#[test]
fn linux_starts_the_steam_that_holds_the_game() {
    let home = Home::new("starter");
    let bin = home.0.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = std::env::join_paths([&bin]).unwrap();
    assert_eq!(linux_starter(Some(&path), &home.0, &[]), None);
    std::fs::write(bin.join("flatpak"), "").unwrap();
    assert_eq!(
        linux_starter(Some(&path), &home.0, &[]),
        None,
        "flatpak without Steam's app folder"
    );
    std::fs::write(bin.join("steam"), "").unwrap();
    assert_eq!(
        linux_starter(Some(&path), &home.0, &[]),
        Some(Starter::Exe(bin.join("steam")))
    );
    let app = home.0.join(".var/app/com.valvesoftware.Steam");
    std::fs::create_dir_all(&app).unwrap();
    let root = app.join(".local/share/Steam");
    assert_eq!(
        linux_starter(Some(&path), &home.0, &[&root]),
        Some(Starter::Flatpak(bin.join("flatpak")))
    );
    std::fs::remove_file(bin.join("steam")).unwrap();
    assert_eq!(
        linux_starter(Some(&path), &home.0, &[]),
        Some(Starter::Flatpak(bin.join("flatpak")))
    );
    let cmd = Starter::Flatpak(bin.join("flatpak"))
        .command(&["-applaunch", "3059520"])
        .unwrap();
    let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args, ["run", FLATPAK_APP, "-applaunch", "3059520"]);
    assert_eq!(linux_starter(None, &home.0, &[]), None);
}

fn quick() -> Wait {
    Wait {
        timeout: Duration::from_millis(300),
        poll: Duration::from_millis(10),
    }
}

fn scripted(states: &[State]) -> impl FnMut() -> Status + use<> {
    let states = states.to_vec();
    let i = Cell::new(0);
    move || {
        let n = i.get();
        i.set(n + 1);
        Status {
            state: states[n.min(states.len() - 1)],
            pid: None,
            signal: Signal::Unknown,
        }
    }
}

fn never() -> std::io::Result<Child> {
    panic!("Steam was started while it was already up")
}

fn stages(reports: &[Report]) -> Vec<Stage> {
    reports
        .iter()
        .filter_map(|r| match r {
            Report::Stage(s) => Some(*s),
            _ => None,
        })
        .collect()
}

fn changes(reports: &[Report]) -> Vec<State> {
    reports
        .iter()
        .filter_map(|r| match r {
            Report::Changed(s, _) => Some(s.state),
            _ => None,
        })
        .collect()
}

#[test]
fn says_what_it_sees() {
    assert_eq!(Status::STOPPED.to_string(), "not running");
    assert_eq!(
        Status::new(State::Starting, 9, Signal::Unknown).to_string(),
        "starting (pid 9, the client hasn't registered yet)"
    );
    assert_eq!(
        Status::new(State::SigningIn, 4242, Signal::ActiveUser(0)).to_string(),
        "waiting for sign-in (pid 4242, ActiveUser 0)"
    );
    assert_eq!(
        Status::new(State::Unconfirmed, 4242, Signal::NoLogin).to_string(),
        "waiting for sign-in (pid 4242, no sign-in in loginusers.vdf since it started)"
    );
    assert_eq!(
        Status::new(State::Ready, 4242, Signal::LoginFile).to_string(),
        "ready (pid 4242, loginusers.vdf written since it started)"
    );
    assert_eq!(
        Status::new(State::Ready, 4242, Signal::LoginOverActiveUser).to_string(),
        "ready (pid 4242, ActiveUser 0, but loginusers.vdf records a sign-in since it started)"
    );
}

#[test]
fn leaves_a_running_steam_alone() {
    let seen = RefCell::new(Vec::new());
    for up in [State::Ready, State::Unconfirmed] {
        let got = ensure(scripted(&[up]), never, quick(), |r| seen.borrow_mut().push(r));
        assert_eq!(got, Ok(Ready::Running));
    }
    let seen = seen.into_inner();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|r| matches!(r, Report::Detected(_))), "{seen:?}");
}

#[test]
fn starts_steam_and_reports_each_stage_once() {
    let mut seen = Vec::new();
    let starts = Cell::new(0);
    let got = ensure(
        scripted(&[
            State::Stopped,
            State::Stopped,
            State::Starting,
            State::Unconfirmed,
            State::SigningIn,
            State::Ready,
        ]),
        || {
            starts.set(starts.get() + 1);
            Command::new("true").spawn()
        },
        quick(),
        |r| seen.push(r),
    );
    assert_eq!(got, Ok(Ready::Started));
    assert_eq!(starts.get(), 1);
    assert!(matches!(seen[0], Report::Detected(s) if s == Status::STOPPED));
    assert_eq!(stages(&seen), [Stage::Starting, Stage::SigningIn]);
    assert_eq!(
        changes(&seen),
        [
            State::Starting,
            State::Unconfirmed,
            State::SigningIn,
            State::Ready
        ]
    );
}

#[test]
fn waits_for_a_steam_already_starting_without_starting_another() {
    let mut seen = Vec::new();
    let got = ensure(
        scripted(&[State::Starting, State::SigningIn, State::Ready]),
        never,
        quick(),
        |r| seen.push(r),
    );
    assert_eq!(got, Ok(Ready::Started));
    assert_eq!(stages(&seen), [Stage::Starting, Stage::SigningIn]);
}

#[test]
fn gives_up_after_the_timeout() {
    let spawn = || {
        Command::new("sleep")
            .arg("5")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    let got = ensure(scripted(&[State::Stopped]), spawn, quick(), |_| {});
    assert_eq!(got, Err(StartError::Timeout(0)));
    assert_eq!(
        StartError::Timeout(90).to_string(),
        "Steam didn't start within 90 s; open Steam and press Play again"
    );
    let got = ensure(
        scripted(&[State::Stopped, State::SigningIn]),
        spawn,
        quick(),
        |_| {},
    );
    assert_eq!(got, Err(StartError::SignedOut));
    let got = ensure(
        scripted(&[State::Stopped, State::Unconfirmed]),
        spawn,
        quick(),
        |_| {},
    );
    assert_eq!(got, Ok(Ready::Unconfirmed));
}

#[test]
fn says_why_steam_could_not_start() {
    let got = ensure(
        scripted(&[State::Stopped]),
        || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Steam was not found",
            ))
        },
        quick(),
        |_| {},
    );
    assert_eq!(
        got.unwrap_err().to_string(),
        "could not start Steam (Steam was not found); open Steam and press Play again"
    );
}

#[cfg(unix)]
#[test]
fn reports_a_steam_that_exits_with_an_error() {
    let wait = Wait {
        timeout: Duration::from_secs(10),
        poll: Duration::from_millis(50),
    };
    let started = Instant::now();
    let got = ensure(
        scripted(&[State::Stopped]),
        || Command::new("sh").args(["-c", "exit 3"]).spawn(),
        wait,
        |_| {},
    );
    assert_eq!(
        got,
        Err(StartError::Spawn("Steam exited with exit status: 3".into()))
    );
    assert!(started.elapsed() < Duration::from_secs(6));
}

#[cfg(target_os = "linux")]
mod fake_steam {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct Fake {
        home: Home,
        bin: PathBuf,
        _turn: std::sync::MutexGuard<'static, ()>,
    }

    impl Fake {
        fn new(name: &str, body: &str) -> Self {
            let turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
            let home = Home::new(name);
            let bin = home.0.join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            let script = bin.join("steam");
            let body = body.replace("$FAKE_HOME", &home.0.display().to_string());
            std::fs::write(&script, format!("#!/bin/sh\n{body}\nwhile :; do sleep 1; done\n")).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self {
                home,
                bin,
                _turn: turn,
            }
        }

        fn starter(&self) -> Starter {
            let path = std::env::join_paths([&self.bin]).unwrap();
            linux_starter(Some(&path), &self.home.0, &[]).unwrap()
        }

        fn status(&self) -> Status {
            let ours: Vec<Proc> = processes()
                .into_iter()
                .filter(|p| crate::platform::process::descends_from(p.pid, std::process::id()))
                .collect();
            linux_status(&self.home.0, &ours)
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let since = Instant::now();
            while let Some(pid) = self.status().pid {
                Command::new("kill")
                    .args(["-TERM", "--", &format!("-{pid}")])
                    .status()
                    .ok();
                if since.elapsed() > Duration::from_secs(5) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    #[test]
    fn starts_a_fake_steam_and_waits_for_it_to_sign_in() {
        let fake = Fake::new(
            "signs-in",
            "sleep 0.3\necho $$ > \"$FAKE_HOME/.steam/steam.pid\"\nsleep 0.3\n\
                 printf '\"Registry\" { \"HKCU\" { \"Software\" { \"Valve\" { \"Steam\" { \"ActiveProcess\" { \"pid\" \"%s\" \"ActiveUser\" \"42\" } } } } } }' $$ > \"$FAKE_HOME/.steam/registry.vdf\"",
        );
        assert_eq!(fake.status(), Status::STOPPED);
        let starter = fake.starter();
        assert_eq!(starter, Starter::Exe(fake.bin.join("steam")));
        let mut seen = Vec::new();
        let wait = Wait {
            timeout: Duration::from_secs(20),
            poll: Duration::from_millis(50),
        };
        let got = ensure(|| fake.status(), || starter.start(), wait, |r| seen.push(r));
        assert_eq!(got, Ok(Ready::Started));
        assert_eq!(stages(&seen), [Stage::Starting, Stage::SigningIn]);
        let Some(Report::Changed(last, after)) = seen.last() else {
            panic!("{seen:?}");
        };
        assert_eq!((last.state, last.signal), (State::Ready, Signal::ActiveUser(42)));
        assert!(*after >= Duration::from_millis(500), "{after:?}");
        let up = fake.status();
        assert_eq!(up.state, State::Ready);
        let again = ensure(
            || fake.status(),
            never,
            wait,
            |r| assert!(matches!(r, Report::Detected(_)), "{r:?}"),
        );
        assert_eq!(again, Ok(Ready::Running));
        assert!(up.pid.is_some());
    }

    #[test]
    fn times_out_on_a_fake_steam_that_never_comes_up() {
        let fake = Fake::new("never", "true");
        let starter = fake.starter();
        let wait = Wait {
            timeout: Duration::from_millis(1500),
            poll: Duration::from_millis(50),
        };
        let started = Instant::now();
        let got = ensure(|| fake.status(), || starter.start(), wait, |_| {});
        assert_eq!(got, Err(StartError::Timeout(1)));
        assert!(started.elapsed() >= Duration::from_millis(1500));
    }
}
