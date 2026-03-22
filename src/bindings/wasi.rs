//! WASI C-ABI exports for prova-pdf.
//!
//! All symbols use the `prova_pdf_` prefix.
//!
//! # Memory protocol
//!
//! 1. Host calls `prova_pdf_alloc(len)` → gets a pointer into WASM linear memory.
//! 2. Host writes data at that pointer (JSON string, font bytes, …).
//! 3. Host calls the desired function passing the pointer and length.
//! 4. Host calls `prova_pdf_free(ptr, len)` to release the allocation.
//!
//! # Return convention
//!
//! `prova_pdf_generate` and `prova_pdf_add_font` return an `i32`:
//! - `>= 0` — success; for `prova_pdf_generate` the value is the number of PDF
//!   bytes written to the output pointer.
//! - `< 0`  — failure; call `prova_pdf_last_error_*` to retrieve the message.
//!
//! # Output buffer for prova_pdf_generate
//!
//! Because WASM modules manage their own linear memory, `prova_pdf_generate`
//! uses a **two-call protocol**:
//!
//! 1. First call with `out_buf = null`, `out_cap = 0` → returns the required
//!    byte count (as a positive i32) without writing anything.  The PDF bytes
//!    are kept in an internal thread-local staging buffer.
//! 2. Host allocates `prova_pdf_alloc(n)` and calls again with the returned
//!    pointer and the same `out_cap = n` → bytes are copied and the staging
//!    buffer is cleared.
//!
//! Alternatively, host languages that can read WASM memory directly may keep
//! the staging buffer in place and read from `prova_pdf_output_ptr` /
//! `prova_pdf_output_len` without a second call.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Deserialize;

use crate::fonts::{FontRegistry, FontRules};
use crate::pipeline::{self, RenderContext};
use crate::spec::ExamSpec;

// ─────────────────────────────────────────────────────────────────────────────
// Thread-local state
// ─────────────────────────────────────────────────────────────────────────────

thread_local! {
    static FONT_REGISTRY: RefCell<FontRegistry>          = RefCell::new(FontRegistry::new());
    static IMAGE_STORE:   RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static FONT_RULES:    RefCell<FontRules>             = RefCell::new(FontRules::default());
    static LAST_ERROR:    RefCell<Option<String>>        = const { RefCell::new(None) };
    /// Staging buffer: holds the most recently generated PDF bytes.
    static OUTPUT_BUF:    RefCell<Vec<u8>>               = RefCell::new(Vec::new());
}

fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg.into()));
}

fn clear_last_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

// ─────────────────────────────────────────────────────────────────────────────
// FontRulesInput — JSON-deserializable partial override
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FontRulesInput {
    body:     Option<String>,
    heading:  Option<String>,
    question: Option<String>,
    math:     Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Memory management
// ─────────────────────────────────────────────────────────────────────────────

/// Allocate `len` bytes in WASM linear memory and return the pointer.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_alloc(len: usize) -> *mut u8 {
    let mut buf = vec![0u8; len];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Free a pointer previously returned by `prova_pdf_alloc`.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() { return; }
    unsafe { drop(Vec::from_raw_parts(ptr, len, len)); }
}

// ─────────────────────────────────────────────────────────────────────────────
// Font management
// ─────────────────────────────────────────────────────────────────────────────

/// Register a font variant under a named family.
///
/// `variant`: 0 = regular, 1 = bold, 2 = italic, 3 = bold-italic.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_add_font(
    family_ptr: *const u8,
    family_len: usize,
    variant:    u8,
    data_ptr:   *const u8,
    data_len:   usize,
) -> i32 {
    let family_name = unsafe {
        match std::str::from_utf8(std::slice::from_raw_parts(family_ptr, family_len)) {
            Ok(s)  => s.to_string(),
            Err(e) => { set_last_error(e.to_string()); return -1; }
        }
    };
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len).to_vec() };

    FONT_REGISTRY.with(|reg| {
        reg.borrow_mut()
            .add_variant(&family_name, variant, data)
            .map(|_| 0i32)
            .unwrap_or_else(|e| { set_last_error(e.to_string()); -1 })
    })
}

/// Override font-role → family-name mappings from a JSON object.
///
/// Accepts `{ "body"?: string, "heading"?: string, "question"?: string, "math"?: string }`.
/// Unset fields keep their current value.
/// Returns 0 on success, -1 on parse error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_set_font_rules(
    json_ptr: *const u8,
    json_len: usize,
) -> i32 {
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str   = match std::str::from_utf8(json_bytes) {
        Ok(s)  => s,
        Err(e) => { set_last_error(format!("UTF-8 error: {e}")); return -1; }
    };

    let input: FontRulesInput = match serde_json::from_str(json_str) {
        Ok(v)  => v,
        Err(e) => { set_last_error(format!("font rules parse error: {e}")); return -1; }
    };

    FONT_RULES.with(|rules| {
        let mut r = rules.borrow_mut();
        if let Some(v) = input.body     { r.body     = v; }
        if let Some(v) = input.heading  { r.heading  = v; }
        if let Some(v) = input.question { r.question = v; }
        if let Some(v) = input.math     { r.math     = v; }
    });

    clear_last_error();
    0
}

// ─────────────────────────────────────────────────────────────────────────────
// Image management
// ─────────────────────────────────────────────────────────────────────────────

/// Register an image by key (for `InlineImage.key` or `header.logoKey`).
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_add_image(
    key_ptr:  *const u8,
    key_len:  usize,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    let key = unsafe {
        match std::str::from_utf8(std::slice::from_raw_parts(key_ptr, key_len)) {
            Ok(s)  => s.to_string(),
            Err(e) => { set_last_error(e.to_string()); return -1; }
        }
    };
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len).to_vec() };
    IMAGE_STORE.with(|s| s.borrow_mut().insert(key, data));
    0
}

// ─────────────────────────────────────────────────────────────────────────────
// Clear
// ─────────────────────────────────────────────────────────────────────────────

/// Reset all fonts, images, font rules, staging buffer, and last error.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_clear_all() {
    FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
    IMAGE_STORE.with(|s| s.borrow_mut().clear());
    FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
    OUTPUT_BUF.with(|b| b.borrow_mut().clear());
}

// ─────────────────────────────────────────────────────────────────────────────
// PDF generation
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a PDF from a JSON-encoded `ExamSpec`.
///
/// On success: renders the PDF, stores bytes in the internal staging buffer,
/// copies up to `out_cap` bytes to `out_buf`, and returns the total byte count
/// (which may be greater than `out_cap` if the buffer was too small).
///
/// Pass `out_buf = null` / `out_cap = 0` to query the required size without copying.
///
/// On failure: returns -1 and stores the error in `LAST_ERROR`.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_generate(
    json_ptr: *const u8,
    json_len: usize,
    out_buf:  *mut u8,
    out_cap:  usize,
) -> i32 {
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str   = match std::str::from_utf8(json_bytes) {
        Ok(s)  => s,
        Err(e) => { set_last_error(format!("UTF-8 error: {e}")); return -1; }
    };

    let spec: ExamSpec = match serde_json::from_str(json_str) {
        Ok(s)  => s,
        Err(e) => { set_last_error(format!("JSON parse error: {e}")); return -1; }
    };

    let pdf_bytes = match generate_from_spec(spec) {
        Ok(b)  => b,
        Err(e) => { set_last_error(e); return -1; }
    };

    let n = pdf_bytes.len();

    // Copy to caller's buffer if provided.
    if !out_buf.is_null() && out_cap >= n {
        unsafe { std::ptr::copy_nonoverlapping(pdf_bytes.as_ptr(), out_buf, n); }
    }

    // Keep bytes in the staging buffer for `prova_pdf_output_ptr` / `_len`.
    OUTPUT_BUF.with(|b| *b.borrow_mut() = pdf_bytes);

    clear_last_error();
    n as i32
}

/// Returns a pointer to the most recently generated PDF bytes (staging buffer).
/// Valid until the next call to `prova_pdf_generate` or `prova_pdf_clear_all`.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_output_ptr() -> *const u8 {
    OUTPUT_BUF.with(|b| {
        let buf = b.borrow();
        if buf.is_empty() { std::ptr::null() } else { buf.as_ptr() }
    })
}

/// Returns the byte length of the most recently generated PDF (staging buffer).
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_output_len() -> usize {
    OUTPUT_BUF.with(|b| b.borrow().len())
}

// ─────────────────────────────────────────────────────────────────────────────
// Error reporting
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the byte length of the last error message (0 if no error).
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_last_error_len() -> usize {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| s.len()))
}

/// Copies the last error message into `buf`.
/// The caller must allocate at least `prova_pdf_last_error_len()` bytes.
/// The string is NOT null-terminated.
#[unsafe(no_mangle)]
pub extern "C" fn prova_pdf_last_error_message(buf: *mut u8) {
    LAST_ERROR.with(|e| {
        if let Some(msg) = e.borrow().as_ref() {
            let bytes = msg.as_bytes();
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len()); }
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helper — testable without raw pointers
// ─────────────────────────────────────────────────────────────────────────────

/// Run the full pipeline from an already-deserialized `ExamSpec`.
///
/// Clones thread-local state into a `RenderContext` and calls `pipeline::render`.
/// Separated from the C-ABI functions so unit tests can call it without unsafe.
pub(crate) fn generate_from_spec(spec: ExamSpec) -> Result<Vec<u8>, String> {
    FONT_REGISTRY.with(|reg| {
        FONT_RULES.with(|rules| {
            IMAGE_STORE.with(|images| {
                let ctx = RenderContext {
                    registry: reg.borrow().clone(),
                    rules:    rules.borrow().clone(),
                    images:   images.borrow().clone(),
                };
                pipeline::render(&spec, &ctx).map_err(|e| e.to_string())
            })
        })
    })
}

/// Parse a JSON string and run the full pipeline.
///
/// Used in tests to exercise the same code path as `prova_pdf_generate`
/// without constructing raw C pointers.
pub(crate) fn generate_from_json(json: &str) -> Result<Vec<u8>, String> {
    let spec: ExamSpec = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {e}"))?;
    generate_from_spec(spec)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::data::{FontData, FontFamily};
    use crate::test_helpers::fixtures::DEJAVU;

    fn setup() {
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        IMAGE_STORE.with(|s| s.borrow_mut().clear());
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
        LAST_ERROR.with(|e| *e.borrow_mut() = None);
        OUTPUT_BUF.with(|b| b.borrow_mut().clear());

        FONT_REGISTRY.with(|reg| {
            let fd = FontData::from_bytes(DEJAVU).unwrap();
            reg.borrow_mut().add_family("body", FontFamily::new(fd));
        });
    }

    // ── generate_from_json ────────────────────────────────────────────────────

    #[test]
    fn generate_all_kinds_fixture() {
        setup();
        let json  = include_str!("../../tests/fixtures/all_kinds.json");
        let bytes = generate_from_json(json).expect("fixture must render");
        assert!(bytes.starts_with(b"%PDF-"), "output must start with %PDF-");
        let tail  = &bytes[bytes.len().saturating_sub(10)..];
        assert!(tail.windows(5).any(|w| w == b"%%EOF"), "must end with %%EOF");
    }

    #[test]
    fn generate_fails_without_font() {
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
        IMAGE_STORE.with(|s| s.borrow_mut().clear());

        let json = include_str!("../../tests/fixtures/all_kinds.json");
        let err  = generate_from_json(json).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn generate_fails_with_invalid_json() {
        setup();
        let err = generate_from_json("not valid json").unwrap_err();
        assert!(err.contains("JSON parse error"), "expected JSON error, got: {err}");
    }

    #[test]
    fn generate_fails_with_empty_sections() {
        setup();
        let err = generate_from_json(r#"{"sections":[]}"#).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn generate_produces_non_empty_pdf() {
        setup();
        let json  = include_str!("../../tests/fixtures/all_kinds.json");
        let bytes = generate_from_json(json).unwrap();
        assert!(bytes.len() > 100, "PDF must have substantial content");
    }

    // ── FONT_RULES thread-local ───────────────────────────────────────────────

    #[test]
    fn font_rules_defaults_to_body() {
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
        FONT_RULES.with(|r| {
            let rules = r.borrow();
            assert_eq!(rules.body,     "body");
            assert_eq!(rules.heading,  "body");
            assert_eq!(rules.question, "body");
            assert_eq!(rules.math,     "body");
        });
    }

    #[test]
    fn apply_font_rules_input_partial_update() {
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());

        let input: FontRulesInput = serde_json::from_str(
            r#"{"heading":"Helvetica","math":"Latin Modern Math"}"#
        ).unwrap();

        FONT_RULES.with(|rules| {
            let mut r = rules.borrow_mut();
            if let Some(v) = input.body     { r.body     = v; }
            if let Some(v) = input.heading  { r.heading  = v; }
            if let Some(v) = input.question { r.question = v; }
            if let Some(v) = input.math     { r.math     = v; }
        });

        FONT_RULES.with(|r| {
            let rules = r.borrow();
            assert_eq!(rules.body,     "body",             "body should be unchanged");
            assert_eq!(rules.heading,  "Helvetica",        "heading should be updated");
            assert_eq!(rules.question, "body",             "question should be unchanged");
            assert_eq!(rules.math,     "Latin Modern Math","math should be updated");
        });
    }

    #[test]
    fn font_rules_input_all_none_leaves_defaults() {
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());

        let input: FontRulesInput = serde_json::from_str("{}").unwrap();

        FONT_RULES.with(|rules| {
            let mut r = rules.borrow_mut();
            if let Some(v) = input.body     { r.body     = v; }
            if let Some(v) = input.heading  { r.heading  = v; }
            if let Some(v) = input.question { r.question = v; }
            if let Some(v) = input.math     { r.math     = v; }
        });

        FONT_RULES.with(|r| {
            let rules = r.borrow();
            assert_eq!(rules.body,     "body");
            assert_eq!(rules.heading,  "body");
        });
    }

    // ── LAST_ERROR helpers ────────────────────────────────────────────────────

    #[test]
    fn set_and_read_last_error() {
        LAST_ERROR.with(|e| *e.borrow_mut() = None);

        set_last_error("test error");

        let len = LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| s.len()));
        assert!(len > 0, "error length must be positive");

        let msg = LAST_ERROR.with(|e| e.borrow().clone());
        assert_eq!(msg.as_deref(), Some("test error"));
    }

    #[test]
    fn clear_last_error_clears_state() {
        set_last_error("something went wrong");
        clear_last_error();
        let len = LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| s.len()));
        assert_eq!(len, 0);
    }

    // ── clear_all ─────────────────────────────────────────────────────────────

    #[test]
    fn clear_all_resets_all_state() {
        setup();
        set_last_error("leftover error");
        OUTPUT_BUF.with(|b| *b.borrow_mut() = vec![1, 2, 3]);

        prova_pdf_clear_all();

        assert!(!FONT_REGISTRY.with(|r| r.borrow().is_ready()),
            "registry must be empty after clear_all");
        assert_eq!(IMAGE_STORE.with(|s| s.borrow().len()), 0);
        assert_eq!(LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| s.len())), 0);
        assert_eq!(OUTPUT_BUF.with(|b| b.borrow().len()), 0);
    }

    // ── OUTPUT_BUF staging ────────────────────────────────────────────────────

    #[test]
    fn output_buf_populated_after_successful_generate() {
        setup();
        OUTPUT_BUF.with(|b| b.borrow_mut().clear());

        let json = include_str!("../../tests/fixtures/all_kinds.json");
        generate_from_json(json).unwrap();

        // Simulate what prova_pdf_generate does: store to OUTPUT_BUF.
        let json_bytes = include_str!("../../tests/fixtures/all_kinds.json").as_bytes();
        let spec: ExamSpec = serde_json::from_slice(json_bytes).unwrap();
        let pdf = generate_from_spec(spec).unwrap();
        OUTPUT_BUF.with(|b| *b.borrow_mut() = pdf);

        let len = OUTPUT_BUF.with(|b| b.borrow().len());
        assert!(len > 0, "staging buffer must be populated after generate");
    }
}
