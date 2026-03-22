//! Browser (wasm-bindgen) bindings for prova-pdf.
//!
//! # Usage from JavaScript / TypeScript
//!
//! ```js
//! import init, { add_font, add_image, set_font_rules, generate_pdf, clear_all }
//!   from './prova_pdf.js';
//!
//! await init();
//! add_font('body', 0, regularFontBytes);
//! add_font('body', 1, boldFontBytes);          // optional
//! add_image('logo.png', logoBytes);            // optional
//! set_font_rules({ heading: 'headingFamily' }); // optional
//!
//! const pdfBytes = generate_pdf(examSpecObject);
//! ```
//!
//! # Font registration
//! Call `add_font(familyName, variant, data)` at least once (variant 0 = regular,
//! the "body" family is required) before calling `generate_pdf`.
//! variant indices: 0 = regular, 1 = bold, 2 = italic, 3 = bold-italic.
//!
//! # Image registration
//! Call `add_image(key, data)` for every image key referenced in the spec.
//!
//! # Font rules
//! `set_font_rules` maps semantic roles (`body`, `heading`, `question`, `math`)
//! to registered family names.  Unset roles keep their current value.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::fonts::{FontRegistry, FontRules};
use crate::pipeline::{self, RenderContext};
use crate::spec::ExamSpec;

// ─────────────────────────────────────────────────────────────────────────────
// Thread-local state
// ─────────────────────────────────────────────────────────────────────────────

thread_local! {
    static FONT_REGISTRY: RefCell<FontRegistry> = RefCell::new(FontRegistry::new());
    static IMAGE_STORE:   RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static FONT_RULES:    RefCell<FontRules>    = RefCell::new(FontRules::default());
}

// ─────────────────────────────────────────────────────────────────────────────
// Font rules input (for set_font_rules)
// ─────────────────────────────────────────────────────────────────────────────

/// Optional overrides for the semantic role → font family mapping.
///
/// Any field left as `null` / `undefined` keeps the current mapping unchanged.
///
/// ```js
/// set_font_rules({ heading: "IBM Plex Sans", math: "Latin Modern Math" });
/// ```
#[derive(Deserialize)]
pub struct FontRulesInput {
    pub body:     Option<String>,
    pub heading:  Option<String>,
    pub question: Option<String>,
    pub math:     Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public wasm-bindgen API
// ─────────────────────────────────────────────────────────────────────────────

/// Register a font variant under a named family.
///
/// At minimum, call `add_font("body", 0, fontBytes)` before `generate_pdf`.
///
/// @param family_name - Named family (e.g., `"body"`, `"heading"`, `"IBM Plex Sans"`)
/// @param variant     - `0`=regular, `1`=bold, `2`=italic, `3`=bold-italic
/// @param data        - TTF or OTF font bytes (`Uint8Array`)
#[wasm_bindgen]
pub fn add_font(family_name: &str, variant: u8, data: &[u8]) -> Result<(), JsError> {
    FONT_REGISTRY.with(|reg| {
        reg.borrow_mut()
            .add_variant(family_name, variant, data.to_vec())
            .map_err(|e| JsError::new(&e.to_string()))
    })
}

/// Register an image by key for use in the exam spec.
///
/// JPEG and PNG are supported when the `images` feature is enabled.
///
/// @param key  - Unique string key referenced in `InlineImage.key` or `header.logoKey`
/// @param data - Image bytes (`Uint8Array`)
#[wasm_bindgen]
pub fn add_image(key: &str, data: &[u8]) {
    IMAGE_STORE.with(|store| {
        store.borrow_mut().insert(key.to_string(), data.to_vec());
    });
}

/// Override the font family used for each semantic role.
///
/// Roles not present in the input object keep their current mapping.
/// Accepts a plain JS object `{ body?, heading?, question?, math? }`.
///
/// @throws if the input cannot be deserialized as a `FontRulesInput` object
#[wasm_bindgen]
pub fn set_font_rules(input: JsValue) -> Result<(), JsError> {
    let parsed: FontRulesInput = serde_wasm_bindgen::from_value(input)
        .map_err(|e| JsError::new(&format!("font rules deserialization error: {e}")))?;

    FONT_RULES.with(|rules| {
        let mut r = rules.borrow_mut();
        if let Some(v) = parsed.body     { r.body     = v; }
        if let Some(v) = parsed.heading  { r.heading  = v; }
        if let Some(v) = parsed.question { r.question = v; }
        if let Some(v) = parsed.math     { r.math     = v; }
    });

    Ok(())
}

/// Clear all registered fonts, images, and font rules.
///
/// Call this between independent exam generations if running in a long-lived
/// WASM instance that serves multiple documents.
#[wasm_bindgen]
pub fn clear_all() {
    FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
    IMAGE_STORE.with(|s| s.borrow_mut().clear());
    FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
}

/// Generate a PDF from an `ExamSpec` JavaScript object.
///
/// Returns the PDF as a `Uint8Array`.  Throws a `JsError` on any failure
/// (spec deserialization error, validation error, or emission error).
///
/// @param input - A plain JavaScript object matching the `ExamSpec` schema
/// @returns     - `Uint8Array` containing the complete PDF bytes
#[wasm_bindgen]
pub fn generate_pdf(input: JsValue) -> Result<Vec<u8>, JsError> {
    let spec: ExamSpec = serde_wasm_bindgen::from_value(input)
        .map_err(|e| JsError::new(&format!("spec deserialization error: {e}")))?;

    generate_pdf_from_spec(spec).map_err(|e| JsError::new(&e))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helper — testable without JsValue
// ─────────────────────────────────────────────────────────────────────────────

/// Run the full pipeline from an already-deserialized `ExamSpec`.
///
/// Builds a `RenderContext` by cloning the thread-local state and calls
/// `pipeline::render`.  Separated from `generate_pdf` so unit tests can
/// call it without constructing a `JsValue`.
pub(crate) fn generate_pdf_from_spec(spec: ExamSpec) -> Result<Vec<u8>, String> {
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

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::data::{FontData, FontFamily};
    use crate::spec::{
        answer::{AnswerSpace, TextualAnswer},
        exam::{ExamSpec, Section},
        inline::{InlineContent, InlineText},
        question::{Question, QuestionKind},
    };
    use crate::test_helpers::fixtures::DEJAVU;

    /// Set up the thread-local state with DejaVu as the body font.
    fn setup() {
        // Reset state so tests don't bleed into each other.
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        IMAGE_STORE.with(|s| s.borrow_mut().clear());
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());

        FONT_REGISTRY.with(|reg| {
            let fd = FontData::from_bytes(DEJAVU).unwrap();
            reg.borrow_mut().add_family("body", FontFamily::new(fd));
        });
    }

    fn one_textual_spec() -> ExamSpec {
        ExamSpec {
            sections: vec![Section {
                title:            None,
                instructions:     vec![],
                category:         None,
                style:            None,
                force_page_break: false,
                questions: vec![Question {
                    number:            None,
                    label:             None,
                    kind:              QuestionKind::Textual,
                    stem:              vec![InlineContent::Text(InlineText {
                        value: "Explique.".into(),
                        style: None,
                    })],
                    answer:            AnswerSpace::Textual(TextualAnswer {
                        line_count: Some(3), ..Default::default()
                    }),
                    base_texts:        vec![],
                    points:            None,
                    full_width:        false,
                    draft_lines:       0,
                    draft_line_height: None,
                    show_number:       true,
                    force_page_break:  false,
                    style:             None,
                }],
            }],
            ..ExamSpec::default()
        }
    }

    // ── generate_pdf_from_spec ────────────────────────────────────────────────

    #[test]
    fn generate_pdf_produces_valid_pdf_bytes() {
        setup();
        let spec  = one_textual_spec();
        let bytes = generate_pdf_from_spec(spec).expect("must render without error");
        assert!(bytes.starts_with(b"%PDF-"), "output must start with %PDF-");
        let tail  = &bytes[bytes.len().saturating_sub(10)..];
        assert!(tail.windows(5).any(|w| w == b"%%EOF"), "output must end with %%EOF");
    }

    #[test]
    fn generate_pdf_fails_without_font() {
        // Reset to an empty registry — no font registered.
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        FONT_RULES.with(|r| *r.borrow_mut() = FontRules::default());
        IMAGE_STORE.with(|s| s.borrow_mut().clear());

        let spec = one_textual_spec();
        let err  = generate_pdf_from_spec(spec).unwrap_err();
        assert!(!err.is_empty(), "error message must not be empty");
        assert!(err.contains("validation") || err.contains("font") || err.contains("Font"),
            "error should mention font or validation: {err}");
    }

    #[test]
    fn generate_pdf_fails_with_no_sections() {
        setup();
        let spec = ExamSpec::default(); // no sections
        let err  = generate_pdf_from_spec(spec).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn generate_all_kinds_fixture() {
        setup();
        let json = include_str!("../../tests/fixtures/all_kinds.json");
        let spec: ExamSpec = serde_json::from_str(json).unwrap();
        let bytes = generate_pdf_from_spec(spec).expect("all_kinds fixture must render");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    // ── set_font_rules (Rust-level) ───────────────────────────────────────────

    #[test]
    fn font_rules_default_all_body() {
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

        let input = FontRulesInput {
            body:     None,
            heading:  Some("heading-family".into()),
            question: None,
            math:     Some("math-family".into()),
        };

        FONT_RULES.with(|rules| {
            let mut r = rules.borrow_mut();
            if let Some(v) = input.body     { r.body     = v; }
            if let Some(v) = input.heading  { r.heading  = v; }
            if let Some(v) = input.question { r.question = v; }
            if let Some(v) = input.math     { r.math     = v; }
        });

        FONT_RULES.with(|r| {
            let rules = r.borrow();
            assert_eq!(rules.body,     "body",           "body should be unchanged");
            assert_eq!(rules.heading,  "heading-family", "heading should be updated");
            assert_eq!(rules.question, "body",           "question should be unchanged");
            assert_eq!(rules.math,     "math-family",    "math should be updated");
        });
    }

    // ── clear_all ─────────────────────────────────────────────────────────────

    #[test]
    fn clear_all_resets_state() {
        setup(); // registers DejaVu as body

        clear_all();

        let ready = FONT_REGISTRY.with(|r| r.borrow().is_ready());
        assert!(!ready, "registry must be empty after clear_all");

        let image_count = IMAGE_STORE.with(|s| s.borrow().len());
        assert_eq!(image_count, 0, "image store must be empty after clear_all");
    }

    // ── add_font ──────────────────────────────────────────────────────────────

    #[test]
    fn add_font_makes_registry_ready() {
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        assert!(!FONT_REGISTRY.with(|r| r.borrow().is_ready()));

        FONT_REGISTRY.with(|reg| {
            reg.borrow_mut()
                .add_variant("body", 0, DEJAVU.to_vec())
                .unwrap();
        });

        assert!(FONT_REGISTRY.with(|r| r.borrow().is_ready()));
    }

    #[test]
    fn add_font_invalid_variant_returns_error() {
        FONT_REGISTRY.with(|r| *r.borrow_mut() = FontRegistry::new());
        let result = FONT_REGISTRY.with(|reg| {
            reg.borrow_mut().add_variant("body", 9, DEJAVU.to_vec())
        });
        assert!(result.is_err());
    }

    // ── add_image ─────────────────────────────────────────────────────────────

    #[test]
    fn add_image_stores_bytes() {
        IMAGE_STORE.with(|s| s.borrow_mut().clear());
        IMAGE_STORE.with(|store| {
            store.borrow_mut().insert("logo".to_string(), vec![0xFF, 0xD8]);
        });
        let found = IMAGE_STORE.with(|s| s.borrow().contains_key("logo"));
        assert!(found, "image must be stored after add_image");
    }
}
