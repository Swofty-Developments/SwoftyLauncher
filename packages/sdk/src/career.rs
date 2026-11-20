use std::ffi::{c_char, c_int, c_void};

pub const CAREER_START_PICK: u32 = 0;
pub const CAREER_START_NEW: u32 = 1;
pub const CAREER_START_CONTINUE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyFlowState {
    pub hash: u32,
    pub from: u32,
    pub to: u32,
    pub named: u32,
    pub transitions: u64,
    pub name: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerState {
    pub loaded: u32,
    pub mode: u32,
    pub hub: u32,
    pub humans: u32,
    pub player_driver: u32,
    pub player_team: u32,
    pub has_player: u32,
    pub components: u32,
    pub component_mode: u32,
    pub team_hq_mode: u32,
    pub flow_career: u32,
    pub reserved: u32,
    pub key: [c_char; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareer {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub state: Option<unsafe extern "C" fn(out: *mut SwoftyCareerState) -> c_int>,
    pub flow: Option<unsafe extern "C" fn(out: *mut SwoftyFlowState) -> c_int>,
    pub flow_status: Option<unsafe extern "C" fn(hash: u32, status: *mut u32, age_ms: *mut u64) -> c_int>,
    pub entity: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub component: Option<unsafe extern "C" fn(type_name: *const c_char) -> *mut c_void>,
    pub component_by_desc: Option<unsafe extern "C" fn(desc: *const c_void) -> *mut c_void>,
    pub post: Option<unsafe extern "C" fn(msg: *mut c_void) -> c_int>,
    pub block: Option<unsafe extern "C" fn(mode: u32, title: *const c_char, body: *const c_char) -> c_int>,
    pub unblock: Option<unsafe extern "C" fn(mode: u32) -> c_int>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCareerStart {
    pub mode: u32,
    pub kind: u32,
    pub blocked: u32,
    pub reserved: u32,
    pub route: [c_char; 64],
}
