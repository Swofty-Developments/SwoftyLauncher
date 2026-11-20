use std::ffi::{c_char, c_int};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerWizard {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub group: Option<unsafe extern "C" fn(out: *mut c_char, cap: usize) -> c_int>,
    pub starts_in_f1: Option<unsafe extern "C" fn(group: *const c_char, on: c_int) -> c_int>,
    pub tile_route:
        Option<unsafe extern "C" fn(group: *const c_char, from: *const c_char, to: *const c_char) -> c_int>,
    pub replace_step: Option<unsafe extern "C" fn(group: *const c_char, step: u32, with: u32) -> c_int>,
    pub force_option: Option<unsafe extern "C" fn(group: *const c_char, option: u32, with: u32) -> c_int>,
    pub confirm_route:
        Option<unsafe extern "C" fn(group: *const c_char, from: *const c_char, to: *const c_char) -> c_int>,
    pub hide_driver_tab:
        Option<unsafe extern "C" fn(group: *const c_char, tab: *const c_char, on: c_int) -> c_int>,
    pub clear: Option<unsafe extern "C" fn(group: *const c_char) -> c_int>,
}
