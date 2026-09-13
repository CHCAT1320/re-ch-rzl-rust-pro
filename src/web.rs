//! Web (wasm) bindings.
//!
//! The font and the hit sounds stay embedded in the wasm binary. Only the
//! chart JSON and the music are supplied by the user through the page's file
//! inputs. Audio is handled by a small WebAudio bridge in `web/app.js` instead
//! of cpal, whose wasm backend needs wasm-bindgen glue that miniquad's loader
//! does not provide.

use std::cell::{Cell, RefCell};

thread_local! {
    static CHART_BYTES: RefCell<Option<Vec<u8>>> = RefCell::new(None);
    static MUSIC_READY: Cell<bool> = Cell::new(false);
}

// --- exports called from JavaScript -----------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn web_alloc(len: u32) -> u32 {
    let mut buffer = vec![0u8; len as usize];
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn web_supply_chart(ptr: u32, len: u32) {
    let bytes = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
    CHART_BYTES.with(|cell| *cell.borrow_mut() = Some(bytes));
}

#[unsafe(no_mangle)]
pub extern "C" fn web_set_music_ready() {
    MUSIC_READY.with(|cell| cell.set(true));
}

// --- imports provided by web/app.js -----------------------------------------

unsafe extern "C" {
    fn js_sfx_load(kind: i32, ptr: u32, len: u32);
    fn js_sfx_play(kind: i32);
    fn js_music_play();
    fn js_music_position() -> f64;
}

pub fn init_hit_sounds(tap: &[u8], drag: &[u8]) {
    unsafe {
        js_sfx_load(0, tap.as_ptr() as u32, tap.len() as u32);
        js_sfx_load(1, drag.as_ptr() as u32, drag.len() as u32);
    }
}

pub fn play_hit_sound(note_type: i32) {
    unsafe { js_sfx_play(if note_type == 1 { 1 } else { 0 }) }
}

pub fn music_play() {
    unsafe { js_music_play() }
}

pub fn music_position() -> f64 {
    unsafe { js_music_position() }
}

pub fn music_ready() -> bool {
    MUSIC_READY.with(|cell| cell.get())
}

pub fn take_chart() -> Option<Vec<u8>> {
    CHART_BYTES.with(|cell| cell.borrow_mut().take())
}
