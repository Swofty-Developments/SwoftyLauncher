use std::ffi::{c_char, c_int, c_void};

pub const ASSET_OK: c_int = 0;
pub const ASSET_LATE: c_int = 1;
pub const ASSET_UNAVAILABLE: c_int = 2;
pub const ASSET_INVALID: c_int = 3;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyFragment {
    pub file: *const c_char,
    pub fourcc: *const c_char,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyAssets {
    pub available: Option<unsafe extern "C" fn(feature: *const c_char) -> c_int>,
    pub register_group: Option<
        unsafe extern "C" fn(
            group: *const c_char,
            erps: *const *const c_char,
            erp_count: usize,
            deps: *const *const c_char,
            dep_count: usize,
            templ: *const c_char,
        ) -> c_int,
    >,
    pub put_resource: Option<
        unsafe extern "C" fn(
            uri: *const c_char,
            fragments: *const SwoftyFragment,
            fragment_count: usize,
            erps: *const *const c_char,
            erp_count: usize,
        ) -> c_int,
    >,
    pub db_set:
        Option<unsafe extern "C" fn(key: *const c_char, kind: *const c_char, value: *const c_char) -> c_int>,
    pub db_set_raw:
        Option<unsafe extern "C" fn(hash: u64, tag: u8, value: *const c_void, size: usize) -> c_int>,
    pub lang_set: Option<
        unsafe extern "C" fn(language: *const c_char, key: *const c_char, text: *const c_char) -> c_int,
    >,
    pub lang_table: Option<unsafe extern "C" fn(language: *const c_char, file: *const c_char) -> c_int>,
    pub overlay_file: Option<unsafe extern "C" fn(game_path: *const c_char, file: *const c_char) -> c_int>,
}
