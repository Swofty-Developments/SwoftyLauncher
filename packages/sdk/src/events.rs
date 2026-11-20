use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyEvent {
    pub name: *const c_char,
    pub frame: u64,
    pub time: f64,
    pub a: i64,
    pub b: i64,
    pub data: *const c_void,
}

pub type SwoftyEventFn = unsafe extern "C" fn(ev: *const SwoftyEvent, user: *mut c_void);
pub type SwoftyFrameFn = unsafe extern "C" fn(dt: f64, user: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyEvents {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub subscribe: Option<
        unsafe extern "C" fn(
            event: *const c_char,
            f: Option<SwoftyEventFn>,
            user: *mut c_void,
            token: *mut u32,
        ) -> c_int,
    >,
    pub on_frame:
        Option<unsafe extern "C" fn(f: Option<SwoftyFrameFn>, user: *mut c_void, token: *mut u32) -> c_int>,
    pub unsubscribe: Option<unsafe extern "C" fn(token: u32) -> c_int>,
}
