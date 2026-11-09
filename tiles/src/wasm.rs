//! A minimal WebAssembly interface with no imports, so a Worker can load it directly.
//! The caller copies a tile in with `alloc`, and reads the result from
//! `result_ptr` and `result_len`.

use crate::pmtiles::{inflate, Archive, Step};
use crate::{base, Labels, Palette, Theme};
use std::cell::{Cell, RefCell};

thread_local! {
    static LABELS: RefCell<Labels> = RefCell::new(Labels::new());
    static RESULT: RefCell<String> = const { RefCell::new(String::new()) };
    static ARCHIVE: RefCell<Option<Archive>> = const { RefCell::new(None) };
    static LOCATED: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
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

/// Marks a tile as just used. Returns 1 if it was there.
#[no_mangle]
pub extern "C" fn touch_tile(z: u32, x: i32, y: i32) -> u32 {
    LABELS.with(|l| u32::from(l.borrow_mut().touch(z as u8, x, y)))
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

/// Loads the archive's header and root directory from its first 16 KB. Returns 1 on success.
///
/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn open_archive(p: *const u8, len: usize) -> u32 {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let head = unsafe { std::slice::from_raw_parts(p, len) };
    let archive = Archive::open(head);
    let ok = archive.is_some();
    ARCHIVE.with(|a| *a.borrow_mut() = archive);
    u32::from(ok)
}

/// What to do for a tile: 0 no such tile, 1 read the tile, 2 read a directory, then
/// `add_directory` and ask again. `located_offset` and `located_length` say what to read.
#[no_mangle]
pub extern "C" fn locate(z: u32, x: u32, y: u32) -> u32 {
    let step = ARCHIVE.with(|a| {
        a.borrow()
            .as_ref()
            .map_or(Step::Missing, |a| a.locate(z as u8, x, y))
    });
    match step {
        Step::Missing => 0,
        Step::Tile { offset, length } => {
            LOCATED.with(|l| l.set((offset as f64, length as f64)));
            1
        }
        Step::Directory { offset, length } => {
            LOCATED.with(|l| l.set((offset as f64, length as f64)));
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn located_offset() -> f64 {
    LOCATED.with(|l| l.get().0)
}

#[no_mangle]
pub extern "C" fn located_length() -> f64 {
    LOCATED.with(|l| l.get().1)
}

/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn add_directory(p: *const u8, len: usize, offset: f64) {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let data = unsafe { std::slice::from_raw_parts(p, len) };
    ARCHIVE.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.add_directory(offset as u64, data);
        }
    });
}

/// Draws a base tile from compressed data. Returns the SVG's length, or 0 if it's unreadable.
///
/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn draw_base_compressed(
    p: *const u8,
    len: usize,
    z: u32,
    theme_id: u32,
) -> u32 {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let data = unsafe { std::slice::from_raw_parts(p, len) };
    inflate(data).map_or(0, |tile| set(base(&tile, z as u8, &theme(theme_id))))
}

/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`, or `len` must be zero.
#[no_mangle]
pub unsafe extern "C" fn add_tile_compressed(p: *const u8, len: usize, z: u32, x: i32, y: i32) {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let data: &[u8] = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(p, len) }
    };
    // Already there: touch it, and skip inflating data that would be thrown away.
    if LABELS.with(|l| l.borrow_mut().touch(z as u8, x, y)) {
        return;
    }
    let tile = if len == 0 {
        Vec::new()
    } else {
        inflate(data).unwrap_or_default()
    };
    LABELS.with(|l| l.borrow_mut().add(&tile, z as u8, x, y));
}

/// Reads a palette the caller copied in. `None` if it isn't one.
///
/// # Safety
/// `p` and `len` must describe a live buffer from `alloc`.
unsafe fn palette(p: *const u8, len: usize) -> Option<Palette> {
    if len != Palette::LEN {
        return None;
    }
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let text = unsafe { std::slice::from_raw_parts(p, len) };
    std::str::from_utf8(text).ok().and_then(Palette::parse)
}

/// Draws a base tile from compressed data in the given palette, which is passed with
/// every call so nothing about a request outlives it. Returns the SVG's length, or 0
/// if the palette or the tile is unreadable.
///
/// # Safety
/// Both buffers must be live buffers from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn draw_base_palette(
    p: *const u8,
    len: usize,
    z: u32,
    palette_p: *const u8,
    palette_len: usize,
) -> u32 {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let Some(colours) = (unsafe { palette(palette_p, palette_len) }) else {
        return 0;
    };
    // SAFETY: as above.
    let data = unsafe { std::slice::from_raw_parts(p, len) };
    inflate(data).map_or(0, |tile| set(base(&tile, z as u8, &colours.theme())))
}

/// Draws the labels for one tile in the given palette. Returns the SVG's length, or 0
/// if the palette is unreadable.
///
/// # Safety
/// `palette_p` and `palette_len` must describe a live buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn draw_labels_palette(
    z: u32,
    x: i32,
    y: i32,
    palette_p: *const u8,
    palette_len: usize,
) -> u32 {
    // SAFETY: the caller guarantees the buffer is live and initialised.
    let Some(colours) = (unsafe { palette(palette_p, palette_len) }) else {
        return 0;
    };
    set(LABELS.with(|l| l.borrow().draw(z as u8, x, y, &colours.theme())))
}
