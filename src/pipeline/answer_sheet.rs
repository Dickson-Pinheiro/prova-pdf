//! Pipeline entry point for the OMR answer sheet (gabarito).
//!
//! Mirrors [`super::render`] but drives the fixed-template layout in
//! `layout::answer_sheet`.  The sheet is positioned in page-absolute
//! coordinates, so emission uses a zero-margin A4 [`PageGeometry`].

use crate::fonts::resolve::FontResolver;
use crate::layout::answer_sheet::layout_answer_sheet;
use crate::layout::page::PageGeometry;
use crate::pdf::emit::PdfEmitter;
use crate::spec::answer_sheet::AnswerSheetSpec;

use super::{PipelineError, RenderContext};
use super::validate::ValidationError;

/// A4 page, in points.
const PAGE_W_PT: f64 = 595.28;
const PAGE_H_PT: f64 = 841.89;

/// Render an `AnswerSheetSpec` to PDF bytes.
///
/// Phases: validation → layout (fixed template, page-absolute) → emission.
pub fn render_answer_sheet(
    spec: &AnswerSheetSpec,
    ctx:  &RenderContext,
) -> Result<Vec<u8>, PipelineError> {
    // ── Phase 1: validation ───────────────────────────────────────────────
    let mut errors: Vec<ValidationError> = Vec::new();
    if !ctx.registry.is_ready() {
        errors.push(ValidationError::NoFont);
    }
    if let Some(ref key) = spec.header.logo_key {
        if !ctx.images.contains_key(key) {
            errors.push(ValidationError::MissingImage { key: key.clone() });
        }
    }
    if let Some(payload) = spec.qr_payload() {
        if qrcodegen::QrCode::encode_text(&payload, qrcodegen::QrCodeEcc::Medium).is_err() {
            errors.push(ValidationError::QrPayloadTooLarge { len: payload.len() });
        }
    }
    if !errors.is_empty() {
        return Err(PipelineError::ValidationFailed(errors));
    }

    // ── Phase 2/3: layout ─────────────────────────────────────────────────
    let resolver = FontResolver::new(&ctx.registry, &ctx.rules);
    let pages = layout_answer_sheet(spec, &resolver, &ctx.images);

    // ── Phase 4: emission ─────────────────────────────────────────────────
    let geometry = PageGeometry {
        page_width_pt:     PAGE_W_PT,
        page_height_pt:    PAGE_H_PT,
        margin_top_pt:     0.0,
        margin_bottom_pt:  0.0,
        margin_left_pt:    0.0,
        margin_right_pt:   0.0,
        content_width_pt:  PAGE_W_PT,
        content_height_pt: PAGE_H_PT,
        columns:           1,
        column_gap_pt:     0.0,
        column_width_pt:   PAGE_W_PT,
    };
    let emitter = PdfEmitter::new(&ctx.registry, &ctx.images, false);
    emitter.emit(pages, &geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::fonts::FontRegistry;
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn ready_ctx() -> RenderContext {
        let (registry, rules) = make_resolver_and_rules();
        RenderContext { registry, rules, images: HashMap::new() }
    }

    fn basic_spec() -> AnswerSheetSpec {
        serde_json::from_str(r##"{
            "trackingCode": "#A:1:2ea687c7-8ff8-4821-8d55-1443fe392a9c#",
            "qrData": {"e": "2ea687c7", "k": 1},
            "header": {
                "institution": "Rede Decisão",
                "title": "P5_Matemática_F7_ANGLO_2026",
                "studentFields": [
                    {"label": "Unidade"}, {"label": "Turma"}, {"label": "Aluno"}
                ]
            },
            "answers": {"count": 5, "alternatives": 4},
            "footerText": "Lize - 2026"
        }"##).unwrap()
    }

    #[test]
    fn renders_valid_pdf() {
        let pdf = render_answer_sheet(&basic_spec(), &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let tail = &pdf[pdf.len().saturating_sub(10)..];
        assert!(tail.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn fails_without_font() {
        let ctx = RenderContext {
            registry: FontRegistry::new(),
            rules:    Default::default(),
            images:   HashMap::new(),
        };
        let err = render_answer_sheet(&basic_spec(), &ctx).unwrap_err();
        assert!(matches!(err, PipelineError::ValidationFailed(_)));
    }

    #[test]
    fn fails_with_missing_logo_image() {
        let mut spec = basic_spec();
        spec.header.logo_key = Some("client_logo".into());
        let err = render_answer_sheet(&spec, &ready_ctx()).unwrap_err();
        match err {
            PipelineError::ValidationFailed(errs) => {
                assert!(errs.iter().any(|e| matches!(e, ValidationError::MissingImage { .. })));
            }
            other => panic!("expected ValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn fails_with_oversized_qr_payload() {
        let mut spec = basic_spec();
        // 8 KiB exceeds the byte-mode capacity of any QR version.
        spec.qr_data = Some(serde_json::Value::String("x".repeat(8192)));
        let err = render_answer_sheet(&spec, &ready_ctx()).unwrap_err();
        match err {
            PipelineError::ValidationFailed(errs) => {
                assert!(errs.iter().any(|e| matches!(e, ValidationError::QrPayloadTooLarge { .. })));
            }
            other => panic!("expected ValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn many_questions_spill_to_second_page() {
        let mut spec = basic_spec();
        spec.answers.count = 150; // > 4 columns × 30 rows
        let pdf = render_answer_sheet(&spec, &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"), "150 questions must produce 2 pages");
    }

    #[test]
    fn empty_spec_renders_blank_template() {
        let spec = AnswerSheetSpec::default();
        let pdf = render_answer_sheet(&spec, &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
