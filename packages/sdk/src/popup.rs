use std::ffi::{c_char, c_int};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyPopup {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub show: Option<unsafe extern "C" fn(title: *const c_char, body: *const c_char) -> c_int>,
}
