use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct SwoftyPaint {
    _private: [u8; 0],
}

pub type SwoftyPaintFn = unsafe extern "C" fn(paint: *mut SwoftyPaint, user: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyOverlay {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub on_paint:
        Option<unsafe extern "C" fn(f: Option<SwoftyPaintFn>, user: *mut c_void, token: *mut u32) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(token: u32) -> c_int>,
    pub size: Option<unsafe extern "C" fn(paint: *const SwoftyPaint, width: *mut f32, height: *mut f32)>,
    pub rect:
        Option<unsafe extern "C" fn(paint: *mut SwoftyPaint, x: f32, y: f32, w: f32, h: f32, rgba: u32)>,
    pub rect_outline: Option<
        unsafe extern "C" fn(
            paint: *mut SwoftyPaint,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            thickness: f32,
            rgba: u32,
        ),
    >,
    pub line: Option<
        unsafe extern "C" fn(
            paint: *mut SwoftyPaint,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            thickness: f32,
            rgba: u32,
        ),
    >,
    pub text: Option<
        unsafe extern "C" fn(
            paint: *mut SwoftyPaint,
            x: f32,
            y: f32,
            size: f32,
            rgba: u32,
            utf8: *const c_char,
        ),
    >,
    pub measure: Option<
        unsafe extern "C" fn(
            paint: *const SwoftyPaint,
            size: f32,
            utf8: *const c_char,
            width: *mut f32,
            height: *mut f32,
        ),
    >,
    pub load_image: Option<unsafe extern "C" fn(id: *const c_char, file: *const c_char) -> c_int>,
    pub image: Option<
        unsafe extern "C" fn(
            paint: *mut SwoftyPaint,
            id: *const c_char,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
            tint: u32,
        ) -> c_int,
    >,
}
