//! Whether a process runs elevated, and its exit code once it has closed, read through its handle.

pub fn own_elevation() -> Option<bool> {
    elevation(std::process::id())
}

pub struct ExitProbe(#[cfg(windows)] *mut std::ffi::c_void);

#[cfg(windows)]
mod token {
    use std::ffi::c_void;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const TOKEN_QUERY: u32 = 0x8;
    pub const TOKEN_ELEVATION: u32 = 20;
    pub const STILL_ACTIVE: u32 = 259;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        pub fn GetExitCodeProcess(process: *mut c_void, code: *mut u32) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        pub fn GetTokenInformation(
            token: *mut c_void,
            class: u32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
    }
}

#[cfg(windows)]
pub fn elevation(pid: u32) -> Option<bool> {
    use std::ffi::c_void;
    use token::*;
    // SAFETY: each handle is checked before use and closed once, and the
    // elevation flag is a u32 whose size is passed to the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut handle: *mut c_void = std::ptr::null_mut();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut handle) != 0;
        CloseHandle(process);
        if !opened {
            return None;
        }
        let (mut flag, mut returned) = (0u32, 0u32);
        let ok = GetTokenInformation(
            handle,
            TOKEN_ELEVATION,
            (&mut flag as *mut u32).cast(),
            4,
            &mut returned,
        );
        CloseHandle(handle);
        (ok != 0).then_some(flag != 0)
    }
}

#[cfg(not(windows))]
pub fn elevation(_: u32) -> Option<bool> {
    None
}

#[cfg(windows)]
impl ExitProbe {
    pub fn open(pid: u32) -> std::io::Result<Self> {
        // SAFETY: the handle is checked before it is kept; Drop closes it once.
        let handle = unsafe { token::OpenProcess(token::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    pub fn exit_code(&self) -> Option<u32> {
        let mut code = 0u32;
        // SAFETY: the handle is open until Drop and `code` is a u32.
        let ok = unsafe { token::GetExitCodeProcess(self.0, &mut code) };
        (ok != 0 && code != token::STILL_ACTIVE).then_some(code)
    }
}

#[cfg(windows)]
impl Drop for ExitProbe {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed only here.
        unsafe { token::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
impl ExitProbe {
    pub fn open(_: u32) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "only Windows reads the exit code of a game the session did not start",
        ))
    }

    pub fn exit_code(&self) -> Option<u32> {
        None
    }
}
