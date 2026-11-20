use super::paths::{norm, tidy};
use super::*;
use crate::platform::registry::{self, Ask, Data, Key};
use crate::platform::run_as_admin::windows_tool_within;

const SESSION_MANAGER: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager";
const EXEC_OPTIONS: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\F1_25.exe";
const CI_POLICY: &str = r"SYSTEM\CurrentControlSet\Control\CI\Policy";
const EXPLOIT_POLICIES: &[&str] = &[
    r"SOFTWARE\Policies\Microsoft\Windows Defender ExploitGuard\Exploit Protection",
    r"SOFTWARE\Microsoft\PolicyManager\current\device\ExploitGuard",
];
const ANTIVIRUS: &str = r"SOFTWARE\Microsoft\Security Center\Provider\Av";

pub fn fill(f: &mut Findings, game: &Path, running: &[GameProcess], since: SystemTime) {
    std::thread::scope(|s| {
        let ci = s.spawn(|| read_log(CI_LOG, CI_IDS, since, game));
        let defender = s.spawn(|| read_log(DEFENDER_LOG, DEFENDER_IDS, since, game));
        let antivirus = s.spawn(antivirus);
        let windows = std::env::var("SystemRoot").ok();
        f.games = running
            .iter()
            .take(MAX_GAMES)
            .map(|g| seen(g, game, windows.as_deref()))
            .collect();
        f.search = Some(search());
        f.exec_options = Some(exec_options());
        f.prefer_from = Some(prefer_from(&game.join(GAME_EXE)));
        f.layers = Some(Ok(crate::platform::run_as_admin::layers(&game.join(GAME_EXE))
            .into_iter()
            .map(|l| format!("{} ({})", l.layers.trim(), l.scope()))
            .collect()));
        f.starter_layers = Some(crate::platform::run_as_admin::starter_layers(game));
        f.ea_app = crate::platform::process::named(crate::platform::ea_app::PROCESS)
            .iter()
            .take(MAX_GAMES)
            .map(|a| ea_app(a.pid, game, windows.as_deref()))
            .collect();
        f.smart_app_control = Some(smart_app_control());
        let joined = |h: std::thread::ScopedJoinHandle<'_, Probe<Vec<Event>>>| {
            h.join().unwrap_or_else(|_| Err("the check failed".into()))
        };
        f.logs = vec![
            EventLog {
                name: "Code Integrity",
                read: joined(ci),
            },
            EventLog {
                name: "Defender",
                read: joined(defender),
            },
        ];
        f.antivirus = Some(
            antivirus
                .join()
                .unwrap_or_else(|_| Err("the check failed".into())),
        );
    });
}

fn error(code: i32) -> String {
    match code {
        2 => "not found".into(),
        5 => "access denied".into(),
        299 => "only part of it could be read".into(),
        n => format!("Windows error {n}"),
    }
}

fn seen(g: &GameProcess, game: &Path, windows: Option<&str>) -> Seen {
    let (modules, mitigation) = match win32::Process::open(g.pid) {
        Ok(p) => (
            p.modules()
                .map(|paths| modules(&paths, game, windows))
                .map_err(error),
            p.mitigation().map_err(error),
        ),
        Err(code) => (Err(error(code)), Err(error(code))),
    };
    Seen {
        mitigation: Some(mitigation),
        ..seen_base(g, game, modules)
    }
}

fn ea_app(pid: u32, game: &Path, windows: Option<&str>) -> EaApp {
    let paths = win32::Process::open(pid).and_then(|p| p.modules()).map_err(error);
    EaApp {
        pid,
        image: paths.as_ref().ok().and_then(|p| p.first().cloned()),
        shims: paths.map(|p| modules(&p, game, windows).shims),
    }
}

/// The key under HKEY_LOCAL_MACHINE (the 64-bit view), None when it does not exist.
fn hklm(path: &str) -> Result<Option<Key>, i32> {
    registry::open(&format!(r"HKLM\{path}"), Ask::default())
}

fn search() -> Probe<Search> {
    let known = hklm(&format!(r"{SESSION_MANAGER}\KnownDLLs")).map_err(error)?;
    let known_dll = known.is_some_and(|k| {
        k.values
            .iter()
            .any(|(_, v)| matches!(v, Data::Sz(t) | Data::ExpandSz(t) if t.eq_ignore_ascii_case(PROXY)))
    });
    let safe_search = hklm(SESSION_MANAGER)
        .map_err(error)?
        .and_then(|k| k.dword("SafeDllSearchMode"));
    Ok(Search {
        known_dll,
        safe_search,
    })
}

fn exec_options() -> Probe<Vec<String>> {
    let Some(key) = hklm(EXEC_OPTIONS).map_err(error)? else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = key.values.into_iter().map(|(n, _)| n).collect();
    for sub in &key.subkeys {
        if let Ok(Some(k)) = hklm(&format!(r"{EXEC_OPTIONS}\{sub}")) {
            names.extend(k.values.into_iter().map(|(n, _)| format!(r"{sub}\{n}")));
        }
    }
    Ok(names)
}

fn prefer_from(exe: &Path) -> Probe<Vec<PreferSource>> {
    let on = |k: &Key| k.bytes("MitigationOptions").is_some_and(|b| prefers_system32(&b));
    let mut found = Vec::new();
    if let Some(key) = hklm(EXEC_OPTIONS).map_err(error)? {
        for sub in &key.subkeys {
            let Ok(Some(k)) = hklm(&format!(r"{EXEC_OPTIONS}\{sub}")) else {
                continue;
            };
            if let Some(path) = k.text("FilterFullPath").filter(|p| same_file(p, exe))
                && on(&k)
            {
                found.push(PreferSource::Path { path: tidy(&path) });
            }
        }
        if on(&key) {
            found.push(PreferSource::Program);
        }
    }
    for policy in EXPLOIT_POLICIES {
        if let Ok(Some(k)) = hklm(policy)
            && let Some(source) = k
                .text("ExploitProtectionSettings")
                .and_then(|t| policy_source(&t))
        {
            found.push(source);
        }
    }
    let kernel = hklm(&format!(r"{SESSION_MANAGER}\kernel")).map_err(error)?;
    if kernel.is_some_and(|k| on(&k)) {
        found.push(PreferSource::System);
    }
    Ok(found)
}

fn same_file(path: &str, exe: &Path) -> bool {
    norm(path) == norm(&exe.to_string_lossy())
}

fn smart_app_control() -> Probe<Option<u32>> {
    Ok(hklm(CI_POLICY)
        .map_err(error)?
        .and_then(|k| k.dword("VerifiedAndReputablePolicyState")))
}

fn antivirus() -> Probe<Vec<String>> {
    let listed: Vec<String> = match hklm(ANTIVIRUS) {
        Ok(Some(key)) => key
            .subkeys
            .iter()
            .filter_map(|sub| {
                let k = hklm(&format!(r"{ANTIVIRUS}\{sub}")).ok()??;
                let name = k.text("DISPLAYNAME")?;
                Some(antivirus_name(&name, k.dword("STATE")))
            })
            .collect(),
        _ => Vec::new(),
    };
    if !listed.is_empty() {
        return Ok(listed);
    }
    let out = windows_tool_within(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | \
             ForEach-Object { $_.displayName + '|' + $_.productState }",
        ],
        TOOL_WAIT,
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let (name, state) = l.trim().rsplit_once('|')?;
            Some(antivirus_name(name.trim(), state.trim().parse().ok()))
        })
        .collect())
}

fn read_log(log: &str, ids: &[u32], since: SystemTime, game: &Path) -> Probe<Vec<Event>> {
    let window = since.elapsed().unwrap_or_default() + EVENT_MARGIN;
    let ids = ids
        .iter()
        .map(|i| format!("EventID={i}"))
        .collect::<Vec<_>>()
        .join(" or ");
    let query = format!(
        "/q:*[System[({ids}) and TimeCreated[timediff(@SystemTime) <= {}]]]",
        window.as_millis()
    );
    let out = windows_tool_within(
        "wevtutil",
        &["qe", log, &query, "/f:xml", "/rd:true", "/c:50"],
        TOOL_WAIT,
    )?;
    Ok(events(&out, game))
}

mod win32 {
    use std::ffi::c_void;

    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const PROCESS_VM_READ: u32 = 0x0010;
    const LIST_MODULES_ALL: u32 = 0x03;
    const PROCESS_SIGNATURE_POLICY: i32 = 8;
    const PROCESS_IMAGE_LOAD_POLICY: i32 = 10;
    const ERROR_PARTIAL_COPY: i32 = 299;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn K32EnumProcessModulesEx(
            process: *mut c_void,
            modules: *mut *mut c_void,
            size: u32,
            needed: *mut u32,
            filter: u32,
        ) -> i32;
        fn K32GetModuleFileNameExW(
            process: *mut c_void,
            module: *mut c_void,
            name: *mut u16,
            size: u32,
        ) -> u32;
        fn GetProcessMitigationPolicy(
            process: *mut c_void,
            policy: i32,
            buffer: *mut c_void,
            size: usize,
        ) -> i32;
    }

    fn last_error() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    pub struct Process(*mut c_void);

    impl Process {
        pub fn open(pid: u32) -> Result<Self, i32> {
            // SAFETY: OpenProcess takes no pointers; a null handle is
            // the failure, read before any other call.
            let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
            if handle.is_null() {
                return Err(last_error());
            }
            Ok(Self(handle))
        }

        /// The full paths of the modules loaded in the process. A
        /// process still mapping its modules can give a partial copy,
        /// which a second look a moment later gets past.
        pub fn modules(&self) -> Result<Vec<String>, i32> {
            let mut tries = 0;
            loop {
                match self.module_handles() {
                    Err(ERROR_PARTIAL_COPY) if tries < 3 => {
                        tries += 1;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => return Err(e),
                    Ok(handles) => {
                        return Ok(handles.into_iter().filter_map(|m| self.module_path(m)).collect());
                    }
                }
            }
        }

        fn module_handles(&self) -> Result<Vec<*mut c_void>, i32> {
            let mut handles: Vec<*mut c_void> = vec![std::ptr::null_mut(); 1024];
            loop {
                let size = (handles.len() * std::mem::size_of::<*mut c_void>()) as u32;
                let mut needed = 0u32;
                // SAFETY: `handles` holds `size` bytes of module slots,
                // and `needed` is a valid out pointer.
                let ok = unsafe {
                    K32EnumProcessModulesEx(self.0, handles.as_mut_ptr(), size, &mut needed, LIST_MODULES_ALL)
                };
                if ok == 0 {
                    return Err(last_error());
                }
                let count = needed as usize / std::mem::size_of::<*mut c_void>();
                if needed <= size {
                    handles.truncate(count);
                    return Ok(handles);
                }
                handles.resize(count + 64, std::ptr::null_mut());
            }
        }

        fn module_path(&self, module: *mut c_void) -> Option<String> {
            let mut name = vec![0u16; 32_768];
            // SAFETY: `name` holds the u16 count passed, and the call
            // writes at most that many including the NUL.
            let len =
                unsafe { K32GetModuleFileNameExW(self.0, module, name.as_mut_ptr(), name.len() as u32) };
            (len > 0).then(|| String::from_utf16_lossy(&name[..len as usize]))
        }

        pub fn mitigation(&self) -> Result<super::Mitigation, i32> {
            let flags = |policy: i32| -> Result<u32, i32> {
                let mut flags = 0u32;
                // SAFETY: both policies fill a single DWORD of flags,
                // the size passed.
                let ok =
                    unsafe { GetProcessMitigationPolicy(self.0, policy, (&mut flags as *mut u32).cast(), 4) };
                if ok == 0 { Err(last_error()) } else { Ok(flags) }
            };
            let signature = flags(PROCESS_SIGNATURE_POLICY)?;
            let image = flags(PROCESS_IMAGE_LOAD_POLICY)?;
            Ok(super::Mitigation {
                microsoft_signed: signature & 0x1 != 0,
                store_signed: signature & 0x2 != 0,
                no_remote: image & 0x1 != 0,
                no_low_label: image & 0x2 != 0,
                prefer_system32: image & 0x4 != 0,
            })
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenProcess and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}
