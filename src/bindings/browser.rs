//! Browser (wasm-bindgen) bindings for prova-pdf.
//!
//! # Font registration
//! Call `add_font(familyName, variant, data)` one or more times before `generate_pdf`.
//! variant: 0=regular, 1=bold, 2=italic, 3=bold-italic.
//! The name "body" is used as the default family.
//!
//! # Image registration
//! Call `add_image(key, data)` for each image referenced in the spec.

use wasm_bindgen::prelude::*;
use crate::fonts::{FontRegistry, FontRules};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static FONT_REGISTRY: RefCell<FontRegistry> = RefCell::new(FontRegistry::new());
    static IMAGE_STORE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static FONT_RULES: RefCell<FontRules> = RefCell::new(FontRules::default());
}

/// Register a font variant under a named family.
///
/// @param familyName - Named family (e.g., "body", "IBM Plex Sans", "heading")
/// @param variant    - 0=regular, 1=bold, 2=italic, 3=bold-italic
/// @param data       - TTF or OTF font bytes
#[wasm_bindgen]
pub fn add_font(family_name: &str, variant: u8, data: &[u8]) -> Result<(), JsError> {
    FONT_REGISTRY.with(|reg| {
        reg.borrow_mut()
            .add_variant(family_name, variant, data.to_vec())
            .map_err(|e| JsError::new(&e.to_string()))
    })
}

/// Register an image for use in the exam spec via its key.
#[wasm_bindgen]
pub fn add_image(key: &str, data: &[u8]) -> Result<(), JsError> {
    IMAGE_STORE.with(|store| {
        store.borrow_mut().insert(key.to_string(), data.to_vec());
    });
    Ok(())
}

/// Clear all registered fonts and images (call between independent exam generations).
#[wasm_bindgen]
pub fn clear_all() {
    FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
    IMAGE_STORE.with(|s| s.borrow_mut().clear());
    FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
}

/// Generate a PDF from an ExamSpec JSON object.
///
/// Returns the PDF as a Uint8Array of bytes.
#[wasm_bindgen]
pub fn generate_pdf(input: JsValue) -> Result<Vec<u8>, JsError> {
    let spec: crate::spec::ExamSpec = serde_wasm_bindgen::from_value(input)
        .map_err(|e| JsError::new(&format!("spec deserialization error: {e}")))?;

    // Validate: at least one font registered
    let ready = FONT_REGISTRY.with(|r| r.borrow().is_ready());
    if !ready {
        return Err(JsError::new("No font registered. Call add_font('body', 0, fontBytes) before generate_pdf."));
    }

    // TODO: wire to pipeline::render once PDF emission is implemented
    // For now return a stub error so the project compiles
    Err(JsError::new("PDF emission not yet implemented — scaffold only"))
}
