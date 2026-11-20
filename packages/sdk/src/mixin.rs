use std::ffi::{c_char, c_int, c_void};

pub const MIXIN_WRAP: u32 = 1;
pub const MIXIN_SITE: u32 = 2;
pub const MIXIN_VTABLE: u32 = 3;
pub const MIXIN_REDIRECT: u32 = 4;
pub const MIXIN_PATCH: u32 = 5;
pub const MIXIN_ON_LOAD: u32 = 6;
pub const MIXIN_OVERWRITE: u32 = 0x100;

pub type RegisterMixinsFn = unsafe extern "C" fn(mixins: *const SwoftyMixin, count: usize) -> c_int;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyMixin {
    pub target: *const c_char,
    pub kind: u32,
    pub priority: i32,
    pub handler: *mut c_void,
    pub orig: *mut *mut c_void,
}
