//! WASI C-ABI exports for prova-pdf.
//! All exports use the `prova_pdf_` prefix.
//!
//! Memory protocol (same as benchmark exam-pdf):
//!   1. Host calls prova_pdf_alloc(len) to get a pointer into WASM memory.
//!   2. Host writes data at that pointer.
//!   3. Host calls the function (e.g., prova_pdf_add_font).
//!   4. Host calls prova_pdf_free(ptr, len) to release the allocation.

use std::cell::RefCell;
use std::collections::HashMap;
use crate::fonts::{FontRegistry, FontRules};

thread_local! {
    static FONT_REGISTRY: RefCell<FontRegistry> = RefCell::new(FontRegistry::new());
    static IMAGE_STORE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg.into()));
}

// ── Memory management ─────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_alloc(len: usize) -> *mut u8 {
    let mut buf = vec![0u8; len];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() { return; }
    unsafe { drop(Vec::from_raw_parts(ptr, len, len)); }
}

// ── Font management ───────────────────────────────────────────────────────────

/// Register a font variant.
/// Returns 0 on success, -1 on error (call prova_pdf_last_error_* for details).
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_add_font(
    family_ptr: *const u8,
    family_len: usize,
    variant: u8,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    let family_name = unsafe {
        match std::str::from_utf8(std::slice::from_raw_parts(family_ptr, family_len)) {
            Ok(s) => s.to_string(),
            Err(e) => { set_last_error(e.to_string()); return -1; }
        }
    };
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len).to_vec() };

    FONT_REGISTRY.with(|reg| {
        reg.borrow_mut().add_variant(&family_name, variant, data)
            .map(|_| 0i32)
            .unwrap_or_else(|e| { set_last_error(e.to_string()); -1 })
    })
}

// ── Image management ──────────────────────────────────────────────────────────

/// Register an image by key.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_add_image(
    key_ptr: *const u8,
    key_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    let key = unsafe {
        match std::str::from_utf8(std::slice::from_raw_parts(key_ptr, key_len)) {
            Ok(s) => s.to_string(),
            Err(e) => { set_last_error(e.to_string()); return -1; }
        }
    };
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len).to_vec() };
    IMAGE_STORE.with(|s| s.borrow_mut().insert(key, data));
    0
}

// ── Clear ─────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_clear_all() {
    FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
    IMAGE_STORE.with(|s| s.borrow_mut().clear());
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

// ── PDF generation ────────────────────────────────────────────────────────────

/// Generate PDF from a JSON ExamSpec.
/// On success: returns pointer to PDF bytes, writes length to *out_len.
/// On failure: returns null, sets last error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_generate(
    json_ptr: *const u8,
    json_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(e) => { set_last_error(e.to_string()); return std::ptr::null_mut(); }
    };

    let _spec: crate::spec::ExamSpec = match serde_json::from_str(json_str) {
        Ok(s) => s,
        Err(e) => { set_last_error(format!("JSON parse error: {e}")); return std::ptr::null_mut(); }
    };

    // TODO: wire to pipeline::render once PDF emission is implemented
    set_last_error("PDF emission not yet implemented — scaffold only");
    std::ptr::null_mut()
}

// ── Error reporting ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_last_error_len() -> usize {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| s.len()))
}

#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_last_error_message(buf: *mut u8) {
    LAST_ERROR.with(|e| {
        if let Some(msg) = e.borrow().as_ref() {
            let bytes = msg.as_bytes();
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len()); }
        }
    });
}
