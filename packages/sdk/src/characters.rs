use std::ffi::{c_char, c_int};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCharacters {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub career_group: Option<unsafe extern "C" fn(out: *mut c_char, cap: usize) -> c_int>,
    pub swap: Option<
        unsafe extern "C" fn(
            group: *const c_char,
            character: *const c_char,
            replacement: *const c_char,
        ) -> c_int,
    >,
    pub mute: Option<unsafe extern "C" fn(group: *const c_char, speaker: *const c_char, on: c_int) -> c_int>,
    pub string_set: Option<
        unsafe extern "C" fn(
            group: *const c_char,
            language: *const c_char,
            key: *const c_char,
            text: *const c_char,
        ) -> c_int,
    >,
    pub strings_json:
        Option<unsafe extern "C" fn(group: *const c_char, json: *const c_char, len: usize) -> c_int>,
    pub clear: Option<unsafe extern "C" fn(group: *const c_char) -> c_int>,
    pub refresh: Option<unsafe extern "C" fn() -> c_int>,
}
