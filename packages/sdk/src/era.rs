use std::ffi::{c_char, c_int};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyEraBox {
    pub size: u32,
    pub entrant_list: *const c_char,
    pub title: *const c_char,
    pub image: *const c_char,
    pub order: i32,
    pub time_trial_entrant_list: *const c_char,
    pub only: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyEra {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub add_box: Option<unsafe extern "C" fn(box_: *const SwoftyEraBox) -> c_int>,
    pub count: Option<unsafe extern "C" fn() -> c_int>,
}
