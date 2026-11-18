use std::ffi::{c_char, c_int, c_void};

pub const HUD_VERSION: u32 = 1;
pub const HUD_TEXT: u32 = 0;
pub const HUD_BOOL: u32 = 1;
pub const HUD_INT: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyHud {
    pub version: u32,
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub toast: Option<
        unsafe extern "C" fn(
            text: *const c_char,
            detail: *const c_char,
            seconds: f32,
            icon: *const c_char,
        ) -> c_int,
    >,
    pub set: Option<unsafe extern "C" fn(path: *const c_char, kind: u32, value: *const c_void) -> c_int>,
}
