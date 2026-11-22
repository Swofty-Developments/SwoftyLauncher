//! Whether F1 25 run without administrator rights can create files in a folder, read from the folder's
//! security descriptor with this process's token, its Administrators group set to deny only and its
//! privileges taken out: the rights the EA app's copy of the game has when Swofty Launcher runs as
//! administrator.

use std::path::Path;

#[cfg(windows)]
mod token {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;

    pub const TOKEN_DUPLICATE: u32 = 0x2;
    pub const TOKEN_QUERY: u32 = 0x8;
    pub const DISABLE_MAX_PRIVILEGE: u32 = 0x1;
    pub const SECURITY_IDENTIFICATION: u32 = 1;
    pub const WIN_BUILTIN_ADMINISTRATORS_SID: u32 = 26;
    pub const SECURITY_MAX_SID_SIZE: u32 = 68;
    pub const OWNER_GROUP_DACL: u32 = 0x1 | 0x2 | 0x4;
    pub const FILE_ADD_FILE: u32 = 0x2;
    pub const FILE_ADD_SUBDIRECTORY: u32 = 0x4;

    #[repr(C)]
    pub struct SidAndAttributes {
        pub sid: *mut c_void,
        pub attributes: u32,
    }

    #[repr(C)]
    pub struct GenericMapping {
        pub read: u32,
        pub write: u32,
        pub execute: u32,
        pub all: u32,
    }

    pub static FILE_MAPPING: GenericMapping = GenericMapping {
        read: 0x0012_0089,
        write: 0x0012_0116,
        execute: 0x0012_00a0,
        all: 0x001f_01ff,
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetCurrentProcess() -> Handle;
        pub fn CloseHandle(handle: Handle) -> i32;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        pub fn CreateWellKnownSid(kind: u32, domain: *mut c_void, sid: *mut c_void, size: *mut u32) -> i32;
        pub fn CreateRestrictedToken(
            existing: Handle,
            flags: u32,
            disable_count: u32,
            disable: *const SidAndAttributes,
            delete_count: u32,
            delete: *const c_void,
            restrict_count: u32,
            restrict: *const SidAndAttributes,
            new: *mut Handle,
        ) -> i32;
        pub fn DuplicateToken(existing: Handle, level: u32, new: *mut Handle) -> i32;
        pub fn GetFileSecurityW(
            name: *const u16,
            info: u32,
            sd: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        pub fn AccessCheck(
            sd: *const c_void,
            token: Handle,
            desired: u32,
            mapping: *const GenericMapping,
            privileges: *mut c_void,
            privileges_len: *mut u32,
            granted: *mut u32,
            status: *mut i32,
        ) -> i32;
    }

    pub struct Token(pub Handle);

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the handle came from a call that opened it and is closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn failed(call: &str) -> String {
        format!("{call}: {}", std::io::Error::last_os_error())
    }
}

/// [`can_create_in`]'s token: this process's, with the Administrators group
/// set to deny only and every privilege but bypass traverse checking taken
/// out, as an identification token the access check reads.
#[cfg(windows)]
fn unelevated_token() -> Result<token::Token, String> {
    use std::ptr::{null, null_mut};
    use token::*;
    // SAFETY: each handle is checked before use and closed once by Token, the SID buffer is u32 aligned and
    // SECURITY_MAX_SID_SIZE bytes long, and every pointer the calls take outlives them.
    unsafe {
        let mut own = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut own) == 0 {
            return Err(failed("OpenProcessToken"));
        }
        let own = Token(own);
        let mut sid = [0u32; SECURITY_MAX_SID_SIZE as usize / 4];
        let mut size = SECURITY_MAX_SID_SIZE;
        if CreateWellKnownSid(
            WIN_BUILTIN_ADMINISTRATORS_SID,
            null_mut(),
            sid.as_mut_ptr().cast(),
            &mut size,
        ) == 0
        {
            return Err(failed("CreateWellKnownSid"));
        }
        let admins = SidAndAttributes {
            sid: sid.as_mut_ptr().cast(),
            attributes: 0,
        };
        let mut restricted = null_mut();
        if CreateRestrictedToken(
            own.0,
            DISABLE_MAX_PRIVILEGE,
            1,
            &admins,
            0,
            null(),
            0,
            null(),
            &mut restricted,
        ) == 0
        {
            return Err(failed("CreateRestrictedToken"));
        }
        let restricted = Token(restricted);
        let mut client = null_mut();
        if DuplicateToken(restricted.0, SECURITY_IDENTIFICATION, &mut client) == 0 {
            return Err(failed("DuplicateToken"));
        }
        Ok(Token(client))
    }
}

/// The owner, group and DACL of `path`, as a self-relative security
/// descriptor.
#[cfg(windows)]
fn security_of(path: &Path) -> Result<Vec<u64>, String> {
    use std::os::windows::ffi::OsStrExt;
    use token::*;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut needed = 0u32;
    // SAFETY: name is NUL-terminated, and a null buffer of length 0 only asks for the size.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            OWNER_GROUP_DACL,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    if needed == 0 {
        return Err(failed("GetFileSecurityW"));
    }
    let mut sd = vec![0u64; (needed as usize).div_ceil(8)];
    let len = u32::try_from(sd.len() * 8).map_err(|e| e.to_string())?;
    // SAFETY: sd is len bytes long and 8 aligned, and name is NUL-terminated.
    let ok = unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            OWNER_GROUP_DACL,
            sd.as_mut_ptr().cast(),
            len,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(failed("GetFileSecurityW"));
    }
    Ok(sd)
}

/// Whether F1 25 run without administrator rights, as this Windows user,
/// can create files and folders in `dir`. Err says which call failed when
/// that can't be read.
#[cfg(windows)]
pub(super) fn can_create_in(dir: &Path) -> Result<bool, String> {
    use token::*;
    let client = unelevated_token()?;
    let sd = security_of(dir)?;
    let mut privileges = [0u32; 64];
    let mut privileges_len = (privileges.len() * 4) as u32;
    let (mut granted, mut status) = (0u32, 0i32);
    // SAFETY: sd holds the self-relative security descriptor GetFileSecurityW wrote, the token is open until
    // client drops, and the privilege buffer is privileges_len bytes long.
    let ok = unsafe {
        AccessCheck(
            sd.as_ptr().cast(),
            client.0,
            FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
            &FILE_MAPPING,
            privileges.as_mut_ptr().cast(),
            &mut privileges_len,
            &mut granted,
            &mut status,
        )
    };
    if ok == 0 {
        return Err(failed("AccessCheck"));
    }
    Ok(status != 0)
}

/// Elsewhere the game runs as this user, with its rights.
#[cfg(not(windows))]
pub(super) fn can_create_in(dir: &Path) -> Result<bool, String> {
    Ok(super::write_access::can_create_in(dir))
}
