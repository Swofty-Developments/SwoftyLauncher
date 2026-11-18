use std::ffi::{c_char, c_int, c_void};

pub const EVENT_CAREER_CONTENT_PRESSED: &str = "career.content.pressed";
pub const CAREER_CONTENT_MODEL_2026: u32 = 0;
pub const CAREER_CONTENT_MODEL_2025: u32 = 1;
pub const CAREER_CONTENT_DRIVER: u32 = 1;
pub const CAREER_CONTENT_MY_TEAM: u32 = 2;
pub const CAREER_CONTENT_QUEUED: u32 = 0;
pub const CAREER_CONTENT_LIVE: u32 = 1;
pub const CAREER_CONTENT_OFF: u32 = 2;
pub const CAREER_CONTENT_EXPORT: &str = "SwoftyCoreCareerContent";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerContentDesc {
    pub size: u32,
    pub model: u32,
    pub id: *const c_char,
    pub title: *const c_char,
    pub description: *const c_char,
    pub data_group: *const c_char,
    pub suffix: *const c_char,
    pub mode: *const c_char,
    pub entitlement: *const c_char,
    pub car_group: i32,
    pub badge: u32,
    pub careers: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerContentInfo {
    pub index: u32,
    pub state: u32,
    pub mode_stat_id: i64,
    pub data_group_index: i64,
    pub tiles: u32,
    pub presses: u32,
    pub id: [c_char; 64],
    pub data_group: [c_char; 64],
    pub why: [c_char; 192],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerContent {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub add: Option<unsafe extern "C" fn(desc: *const SwoftyCareerContentDesc) -> c_int>,
    pub info: Option<unsafe extern "C" fn(id: *const c_char, out: *mut SwoftyCareerContentInfo) -> c_int>,
    pub data_group:
        Option<unsafe extern "C" fn(career: *const c_void, out: *mut c_char, cap: usize) -> c_int>,
    pub pressed: Option<unsafe extern "C" fn(out: *mut c_char, cap: usize) -> c_int>,
}
