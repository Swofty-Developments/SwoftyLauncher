use std::ffi::{c_char, c_int};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyLang {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub override_set: Option<
        unsafe extern "C" fn(language: *const c_char, key: *const c_char, text: *const c_char) -> c_int,
    >,
    pub override_clear: Option<unsafe extern "C" fn(language: *const c_char, key: *const c_char) -> c_int>,
    pub override_clear_all: Option<unsafe extern "C" fn() -> c_int>,
    pub refresh: Option<unsafe extern "C" fn() -> c_int>,
}
