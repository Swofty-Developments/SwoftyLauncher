use std::ffi::{c_char, c_int};

pub const CAREER_SAVES_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyText {
    pub data: *const c_char,
    pub size: usize,
}

/// The core's career saves table (export `SwoftyCoreCareerSaves`, since 1.7.0).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerSaves {
    pub version: u32,
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub index: Option<unsafe extern "C" fn(json: *mut c_char, cap: usize, len: *mut usize) -> c_int>,
    pub current: Option<unsafe extern "C" fn(id: *mut c_char, cap: usize, len: *mut usize) -> c_int>,
    pub describe: Option<unsafe extern "C" fn(request_json: SwoftyText) -> c_int>,
    pub need:
        Option<unsafe extern "C" fn(addon: *const c_char, range: *const c_char, why: *const c_char) -> c_int>,
    pub data_get: Option<
        unsafe extern "C" fn(
            career: *const c_char,
            addon: *const c_char,
            json: *mut c_char,
            cap: usize,
            len: *mut usize,
            version: *mut u32,
        ) -> c_int,
    >,
    pub data_set: Option<
        unsafe extern "C" fn(
            career: *const c_char,
            addon: *const c_char,
            version: u32,
            json: SwoftyText,
        ) -> c_int,
    >,
    pub data_remove: Option<unsafe extern "C" fn(career: *const c_char, addon: *const c_char) -> c_int>,
    pub missing: Option<unsafe extern "C" fn(json: *mut c_char, cap: usize, len: *mut usize) -> c_int>,
}
