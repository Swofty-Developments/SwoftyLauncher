use std::ffi::{c_char, c_int, c_void};

pub const DB_INT: u8 = 0;
pub const DB_FLOAT: u8 = 1;
pub const DB_STRING: u8 = 2;
pub const DB_ENUM: u8 = 3;
pub const DB_ID: u8 = 4;
pub const DB_BOOL: u8 = 5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyDbValue {
    pub tag: u8,
    pub i: i64,
    pub f: f64,
    pub s: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyDbCursor {
    pub table: u64,
    pub index: u64,
    pub count: u64,
    pub row: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyDb {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub get: Option<unsafe extern "C" fn(key: *const c_char, out: *mut SwoftyDbValue) -> c_int>,
    pub get_hash: Option<unsafe extern "C" fn(hash: u64, out: *mut SwoftyDbValue) -> c_int>,
    pub int_at: Option<unsafe extern "C" fn(key: *const c_char, out: *mut i64) -> c_int>,
    pub float_at: Option<unsafe extern "C" fn(key: *const c_char, out: *mut f64) -> c_int>,
    pub string_at: Option<unsafe extern "C" fn(key: *const c_char, out: *mut *const c_char) -> c_int>,
    pub bool_at: Option<unsafe extern "C" fn(key: *const c_char, out: *mut i32) -> c_int>,
    pub open: Option<unsafe extern "C" fn(table: *const c_char, cursor: *mut SwoftyDbCursor) -> c_int>,
    pub row: Option<
        unsafe extern "C" fn(
            table: *const c_char,
            label: *const c_char,
            cursor: *mut SwoftyDbCursor,
        ) -> c_int,
    >,
    pub step: Option<unsafe extern "C" fn(cursor: *mut SwoftyDbCursor) -> c_int>,
    pub cell: Option<
        unsafe extern "C" fn(
            cursor: *const SwoftyDbCursor,
            field: *const c_char,
            out: *mut SwoftyDbValue,
        ) -> c_int,
    >,
    pub row_name: Option<unsafe extern "C" fn(cursor: *const SwoftyDbCursor) -> *const c_char>,
}
