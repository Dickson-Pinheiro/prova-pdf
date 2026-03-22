//! Orchestrates the 4-phase PDF generation pipeline.
//!
//! Phase 1: Validation   — check required fields and font availability
//! Phase 2: Style cascade — resolve PrintConfig → Section → Question → Inline
//! Phase 3: Layout       — lay out all elements into positioned fragments
//! Phase 4: Emission     — write PDF bytes from fragments

pub mod style;
pub mod validate;

use std::collections::HashMap;
use std::rc::Rc;

use thiserror::Error;

use crate::fonts::resolve::FontResolver;
use crate::fonts::{FontRegistry, FontRules};
use crate::layout::appendix::{layout_appendix, RenderedAppendixItem};
use crate::layout::fragment::Fragment;
use crate::layout::header::layout_header;
use crate::layout::page::{PageComposer, PageGeometry};
use crate::layout::question::{layout_question, ColumnGeometry};
use crate::layout::running::layout_running_overlay;
use crate::layout::section::{layout_section_header, RenderedSectionItem};
use crate::pdf::emit::PdfEmitter;
use crate::spec::ExamSpec;
use crate::spec::config::PrintConfig;
use crate::spec::question::QuestionKind;

use self::validate::ValidationError;

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("validation failed with {} error(s)", .0.len())]
    ValidationFailed(Vec<ValidationError>),
    #[error("no font registered: call add_font('body', 0, bytes) first")]
    NoFont,
    #[error("PDF emission error: {0}")]
    EmissionError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// RenderContext
// ─────────────────────────────────────────────────────────────────────────────

pub struct RenderContext {
    pub registry: FontRegistry,
    pub rules:    FontRules,
    pub images:   HashMap<String, Vec<u8>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Render an `ExamSpec` to PDF bytes through 4 phases:
///
/// 1. **Validation** — all errors are collected in one pass; any error aborts.
/// 2. **Layout** — institutional header, sections, questions, appendix, running
///    headers/footers are composed into `Vec<Vec<Fragment>>` (one vec per page).
/// 3. **Emission** — font subsetting, image embedding, and PDF serialisation.
pub fn render(
    spec: &ExamSpec,
    ctx:  &RenderContext,
) -> Result<Vec<u8>, PipelineError> {
    // ── Phase 1: Validation ───────────────────────────────────────────────────
    let errors = validate::validate(spec, &ctx.registry, &ctx.images);
    if !errors.is_empty() {
        return Err(PipelineError::ValidationFailed(errors));
    }

    // ── Phases 2 + 3: Layout ─────────────────────────────────────────────────
    // Economy mode overrides: force 2 columns + allow enunciation breaks
    // (matches lize HTML behavior — no spacing changes, just layout policy).
    // Clone only PrintConfig (not the entire ExamSpec with all questions).
    let effective_config = if spec.config.economy_mode {
        let mut c = spec.config.clone();
        c.columns = 2;
        c.break_enunciation = true;
        c
    } else {
        spec.config.clone()
    };
    let resolver = FontResolver::new(&ctx.registry, &ctx.rules);
    let geometry = PageGeometry::from_config(&effective_config);
    let pages    = layout_exam(spec, &effective_config, &resolver, &geometry, &ctx.images)?;

    // ── Phase 4: Emission ─────────────────────────────────────────────────────
    let emitter = PdfEmitter::new(&ctx.registry, &ctx.images);
    emitter.emit(pages, &geometry)
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout orchestrator (Phase 3)
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out a complete exam into pages of positioned fragments.
///
/// Order of operations:
/// 1. Institutional header (always on page 1).
/// 2. For each section: section header, then each question block.
/// 3. Appendix (starts on a new page when present).
/// 4. Running header / footer overlays applied to every page.
fn layout_exam(
    spec:     &ExamSpec,
    config:   &PrintConfig,
    resolver: &FontResolver<'_>,
    geometry: &PageGeometry,
    images:   &HashMap<String, Vec<u8>>,
) -> Result<Vec<Vec<Fragment>>, PipelineError> {
    let mut composer = PageComposer::new(geometry.clone());

    // ── Institutional header ──────────────────────────────────────────────────
    let header_height: f64;
    {
        let (frags, h) = layout_header(
            &spec.header, resolver, geometry, images,
            config.font_size, config.line_spacing.multiplier(),
        );
        header_height = h;
        if h > 0.0 {
            composer.push_block_full_width(h, frags);
        }
    }

    // ── Sections and questions ────────────────────────────────────────────────
    let mut q_number: u32 = 1;

    for section in &spec.sections {
        let col_geom = ColumnGeometry::from_page(geometry);

        // Section header (category badge, title, instructions, SectionTop texts).
        for item in layout_section_header(section, resolver, &col_geom, config) {
            match item {
                RenderedSectionItem::PageBreak => composer.new_page(),
                RenderedSectionItem::Block { fragments, height } => {
                    composer.push_block(height, fragments);
                }
            }
        }

        // Questions.
        for question in &section.questions {
            // Honour per-question page break and global break_all_questions.
            if question.force_page_break || config.break_all_questions {
                composer.force_break();
            }

            // Essay questions always span full width (redação ocupa a folha inteira).
            let fw = question.full_width || question.kind == QuestionKind::Essay;
            let col_geom = composer.column_geom_for(fw);
            let (frags, height) = layout_question(question, q_number, resolver, &col_geom, config);

            if fw {
                composer.push_block_full_width(height, frags);
            } else {
                composer.push_block(height, frags);
            }

            q_number += 1;
        }
    }

    // ── Appendix ──────────────────────────────────────────────────────────────
    if let Some(ref appendix) = spec.appendix {
        composer.new_page();
        let col_geom = ColumnGeometry::from_page(geometry);
        for item in layout_appendix(appendix, resolver, &col_geom, config) {
            match item {
                RenderedAppendixItem::PageBreak => composer.new_page(),
                RenderedAppendixItem::Block { fragments, height } => {
                    composer.push_block(height, fragments);
                }
            }
        }
    }

    // ── Finalise pages ────────────────────────────────────────────────────────
    let (mut pages, fw_ranges_per_page) = composer.finalize();

    // ── Column rule (vertical separator between columns) ─────────────────────
    // The rule is split into segments that skip full-width blocks (header,
    // essay sheets, etc.) so the line never crosses through them.
    if geometry.columns > 1 {
        let rule_color = if config.all_black {
            "#000000".to_owned()
        } else {
            "#3b4863".to_owned()
        };
        let rule_x = geometry.column_x(1) - geometry.column_gap_pt / 2.0;

        for (page_idx, page_frags) in pages.iter_mut().enumerate() {
            // Collect exclusion zones: header on page 0 + any full-width blocks.
            let mut exclusions: Vec<(f64, f64)> = Vec::new();
            if page_idx == 0 && header_height > 0.0 {
                exclusions.push((0.0, header_height));
            }
            if let Some(ranges) = fw_ranges_per_page.get(page_idx) {
                exclusions.extend(ranges.iter().copied());
            }
            exclusions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            // Build visible segments by subtracting exclusions from [0, content_height].
            let content_h = geometry.content_height_pt;
            let mut segments: Vec<(f64, f64)> = Vec::new();
            let mut cursor = 0.0_f64;
            for (ex_start, ex_end) in &exclusions {
                if *ex_start > cursor {
                    segments.push((cursor, *ex_start));
                }
                cursor = cursor.max(*ex_end);
            }
            if cursor < content_h {
                segments.push((cursor, content_h));
            }

            // Emit one VRule fragment per visible segment.
            for (seg_y, seg_end) in segments {
                let seg_h = seg_end - seg_y;
                if seg_h < 1.0 { continue; }
                page_frags.push(Fragment {
                    x:      rule_x,
                    y:      seg_y,
                    width:  1.0,
                    height: seg_h,
                    kind:   crate::layout::fragment::FragmentKind::VRule(
                        crate::layout::fragment::VRule {
                            stroke_width: 0.75,
                            color:        rule_color.clone(),
                        },
                    ),
                });
            }
        }
    }

    // ── Running header / footer overlays ─────────────────────────────────────
    let total_pages = pages.len() as u32;
    for (i, page_frags) in pages.iter_mut().enumerate() {
        let page_num = i as u32 + 1;

        if let Some(ref rh) = spec.header.running_header {
            let overlay = layout_running_overlay(
                rh, resolver, geometry, page_num, total_pages, false,
            );
            page_frags.extend(overlay);
        }

        if let Some(ref rf) = spec.header.running_footer {
            let overlay = layout_running_overlay(
                rf, resolver, geometry, page_num, total_pages, true,
            );
            page_frags.extend(overlay);
        }
    }

    // ── all_black post-processing ──────────────────────────────────────────
    if config.all_black {
        apply_all_black(&mut pages);
    }

    Ok(pages)
}

/// Force all fragment colours to `#000000` (economy / SEDUC mode).
///
/// Exceptions:
/// - `GlyphRun` with `#ffffff` stays white (badge text on black circle).
/// - `FilledRect` with a decorative background (not black, not white) becomes
///   `#ffffff` so the stripe disappears instead of turning into a black block.
fn apply_all_black(pages: &mut [Vec<Fragment>]) {
    use crate::layout::fragment::FragmentKind;

    const BLACK: &str = "#000000";
    const WHITE: &str = "#ffffff";

    for page in pages.iter_mut() {
        for frag in page.iter_mut() {
            match &mut frag.kind {
                FragmentKind::GlyphRun(r) => {
                    if !r.color.eq_ignore_ascii_case(WHITE) {
                        r.color = Rc::from(BLACK);
                    }
                }
                FragmentKind::HRule(r) => r.color = BLACK.to_owned(),
                FragmentKind::VRule(r) => r.color = BLACK.to_owned(),
                FragmentKind::StrokedRect(r) => r.color = BLACK.to_owned(),
                FragmentKind::FilledCircle(r) => r.color = BLACK.to_owned(),
                FragmentKind::FilledRect(r) => {
                    // Keep existing black or white; turn decorative colours
                    // (stripes, tinted backgrounds) into white so they vanish.
                    if !r.color.eq_ignore_ascii_case(BLACK)
                        && !r.color.eq_ignore_ascii_case(WHITE)
                    {
                        r.color = WHITE.to_owned();
                    }
                }
                FragmentKind::Image(_) | FragmentKind::Spacer => {}
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::registry::FontRules;
    use crate::layout::fragment::FragmentKind;
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn ready_ctx() -> RenderContext {
        let (registry, rules) = make_resolver_and_rules();
        RenderContext {
            registry,
            rules,
            images: HashMap::new(),
        }
    }

    // ── Validation path ───────────────────────────────────────────────────────

    #[test]
    fn render_fails_with_no_font() {
        let ctx  = RenderContext {
            registry: FontRegistry::new(),
            rules:    FontRules::default(),
            images:   HashMap::new(),
        };
        let spec = ExamSpec::default();
        let err  = render(&spec, &ctx).unwrap_err();
        assert!(matches!(err, PipelineError::ValidationFailed(_)),
            "empty registry must produce ValidationFailed");
    }

    #[test]
    fn render_fails_with_no_sections() {
        let ctx  = ready_ctx();
        let spec = ExamSpec::default(); // no sections
        let err  = render(&spec, &ctx).unwrap_err();
        assert!(matches!(err, PipelineError::ValidationFailed(_)));
    }

    #[test]
    fn validation_error_carries_all_errors() {
        let ctx = RenderContext {
            registry: FontRegistry::new(),
            rules:    FontRules::default(),
            images:   HashMap::new(),
        };
        let spec = ExamSpec::default(); // no font + no sections = 2 errors
        match render(&spec, &ctx).unwrap_err() {
            PipelineError::ValidationFailed(errs) => assert_eq!(errs.len(), 2),
            other => panic!("expected ValidationFailed, got {other:?}"),
        }
    }

    // ── Successful render ─────────────────────────────────────────────────────

    fn all_kinds_spec() -> ExamSpec {
        let json = include_str!("../../tests/fixtures/all_kinds.json");
        serde_json::from_str(json).expect("all_kinds.json must be valid ExamSpec")
    }

    #[test]
    fn all_kinds_renders_valid_pdf() {
        let ctx   = ready_ctx();
        let spec  = all_kinds_spec();
        let bytes = render(&spec, &ctx).expect("all_kinds.json must render without error");
        assert!(bytes.starts_with(b"%PDF-"), "output must start with %PDF-");
        assert!(!bytes.is_empty(), "output must not be empty");
        let tail = &bytes[bytes.len().saturating_sub(10)..];
        assert!(tail.windows(5).any(|w| w == b"%%EOF"), "output must end with %%EOF");
    }

    #[test]
    fn all_kinds_produces_multi_page_pdf() {
        let ctx   = ready_ctx();
        let spec  = all_kinds_spec();
        let bytes = render(&spec, &ctx).unwrap();
        // 6 questions of different kinds should span at least 1 page.
        assert!(bytes.len() > 500, "PDF must have substantial content");
    }

    #[test]
    fn render_with_running_header_produces_valid_pdf() {
        use crate::spec::answer::{AnswerSpace, TextualAnswer};
        use crate::spec::exam::Section;
        use crate::spec::header::{InstitutionalHeader, RunningHeader};
        use crate::spec::inline::{InlineContent, InlineText};
        use crate::spec::question::{Question, QuestionKind};

        let ctx = ready_ctx();
        let spec = ExamSpec {
            header: InstitutionalHeader {
                running_header: Some(RunningHeader {
                    left:   Some("{page}/{pages}".into()),
                    center: None,
                    right:  None,
                }),
                running_footer: None,
                ..Default::default()
            },
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
        };
        let bytes = render(&spec, &ctx).expect("must render with running header");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn render_empty_section_is_rejected() {
        use crate::spec::exam::Section;
        let ctx  = ready_ctx();
        let spec = ExamSpec {
            sections: vec![Section {
                questions:        vec![],
                title:            None,
                instructions:     vec![],
                category:         None,
                style:            None,
                force_page_break: false,
            }],
            ..ExamSpec::default()
        };
        let err = render(&spec, &ctx).unwrap_err();
        assert!(matches!(err, PipelineError::ValidationFailed(_)));
    }

    // ── apply_all_black ──────────────────────────────────────────────────────

    fn make_glyph_run_frag(color: &str) -> Fragment {
        use crate::layout::fragment::GlyphRun;
        Fragment {
            x: 0.0, y: 0.0, width: 10.0, height: 10.0,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids: vec![], x_advances: vec![], x_offsets: vec![], y_offsets: vec![],
                font_size: 12.0, font_family: Rc::from("body"), variant: 0,
                color: Rc::from(color), baseline_offset: 10.0,
            }),
        }
    }

    #[test]
    fn all_black_forces_glyph_run_to_black() {
        let mut pages = vec![vec![make_glyph_run_frag("#ff0000")]];
        apply_all_black(&mut pages);
        match &pages[0][0].kind {
            FragmentKind::GlyphRun(r) => assert_eq!(&*r.color, "#000000"),
            _ => panic!("expected GlyphRun"),
        }
    }

    #[test]
    fn all_black_preserves_white_badge_text() {
        let mut pages = vec![vec![make_glyph_run_frag("#ffffff")]];
        apply_all_black(&mut pages);
        match &pages[0][0].kind {
            FragmentKind::GlyphRun(r) => assert_eq!(&*r.color, "#ffffff"),
            _ => panic!("expected GlyphRun"),
        }
    }

    #[test]
    fn all_black_forces_hrule_color() {
        use crate::layout::fragment::HRule;
        let mut pages = vec![vec![Fragment {
            x: 0.0, y: 0.0, width: 100.0, height: 1.0,
            kind: FragmentKind::HRule(HRule { stroke_width: 0.5, color: "#C2C2C2".into() }),
        }]];
        apply_all_black(&mut pages);
        match &pages[0][0].kind {
            FragmentKind::HRule(r) => assert_eq!(r.color, "#000000"),
            _ => panic!("expected HRule"),
        }
    }

    #[test]
    fn all_black_turns_decorative_filled_rect_white() {
        use crate::layout::fragment::FilledRect;
        let mut pages = vec![vec![Fragment {
            x: 0.0, y: 0.0, width: 100.0, height: 20.0,
            kind: FragmentKind::FilledRect(FilledRect { color: "#F3F4F7".into() }),
        }]];
        apply_all_black(&mut pages);
        match &pages[0][0].kind {
            FragmentKind::FilledRect(r) => assert_eq!(r.color, "#ffffff"),
            _ => panic!("expected FilledRect"),
        }
    }

    #[test]
    fn all_black_keeps_black_filled_rect() {
        use crate::layout::fragment::FilledRect;
        let mut pages = vec![vec![Fragment {
            x: 0.0, y: 0.0, width: 100.0, height: 20.0,
            kind: FragmentKind::FilledRect(FilledRect { color: "#000000".into() }),
        }]];
        apply_all_black(&mut pages);
        match &pages[0][0].kind {
            FragmentKind::FilledRect(r) => assert_eq!(r.color, "#000000"),
            _ => panic!("expected FilledRect"),
        }
    }

    #[test]
    fn all_black_render_produces_valid_pdf() {
        let ctx  = ready_ctx();
        let mut spec = all_kinds_spec();
        spec.config.all_black = true;
        let bytes = render(&spec, &ctx).expect("all_black render must succeed");
        assert!(bytes.starts_with(b"%PDF-"));
    }
}
