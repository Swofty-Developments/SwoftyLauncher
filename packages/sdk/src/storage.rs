use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyStorage {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub read: Option<
        unsafe extern "C" fn(path: *const c_char, buf: *mut c_void, cap: usize, len: *mut usize) -> c_int,
    >,
    pub write: Option<unsafe extern "C" fn(path: *const c_char, data: *const c_void, len: usize) -> c_int>,
    pub append: Option<unsafe extern "C" fn(path: *const c_char, data: *const c_void, len: usize) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(path: *const c_char) -> c_int>,
    pub size: Option<unsafe extern "C" fn(path: *const c_char, out: *mut u64) -> c_int>,
    pub list: Option<
        unsafe extern "C" fn(dir: *const c_char, buf: *mut c_char, cap: usize, len: *mut usize) -> c_int,
    >,
}
