//! Whether this process runs as administrator, and starting it again as administrator through the elevation
//! prompt.

use super::tools::windows_tool;

/// True when this process runs with a high (or system) integrity level.
pub fn elevated() -> bool {
    windows_tool("whoami", &["/groups", "/fo", "csv", "/nh"])
        .is_some_and(|groups| groups.contains("S-1-16-12288") || groups.contains("S-1-16-16384"))
}

#[cfg(windows)]
mod shell {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct ShellExecuteInfoW {
        size: u32,
        mask: u32,
        hwnd: *mut c_void,
        verb: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
        instance: *mut c_void,
        id_list: *mut c_void,
        class: *const u16,
        key_class: *mut c_void,
        hot_key: u32,
        icon_or_monitor: *mut c_void,
        process: *mut c_void,
    }

    const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
    const SEE_MASK_NOASYNC: u32 = 0x100;
    const SW_SHOWNORMAL: i32 = 1;
    const ERROR_CANCELLED: i32 = 1223;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteExW(info: *mut ShellExecuteInfoW) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
        fn CoUninitialize();
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetProcessId(process: *mut c_void) -> u32;
        fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
        fn GetExitCodeProcess(process: *mut c_void, code: *mut u32) -> i32;
    }

    const WAIT_OBJECT_0: u32 = 0;

    pub struct Process(*mut c_void);

    // SAFETY: a process handle can be waited on and closed from any thread.
    unsafe impl Send for Process {}

    impl Process {
        pub fn pid(&self) -> u32 {
            // SAFETY: the handle is open until drop.
            unsafe { GetProcessId(self.0) }
        }

        pub fn exited_within(&self, wait: std::time::Duration) -> Option<u32> {
            let ms = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: the handle is open until drop, and `code` outlives the call.
            unsafe {
                if WaitForSingleObject(self.0, ms) != WAIT_OBJECT_0 {
                    return None;
                }
                let mut code = 0;
                (GetExitCodeProcess(self.0, &mut code) != 0).then_some(code)
            }
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the handle came from ShellExecuteExW with NOCLOSEPROCESS and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn quote(arg: &str) -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_owned();
        }
        let mut inner = arg.replace('"', "\\\"");
        let trailing = inner.chars().rev().take_while(|&c| c == '\\').count();
        inner.push_str(&"\\".repeat(trailing));
        format!("\"{inner}\"")
    }

    pub fn relaunch_elevated() -> Result<Option<Process>, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args: Vec<String> = std::env::args().skip(1).map(|a| quote(&a)).collect();
        let (verb, file) = (wide("runas".as_ref()), wide(exe.as_os_str()));
        let parameters = wide(args.join(" ").as_ref());
        let directory = std::env::current_dir().ok().map(|d| wide(d.as_os_str()));
        let mut info = ShellExecuteInfoW {
            size: std::mem::size_of::<ShellExecuteInfoW>() as u32,
            mask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
            hwnd: std::ptr::null_mut(),
            verb: verb.as_ptr(),
            file: file.as_ptr(),
            parameters: parameters.as_ptr(),
            directory: directory.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            show: SW_SHOWNORMAL,
            instance: std::ptr::null_mut(),
            id_list: std::ptr::null_mut(),
            class: std::ptr::null(),
            key_class: std::ptr::null_mut(),
            hot_key: 0,
            icon_or_monitor: std::ptr::null_mut(),
            process: std::ptr::null_mut(),
        };
        // SAFETY: COM is initialised for this thread only and uninitialised after the call when it succeeded,
        // and every string the struct points at outlives the call.
        let ok = unsafe {
            let com = CoInitializeEx(
                std::ptr::null_mut(),
                COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
            ) >= 0;
            let ok = ShellExecuteExW(&mut info) != 0;
            if com {
                CoUninitialize();
            }
            ok
        };
        if !ok {
            let e = std::io::Error::last_os_error();
            return Err(if e.raw_os_error() == Some(ERROR_CANCELLED) {
                "the administrator prompt was declined".to_owned()
            } else {
                e.to_string()
            });
        }
        Ok((!info.process.is_null()).then_some(Process(info.process)))
    }

    #[cfg(test)]
    #[test]
    fn quotes_arguments_with_spaces() {
        assert_eq!(quote("run"), "run");
        assert_eq!(quote("C:\\Program Files\\F1 25"), "\"C:\\Program Files\\F1 25\"");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("C:\\My Games\\"), "\"C:\\My Games\\\\\"");
    }
}

/// The copy of this program [`relaunch_elevated`] started as administrator.
pub struct Elevated {
    #[cfg(windows)]
    pub(super) process: Option<shell::Process>,
}

impl Elevated {
    pub fn pid(&self) -> Option<u32> {
        #[cfg(windows)]
        return self.process.as_ref().map(shell::Process::pid);
        #[cfg(not(windows))]
        None
    }

    /// Its exit code, when it closed within `wait`.
    pub fn exited_within(&self, wait: std::time::Duration) -> Option<u32> {
        #[cfg(windows)]
        return self.process.as_ref()?.exited_within(wait);
        #[cfg(not(windows))]
        {
            let _ = wait;
            None
        }
    }
}

/// Starts this program again as administrator, with the same arguments,
/// through the Windows elevation prompt. Ok with the new process once it has
/// started; the caller waits a moment before it exits.
pub fn relaunch_elevated() -> Result<Elevated, String> {
    #[cfg(windows)]
    return shell::relaunch_elevated().map(|process| Elevated { process });
    #[cfg(not(windows))]
    Err("restarting as administrator is only for Windows".to_owned())
}
