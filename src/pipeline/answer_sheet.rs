//! Pipeline entry point for the OMR answer sheet (gabarito).
//!
//! Mirrors [`super::render`] but drives the fixed-template layout in
//! `layout::answer_sheet`.  The sheet is positioned in page-absolute
//! coordinates, so emission uses a zero-margin A4 [`PageGeometry`].

use crate::fonts::resolve::FontResolver;
use crate::layout::answer_sheet::layout_answer_sheet;
use crate::layout::fragment::Fragment;
use crate::layout::page::PageGeometry;
use crate::pdf::emit::PdfEmitter;
use crate::spec::answer_sheet::AnswerSheetSpec;

use super::{PipelineError, RenderContext};
use super::validate::ValidationError;

/// A4 page, in points.
const PAGE_W_PT: f64 = 595.28;
const PAGE_H_PT: f64 = 841.89;

/// Zero-margin A4 geometry: the sheet is positioned in page-absolute
/// coordinates, so the emitter must not apply any content margin.
fn sheet_geometry() -> PageGeometry {
    PageGeometry {
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
    }
}

/// Validate a single answer-sheet spec against the render context.
///
/// Collects every problem in one pass (missing font, missing logo image,
/// oversized QR payload) so the caller can report them together.
fn validate_answer_sheet(
    spec: &AnswerSheetSpec,
    ctx:  &RenderContext,
) -> Vec<ValidationError> {
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
    errors
}

/// Render a single `AnswerSheetSpec` to PDF bytes.
///
/// Phases: validation → layout (fixed template, page-absolute) → emission.
/// Equivalent to [`render_answer_sheets`] with a one-element slice.
pub fn render_answer_sheet(
    spec: &AnswerSheetSpec,
    ctx:  &RenderContext,
) -> Result<Vec<u8>, PipelineError> {
    render_answer_sheets(std::slice::from_ref(spec), ctx)
}

/// Render a list of answer sheets into a single PDF, one sheet after another.
///
/// Each sheet keeps its own fixed template and starts on a fresh page; a long
/// answer grid still spills onto continuation pages within its own sheet. All
/// pages are emitted in one document so font subsetting is shared across the
/// whole batch.
///
/// Phases:
/// 1. **Validation** — every spec is validated first; the first invalid sheet
///    aborts the batch with its index, so the PDF is never partial.
/// 2. **Layout** — each spec is laid out and its pages concatenated in order.
/// 3. **Emission** — the combined pages become one PDF.
///
/// An empty slice produces a single blank page (a valid, if empty, PDF).
pub fn render_answer_sheets(
    specs: &[AnswerSheetSpec],
    ctx:   &RenderContext,
) -> Result<Vec<u8>, PipelineError> {
    // ── Phase 1: validation (whole batch, fail-fast with index) ───────────
    for (index, spec) in specs.iter().enumerate() {
        let errors = validate_answer_sheet(spec, ctx);
        if !errors.is_empty() {
            return Err(PipelineError::AnswerSheetValidationFailed { index, errors });
        }
    }

    // ── Phase 2/3: layout each sheet; concatenate pages ───────────────────
    let resolver = FontResolver::new(&ctx.registry, &ctx.rules);
    let mut pages: Vec<Vec<Fragment>> = Vec::new();
    for spec in specs {
        pages.extend(layout_answer_sheet(spec, &resolver, &ctx.images));
    }

    // ── Phase 4: emission ─────────────────────────────────────────────────
    let geometry = sheet_geometry();
    let emitter  = PdfEmitter::new(&ctx.registry, &ctx.images, false);
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
        assert!(matches!(err, PipelineError::AnswerSheetValidationFailed { index: 0, .. }));
    }

    #[test]
    fn fails_with_missing_logo_image() {
        let mut spec = basic_spec();
        spec.header.logo_key = Some("client_logo".into());
        let err = render_answer_sheet(&spec, &ready_ctx()).unwrap_err();
        match err {
            PipelineError::AnswerSheetValidationFailed { index, errors } => {
                assert_eq!(index, 0);
                assert!(errors.iter().any(|e| matches!(e, ValidationError::MissingImage { .. })));
            }
            other => panic!("expected AnswerSheetValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn fails_with_oversized_qr_payload() {
        let mut spec = basic_spec();
        // 8 KiB exceeds the byte-mode capacity of any QR version.
        spec.qr_data = Some(serde_json::Value::String("x".repeat(8192)));
        let err = render_answer_sheet(&spec, &ready_ctx()).unwrap_err();
        match err {
            PipelineError::AnswerSheetValidationFailed { index, errors } => {
                assert_eq!(index, 0);
                assert!(errors.iter().any(|e| matches!(e, ValidationError::QrPayloadTooLarge { .. })));
            }
            other => panic!("expected AnswerSheetValidationFailed, got {other:?}"),
        }
    }

    // ── Batch (list of sheets) ────────────────────────────────────────────

    /// Count `/Type /Page` occurrences (page leaves, not the `/Pages` tree
    /// root) so we can assert how many sheets/pages a batch produced.
    fn page_count(pdf: &[u8]) -> usize {
        let text = String::from_utf8_lossy(pdf);
        text.matches("/Type /Page\n").count() + text.matches("/Type /Page ").count()
    }

    #[test]
    fn batch_concatenates_one_sheet_per_page() {
        // Three single-page sheets → three pages in one PDF.
        let specs = vec![basic_spec(), basic_spec(), basic_spec()];
        let pdf = render_answer_sheets(&specs, &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 3"), "3 sheets must produce 3 pages");
    }

    #[test]
    fn batch_preserves_per_sheet_spill() {
        // Sheet 0: 1 page. Sheet 1: 160 questions (> 5 cols × 30) → 2 pages. Total = 3.
        let mut big = basic_spec();
        big.answers.count = 160;
        let specs = vec![basic_spec(), big];
        let pdf = render_answer_sheets(&specs, &ready_ctx()).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 3"), "1 + 2 pages expected, got:\n{}",
            &text[..text.find("/Kids").unwrap_or(text.len().min(400))]);
    }

    #[test]
    fn batch_matches_single_for_one_element() {
        let spec = basic_spec();
        let single = render_answer_sheet(&spec, &ready_ctx()).unwrap();
        let batch  = render_answer_sheets(std::slice::from_ref(&spec), &ready_ctx()).unwrap();
        assert_eq!(single, batch, "single render must equal a 1-element batch");
    }

    #[test]
    fn batch_fails_with_offending_index() {
        // Second sheet references a logo that was never registered.
        let mut bad = basic_spec();
        bad.header.logo_key = Some("client_logo".into());
        let specs = vec![basic_spec(), bad, basic_spec()];
        let err = render_answer_sheets(&specs, &ready_ctx()).unwrap_err();
        match err {
            PipelineError::AnswerSheetValidationFailed { index, errors } => {
                assert_eq!(index, 1, "the second sheet is the invalid one");
                assert!(errors.iter().any(|e| matches!(e, ValidationError::MissingImage { .. })));
            }
            other => panic!("expected AnswerSheetValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn empty_batch_renders_valid_pdf() {
        let pdf = render_answer_sheets(&[], &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let _ = page_count(&pdf); // keep helper referenced across cfgs
    }

    #[test]
    fn many_questions_spill_to_second_page() {
        let mut spec = basic_spec();
        spec.answers.count = 160; // > 5 columns × 30 rows
        let pdf = render_answer_sheet(&spec, &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"), "160 questions must produce 2 pages");
    }

    #[test]
    fn empty_spec_renders_blank_template() {
        let spec = AnswerSheetSpec::default();
        let pdf = render_answer_sheet(&spec, &ready_ctx()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
