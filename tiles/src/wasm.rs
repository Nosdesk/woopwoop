//! A minimal WebAssembly interface with no imports, so a Worker can load it directly.
//! The caller copies a tile in with `alloc`, and reads the result from
//! `result_ptr` and `result_len`.

use crate::{base, Labels, Theme};
use std::cell::RefCell;

thread_local! {
    static LABELS: RefCell<Labels> = RefCell::new(Labels::new());
    static RESULT: RefCell<String> = const { RefCell::new(String::new()) };
}

fn theme(id: u32) -> Theme<'static> {
    if id == 1 {
        Theme::DARK
    } else {
        Theme::LIGHT
    }
}

fn set(result: String) -> u32 {
    let len = result.len() as u32;
    RESULT.with(|r| *r.borrow_mut() = result);
    len
}

/// # Safety
/// The caller frees the buffer with `free`, passing the same length.
#[no_mangle]
pub unsafe extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p` and `len` must come from one call to `alloc`.
#[no_mangle]
pub unsafe extern "C" fn free(p: *mut u8, len: usize) {
    // SAFETY: the caller guarantees this is the buffer `alloc` returned.
    drop(unsafe { Vec::from_raw_parts(p, 0, len) });
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn draw_base(p: *const u8, len: usize, z: u32, theme_id: u32) -> u32 {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let tile = unsafe { std::slice::from_raw_parts(p, len) };
    set(base(tile, z as u8, &theme(theme_id)))
}

#[no_mangle]
pub extern "C" fn has_tile(z: u32, x: i32, y: i32) -> u32 {
    LABELS.with(|l| u32::from(l.borrow().has(z as u8, x, y)))
}

/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`, or `len` must be zero.
#[no_mangle]
pub unsafe extern "C" fn add_tile(p: *const u8, len: usize, z: u32, x: i32, y: i32) {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let tile: &[u8] = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(p, len) }
    };
    LABELS.with(|l| l.borrow_mut().add(tile, z as u8, x, y));
}

#[no_mangle]
pub extern "C" fn draw_labels(z: u32, x: i32, y: i32, theme_id: u32) -> u32 {
    set(LABELS.with(|l| l.borrow().draw(z as u8, x, y, &theme(theme_id))))
}
