use std::ffi::{c_char, c_int, c_void};

pub const SETTING_TOGGLE: u32 = 1;
pub const SETTING_SLIDER: u32 = 2;
pub const SETTING_CHOICE: u32 = 3;

pub type SettingFn =
    unsafe extern "C" fn(id: *const c_char, kind: u32, out: *mut SwoftySettingValue) -> c_int;
pub type SettingChangedFn =
    unsafe extern "C" fn(id: *const c_char, value: *const SwoftySettingValue, user: *mut c_void);
pub type WatchSettingFn =
    unsafe extern "C" fn(id: *const c_char, f: Option<SettingChangedFn>, user: *mut c_void) -> c_int;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwoftySettingValue {
    pub kind: u32,
    pub toggle: i32,
    pub slider: f64,
    pub choice: *const c_char,
}
