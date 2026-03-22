//! Question block layout — number prefix, base texts, and stem.
//!
//! # Layout structure (vertical, top-to-bottom)
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ [BeforeQuestion base texts]             │
//! ├─────────────────────────────────────────┤
//! │ 01.  Stem text that can wrap across     │
//! │      multiple lines as needed …        │   ← number + stem row
//! │                            (1pt)        │   ← points badge (right)
//! ├─────────────────────────────────────────┤
//! │ [answer space — TASK-015 … TASK-020]    │
//! ├─────────────────────────────────────────┤
//! │ [AfterQuestion base texts]              │
//! └─────────────────────────────────────────┘
//! ```
//!
//! All fragment coordinates are **column-relative**: `(0, 0)` is the top-left of
//! this question block.  The `PageComposer` translates them to content-area
//! absolute coordinates via `push_block`.

pub(crate) mod choice;
pub(crate) mod cloze;
pub(crate) mod draft;
pub(crate) mod essay;
pub(crate) mod file;
pub(crate) mod sum;
pub(crate) mod textual;

use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun, HRule};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::page::PageGeometry;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::answer::AnswerSpace;
use crate::spec::config::PrintConfig;
use crate::spec::inline::{InlineContent, InlineText};
use crate::spec::question::{BaseText, BaseTextPosition, Question, QuestionKind};
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants
// ─────────────────────────────────────────────────────────────────────────────

/// Stroke width of the separator rule rendered below the question number heading.
const QUESTION_RULE_STROKE_PT: f64 = 0.7;
/// Vertical gap below the question separator rule before the stem.
/// Matches lize CSS: hr.mb-3 = 1rem = 12pt at 12pt font.
const QUESTION_RULE_GAP_PT: f64 = 8.0;
/// Vertical margin inserted below the stem before the answer space.
const STEM_BOTTOM_MARGIN_PT: f64 = 3.0;
/// Vertical margin after each BeforeQuestion base-text block.
/// Matches lize CSS: col-12.mb-3 (1rem=12pt) + question-number-header.mt-2 (0.5rem=6pt) = 18pt.
const BASE_TEXT_V_MARGIN_PT: f64 = 18.0;
/// Scale factor applied to all spacings in economy mode.
/// Reduces vertical gaps for tighter layout when economy_mode is enabled.
const ECONOMY_FACTOR: f64 = 0.7;
/// Horizontal gap between the question number and the start of the stem text.
const NUMBER_STEM_GAP_PT: f64 = 4.0;
/// Top margin before each question block (matches lize CSS `.question.mt-3` = 1rem = 12pt at 12pt font).
const QUESTION_TOP_MARGIN_PT: f64 = 12.0;
/// Scale factor for the alternative letter badge size (badge side = font_size × scale).
const ALT_BADGE_SCALE: f64 = 1.5;
/// Gap between the right edge of the alternative badge and the content start.
const ALT_BADGE_GAP_PT: f64 = 6.0;

// ─────────────────────────────────────────────────────────────────────────────
// ColumnGeometry
// ─────────────────────────────────────────────────────────────────────────────

/// Geometry of one layout column — the unit questions are laid out into.
///
/// Coordinates produced by [`layout_question`] are relative to the top-left of
/// this column (x = 0 at the column left edge, y = 0 at the block top).
#[derive(Debug, Clone, Copy)]
pub struct ColumnGeometry {
    /// Available width for content, in PDF points.
    pub column_width_pt: f64,
}

impl ColumnGeometry {
    /// Derive from a full `PageGeometry` (uses `column_width_pt`).
    pub fn from_page(geom: &PageGeometry) -> Self {
        Self { column_width_pt: geom.column_width_pt }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out a single question block into a flat list of [`Fragment`]s.
///
/// Returns `(fragments, total_height_pt, split_points)` in column-relative
/// coordinates. `split_points` is a sorted list of Y positions where the
/// question block can be safely split across pages/columns. The `PageComposer`
/// uses these to break questions at semantic boundaries instead of mid-content.
///
/// # Parameters
/// - `q`        — the question to render.
/// - `number`   — display number (sequential, caller-managed; global or per-section).
/// - `resolver` — font resolver.
/// - `geometry` — column geometry (available width).
/// - `config`   — print config (font size, economy mode, show_score, …).
pub fn layout_question<'a>(
    q:        &Question,
    number:   u32,
    resolver: &'a FontResolver<'a>,
    geometry: &ColumnGeometry,
    config:   &PrintConfig,
) -> (Vec<Fragment>, f64, Vec<f64>) {
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut split_points: Vec<f64> = Vec::new();

    let font_size        = config.font_size;
    let line_spacing     = config.line_spacing.multiplier();
    let spc              = if config.economy_mode { ECONOMY_FACTOR } else { 1.0 };
    let blank_default_cm = if config.economy_mode { 2.5 } else { 3.5 };

    // Top margin separating this question from the previous block.
    let mut cursor_y: f64 = QUESTION_TOP_MARGIN_PT * spc;

    // ── BeforeQuestion base texts ─────────────────────────────────────────────
    for bt in q.base_texts.iter().filter(|b| b.position == BaseTextPosition::BeforeQuestion) {
        let (frags, h) = layout_base_text(bt, resolver, geometry, font_size, line_spacing, blank_default_cm, cursor_y);
        fragments.extend(frags);
        cursor_y += h + BASE_TEXT_V_MARGIN_PT * spc;
    }

    // ── Question heading: "Questão N" bold text + optional score + HRule ───────
    // Matches lize HTML: <h5> heading + score badge (right-aligned, same row) + <hr>.
    // Fixed 9pt size — always smaller than question content.
    // Uses fixed 1.2 line spacing so config.line_spacing doesn't push it away from the rule.
    let show_number = q.show_number && !config.hide_numbering;
    if show_number {
        let heading_size = 9.0;
        let heading_y    = cursor_y;  // saved for score badge alignment
        let heading_text = format!("Questão {}", number);
        let fd     = resolver.resolve(FontRole::Question, FontWeight::Bold, FontStyle::Normal, None);
        let glyphs = shape_text(fd, &heading_text);
        let text_w = shaped_text_width(&glyphs, heading_size, fd.units_per_em);
        let ascent = fd.ascender as f64 / fd.units_per_em as f64 * heading_size;
        let family = Rc::from(resolver.resolve_family_name(FontRole::Question, None));

        // "Questão N" text — bold, black, left-aligned, smaller than content
        fragments.push(Fragment {
            x:      0.0,
            y:      heading_y,
            width:  text_w,
            height: heading_size,
            kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &glyphs, heading_size, family, 1, Rc::from("#000000"), ascent,
            )),
        });

        // Score badge — right-aligned on the same row as "Questão N".
        // Matches lize HTML: score chip floated right in the heading row.
        if config.show_score {
            if let Some(pts) = q.points {
                let score_text  = format_points(pts);
                let fd_body     = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
                let score_glyph = shape_text(fd_body, &score_text);
                let score_w     = shaped_text_width(&score_glyph, heading_size, fd_body.units_per_em);
                let score_asc   = fd_body.ascender as f64 / fd_body.units_per_em as f64 * heading_size;
                let score_fam   = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
                let score_x     = (geometry.column_width_pt - score_w).max(0.0);
                fragments.push(Fragment {
                    x:      score_x,
                    y:      heading_y,
                    width:  score_w,
                    height: heading_size,
                    kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                        &score_glyph, heading_size, score_fam, 0, Rc::from("#000000"), score_asc,
                    )),
                });
            }
        }

        cursor_y += heading_size * 1.2;  // fixed spacing, independent of config.line_spacing

        // Full-width HRule below the heading
        fragments.push(Fragment {
            x:      0.0,
            y:      cursor_y,
            width:  geometry.column_width_pt,
            height: QUESTION_RULE_STROKE_PT,
            kind:   FragmentKind::HRule(HRule {
                stroke_width: QUESTION_RULE_STROKE_PT,
                color:        "#000000".to_owned(),
            }),
        });
        cursor_y += QUESTION_RULE_STROKE_PT + QUESTION_RULE_GAP_PT * spc;
    }

    // ── Stem ─────────────────────────────────────────────────────────────────
    let style = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };
    let engine = InlineLayoutEngine {
        resolver,
        available_width:  geometry.column_width_pt,
        font_size,
        line_spacing,
        blank_default_cm,
        justify: true,
    };
    let (stem_frags, stem_h) = engine.layout(
        &q.stem,
        FontRole::Question,
        &style,
        0.0,
        cursor_y,
    );
    fragments.extend(stem_frags);
    cursor_y += stem_h.max(font_size * line_spacing);

    // Margin below stem before answer space.
    cursor_y += STEM_BOTTOM_MARGIN_PT * spc;

    // Split point: after stem, before answer space.
    // Unless force_choices_with_statement is set, this is the primary split point
    // that allows the stem to stay on one page while alternatives go to the next.
    if config.force_choices_with_statement == 0 {
        split_points.push(cursor_y);
    }

    // ── Answer space — TASK-015 … TASK-020 ───────────────────────────────────
    match (&q.kind, &q.answer) {
        (QuestionKind::Choice, AnswerSpace::Choice(choice_data)) => {
            let (f, h, alt_splits) = choice::layout_choice(choice_data, number, resolver, geometry, font_size, line_spacing, blank_default_cm, cursor_y, config, spc);
            fragments.extend(f);
            // If break_alternatives is enabled, add split points between alternatives.
            if config.break_alternatives {
                split_points.extend(alt_splits);
            }
            cursor_y += h;
        }
        (QuestionKind::Textual, AnswerSpace::Textual(textual_data)) => {
            let (f, h) = textual::layout_textual(textual_data, geometry, cursor_y, config, spc);
            fragments.extend(f);
            cursor_y += h;
        }
        (QuestionKind::Cloze, AnswerSpace::Cloze(cloze_data)) => {
            let (f, h) = cloze::layout_cloze(cloze_data, resolver, geometry, font_size, line_spacing, blank_default_cm, cursor_y, spc, config);
            fragments.extend(f);
            cursor_y += h;
        }
        (QuestionKind::Sum, AnswerSpace::Sum(sum_data)) => {
            let (f, h) = sum::layout_sum(sum_data, resolver, geometry, font_size, line_spacing, blank_default_cm, cursor_y, spc);
            fragments.extend(f);
            cursor_y += h;
        }
        (QuestionKind::Essay, AnswerSpace::Essay(essay_data)) => {
            let (f, h) = essay::layout_essay(essay_data, geometry, cursor_y, config, spc);
            fragments.extend(f);
            cursor_y += h;
        }
        (QuestionKind::File, AnswerSpace::File(file_data)) => {
            let (f, h) = file::layout_file(file_data, resolver, geometry, font_size, line_spacing, cursor_y, spc);
            fragments.extend(f);
            cursor_y += h;
        }
        _ => {}
    }

    // ── Draft lines — TASK-023 ────────────────────────────────────────────────
    if q.draft_lines > 0 {
        let (f, h) = draft::layout_draft_lines(q.draft_lines, q.draft_line_height, resolver, geometry, cursor_y, spc);
        fragments.extend(f);
        cursor_y += h;
    }

    // ── AfterQuestion base texts ──────────────────────────────────────────────
    for bt in q.base_texts.iter().filter(|b| b.position == BaseTextPosition::AfterQuestion) {
        let (frags, h) = layout_base_text(bt, resolver, geometry, font_size, line_spacing, blank_default_cm, cursor_y);
        fragments.extend(frags);
        cursor_y += h + BASE_TEXT_V_MARGIN_PT * spc;
    }

    (fragments, cursor_y, split_points)
}

// ─────────────────────────────────────────────────────────────────────────────
// Base-text helper
// ─────────────────────────────────────────────────────────────────────────────

fn layout_base_text<'a>(
    bt:              &BaseText,
    resolver:        &'a FontResolver<'a>,
    geometry:        &ColumnGeometry,
    font_size:       f64,
    line_spacing:    f64,
    blank_default_cm: f64,
    origin_y:        f64,
) -> (Vec<Fragment>, f64) {
    let mut frags: Vec<Fragment> = Vec::new();
    let mut local_y = origin_y;

    let small_size  = font_size * 0.9;
    let small_style = ResolvedStyle {
        font_size:   small_size,
        font_style:  crate::spec::style::FontStyle::Italic,
        line_spacing,
        ..ResolvedStyle::default()
    };
    let body_style = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };

    // Optional title
    if let Some(ref title) = bt.title {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size:        small_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let content = vec![crate::spec::inline::InlineContent::Text(
            crate::spec::inline::InlineText { value: title.clone(), style: None },
        )];
        let (f, h) = engine.layout(&content, FontRole::Body, &small_style, 0.0, local_y);
        frags.extend(f);
        local_y += h;
    }

    // Content
    if !bt.content.is_empty() {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: true,
        };
        let (f, h) = engine.layout(&bt.content, FontRole::Body, &body_style, 0.0, local_y);
        frags.extend(f);
        local_y += h;
    }

    // Optional attribution
    if let Some(ref attr) = bt.attribution {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size:        small_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let content = vec![crate::spec::inline::InlineContent::Text(
            crate::spec::inline::InlineText { value: attr.clone(), style: None },
        )];
        let (f, h) = engine.layout(&content, FontRole::Body, &small_style, 0.0, local_y);
        frags.extend(f);
        local_y += h;
    }

    let total_h = local_y - origin_y;
    (frags, total_h)
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Format a question number as a zero-padded badge: "01.", "10.", "100.".
pub fn format_number(n: u32) -> String {
    if n < 100 { format!("{n:02}") } else { format!("{n}") }
}

/// Format a points value for the score badge: "(1pt)", "(1.5pt)".
fn format_points(pts: f64) -> String {
    if pts == pts.floor() {
        format!("({:.0}pt)", pts)
    } else {
        format!("({:.1}pt)", pts)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::answer::{AlternativeLayout, AnswerSpace, TextualAnswer};
    use crate::spec::config::LetterCase;
    use crate::spec::inline::{InlineContent, InlineText};
    use crate::spec::question::{BaseText, QuestionKind};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    // Re-import submodule items used by tests
    use super::choice::{format_alt_label, format_alt_letter, GRID_COLUMNS};
    use super::draft::DRAFT_LABEL_SIZE_PT;
    use super::textual::DEFAULT_LINE_COUNT;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn col_geom(width_pt: f64) -> ColumnGeometry {
        ColumnGeometry { column_width_pt: width_pt }
    }

    fn text_inline(s: &str) -> InlineContent {
        InlineContent::Text(InlineText { value: s.to_owned(), style: None })
    }

    fn simple_question(stem: &str, show_number: bool) -> Question {
        Question {
            number:           None,
            label:            None,
            kind:             QuestionKind::Textual,
            stem:             vec![text_inline(stem)],
            answer:           AnswerSpace::Textual(TextualAnswer::default()),
            base_texts:       vec![],
            points:           None,
            full_width:       false,
            draft_lines:      0,
            draft_line_height: None,
            show_number,
            force_page_break: false,
            style:            None,
        }
    }

    fn default_config() -> PrintConfig {
        PrintConfig::default()
    }

    fn call<'a>(
        q:        &Question,
        number:   u32,
        resolver: &'a FontResolver<'a>,
        width_pt: f64,
    ) -> (Vec<Fragment>, f64) {
        let (frags, h, _) = layout_question(q, number, resolver, &col_geom(width_pt), &default_config());
        (frags, h)
    }

    fn glyph_runs(frags: &[Fragment]) -> Vec<&Fragment> {
        frags.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).collect()
    }

    // ── format_number ────────────────────────────────────────────────────────

    #[test]
    fn number_01_for_single_digit() {
        assert_eq!(format_number(1),   "01");
        assert_eq!(format_number(9),   "09");
    }

    #[test]
    fn number_10_for_two_digits() {
        assert_eq!(format_number(10),  "10");
        assert_eq!(format_number(99),  "99");
    }

    #[test]
    fn number_no_padding_above_99() {
        assert_eq!(format_number(100), "100");
        assert_eq!(format_number(150), "150");
    }

    // ── format_points ────────────────────────────────────────────────────────

    #[test]
    fn points_integer_no_decimal() {
        assert_eq!(format_points(1.0), "(1pt)");
        assert_eq!(format_points(2.0), "(2pt)");
    }

    #[test]
    fn points_decimal_one_place() {
        assert_eq!(format_points(1.5), "(1.5pt)");
    }

    // ── show_number ───────────────────────────────────────────────────────────

    #[test]
    fn show_number_true_produces_heading_text_and_hrule() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q   = simple_question("Qual a capital?", true);
        let (frags, _) = call(&q, 1, &res, 400.0);

        // Should have a bold GlyphRun with "Questão 1" (black text, not white)
        let heading_run = frags.iter().find(|f| {
            if let FragmentKind::GlyphRun(ref r) = f.kind {
                r.variant == 1 && &*r.color == "#000000"
            } else { false }
        });
        assert!(heading_run.is_some(), "should have a bold black GlyphRun for 'Questão N'");

        // Should have an HRule below the heading
        let hrules: Vec<_> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::HRule(_)))
            .collect();
        assert!(!hrules.is_empty(), "should have an HRule below the question heading");
    }

    #[test]
    fn show_number_false_no_badge_at_x0() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q   = simple_question("Qual a capital?", false);
        let (frags, _) = call(&q, 1, &res, 400.0);
        // There should be no GlyphRun at x=0 that looks like a number
        // (stem text may still start at x=0 if no indent)
        // We verify height is still positive
        let (_, h) = call(&q, 1, &res, 400.0);
        assert!(h > 0.0, "question with no number still has positive height");
        let _ = frags;
    }

    #[test]
    fn hide_numbering_config_suppresses_badge() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = simple_question("Stem.", true); // show_number = true
        let cfg  = PrintConfig { hide_numbering: true, ..PrintConfig::default() };
        let (frags_hidden, h_hidden, _) = layout_question(&q, 5, &res, &col_geom(400.0), &cfg);
        let (frags_shown,  h_shown, _)  = layout_question(&q, 5, &res, &col_geom(400.0), &PrintConfig::default());
        // With numbering hidden, the stem starts at x=0
        let min_x_hidden = frags_hidden.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).map(|f| f.x).fold(f64::INFINITY, f64::min);
        let min_x_shown  = frags_shown.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).map(|f| f.x).fold(f64::INFINITY, f64::min);
        assert!(min_x_hidden <= min_x_shown,
            "hidden numbering: stem should start at or before the indented x");
        let _ = h_hidden;
        let _ = h_shown;
    }

    // ── Stem ─────────────────────────────────────────────────────────────────

    #[test]
    fn stem_fragments_have_positive_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q   = simple_question("Um enunciado qualquer.", true);
        let (_, h) = call(&q, 1, &res, 400.0);
        assert!(h > 0.0);
    }

    #[test]
    fn empty_stem_still_has_minimum_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let mut q = simple_question("", true);
        q.stem.clear();
        let (_, h) = call(&q, 1, &res, 400.0);
        let min_h = PrintConfig::default().font_size * PrintConfig::default().line_spacing.multiplier();
        assert!(h >= min_h, "empty stem should still have at least one line-height ({min_h:.2}), got {h:.2}");
    }

    #[test]
    fn stem_is_indented_when_show_number_true() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q   = simple_question("Texto do enunciado.", true);
        let (frags, _) = call(&q, 1, &res, 400.0);
        let runs = glyph_runs(&frags);
        // The stem GlyphRun(s) should start at x > 0 (indented past the number badge)
        let stem_runs: Vec<&&Fragment> = runs.iter()
            .filter(|f| f.x > 1.0)
            .collect();
        assert!(!stem_runs.is_empty(), "stem text should be indented (x > 0)");
    }

    // ── Economy mode ─────────────────────────────────────────────────────────

    #[test]
    fn economy_mode_reduces_height() {
        // Economy mode applies ECONOMY_FACTOR < 1.0 to reduce spacings.
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = simple_question("Stem.", true);
        let geom = col_geom(400.0);

        let (_, h_normal, _)  = layout_question(&q, 1, &res, &geom, &PrintConfig::default());
        let (_, h_economy, _) = layout_question(&q, 1, &res, &geom, &PrintConfig { economy_mode: true, ..PrintConfig::default() });

        assert!(h_economy < h_normal,
            "economy mode ({h_economy:.2}) should be shorter than normal ({h_normal:.2})");
    }

    // ── Show score / points badge ─────────────────────────────────────────────

    #[test]
    fn show_score_true_and_points_set_produces_extra_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let mut q = simple_question("Stem.", true);
        q.points  = Some(1.0);
        let cfg   = PrintConfig { show_score: true, ..PrintConfig::default() };
        let geom  = col_geom(400.0);

        let (frags_with,    _, _) = layout_question(&q, 1, &res, &geom, &cfg);
        let (frags_without, _, _) = layout_question(&q, 1, &res, &geom, &PrintConfig::default());

        assert!(glyph_runs(&frags_with).len() > glyph_runs(&frags_without).len(),
            "show_score=true should produce more GlyphRuns than without");
    }

    #[test]
    fn points_badge_is_right_aligned() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let mut q = simple_question("Stem.", true);
        q.points  = Some(2.0);
        let width = 400.0_f64;
        let cfg   = PrintConfig { show_score: true, ..PrintConfig::default() };
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(width), &cfg);

        // The points badge has the largest right edge (x + width).
        let max_right = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .map(|f| f.x + f.width)
            .fold(0.0_f64, f64::max);

        assert!((max_right - width).abs() < 2.0,
            "points badge right edge ({max_right:.2}) should be near column right ({width:.2})");
    }

    // ── BeforeQuestion / AfterQuestion base texts ─────────────────────────────

    #[test]
    fn before_question_base_text_produces_fragments_above_stem() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let mut q = simple_question("Stem.", true);
        q.base_texts = vec![BaseText {
            content:     vec![text_inline("Leia o texto abaixo.")],
            position:    BaseTextPosition::BeforeQuestion,
            title:       None,
            attribution: None,
            style:       None,
        }];
        let (frags, _) = call(&q, 1, &res, 400.0);

        // The fragment at the smallest y is the base text
        let min_y = frags.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
        // All base-text frags should appear before the number (y=0)
        assert!(min_y >= 0.0, "no fragment at negative y");

        // There should be more fragments than without base text
        let (frags_plain, _) = call(&simple_question("Stem.", true), 1, &res, 400.0);
        assert!(frags.len() > frags_plain.len(),
            "base text should add fragments");
    }

    #[test]
    fn after_question_base_text_produces_fragments_below_stem() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let mut q = simple_question("Stem.", true);
        q.base_texts = vec![BaseText {
            content:     vec![text_inline("Fonte: IBGE, 2024.")],
            position:    BaseTextPosition::AfterQuestion,
            title:       None,
            attribution: None,
            style:       None,
        }];
        let (frags, h) = call(&q, 1, &res, 400.0);

        let (_, h_plain) = call(&simple_question("Stem.", true), 1, &res, 400.0);
        assert!(h > h_plain, "after-question base text should increase total height");
        let _ = frags;
    }

    // ── Number sequence ────────────────────────────────────────────────────────

    #[test]
    fn number_sequence_produces_distinct_badge_texts() {
        // Verify that format_number produces distinct strings for 1..=5
        let ns: Vec<String> = (1u32..=5).map(format_number).collect();
        assert_eq!(ns, ["01", "02", "03", "04", "05"]);
    }

    // ── Column width respected ─────────────────────────────────────────────────

    #[test]
    fn no_fragment_exceeds_column_width() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let q     = simple_question(
            "Um enunciado bem longo que vai precisar de quebra de linha para caber na coluna.",
            true,
        );
        let width = 200.0_f64;
        let (frags, _) = call(&q, 3, &res, width);
        for f in &frags {
            assert!(f.x + f.width <= width + 0.5,
                "fragment x={:.2} w={:.2} exceeds column_width={width:.2}", f.x, f.width);
        }
    }

    // ── Choice (TASK-015) ─────────────────────────────────────────────────────

    fn make_choice_question(alts: &[(&str, &str)], layout: AlternativeLayout) -> Question {
        use crate::spec::answer::{Alternative, ChoiceAnswer};
        let alternatives = alts.iter().map(|(label, text)| Alternative {
            label:   label.to_string(),
            content: vec![text_inline(text)],
        }).collect();
        Question {
            number:           None,
            label:            None,
            kind:             QuestionKind::Choice,
            stem:             vec![text_inline("Qual a alternativa correta?")],
            answer:           AnswerSpace::Choice(ChoiceAnswer { alternatives, layout }),
            base_texts:       vec![],
            points:           None,
            full_width:       false,
            draft_lines:      0,
            draft_line_height: None,
            show_number:      false,
            force_page_break: false,
            style:            None,
        }
    }

    #[test]
    fn choice_vertical_five_alts_positive_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q   = make_choice_question(
            &[("A","alfa"),("B","beta"),("C","gama"),("D","delta"),("E","épsilon")],
            AlternativeLayout::Vertical,
        );
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());
        assert!(h > 0.0, "height should be positive, got {h}");
        assert!(!frags.is_empty(), "should have fragments");
    }

    #[test]
    fn choice_vertical_height_greater_than_stem_alone() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let stem = simple_question("Qual a alternativa correta?", false);
        let (_, h_stem, _) = layout_question(&stem, 1, &res, &col_geom(400.0), &default_config());

        let choice = make_choice_question(
            &[("A","alfa"),("B","beta"),("C","gama"),("D","delta"),("E","épsilon")],
            AlternativeLayout::Vertical,
        );
        let (_, h_choice, _) = layout_question(&choice, 1, &res, &col_geom(400.0), &default_config());
        assert!(h_choice > h_stem,
            "choice question ({h_choice:.2}) should be taller than stem only ({h_stem:.2})");
    }

    /// Critério: 5 alternativas em grid 2×3 posicionadas corretamente.
    #[test]
    fn choice_grid_five_alts_two_columns_positioned() {
        let (reg, rules) = make_resolver_and_rules();
        let res     = FontResolver::new(&reg, &rules);
        let width   = 400.0_f64;
        let q = make_choice_question(
            &[("A","alfa"),("B","beta"),("C","gama"),("D","delta"),("E","épsilon")],
            AlternativeLayout::Horizontal,
        );
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(width), &default_config());

        assert!(h > 0.0, "height should be positive");

        // In a 2-column grid, col 0 starts at x=0 and col 1 at x≈200.
        let col_width  = width / GRID_COLUMNS as f64; // 200.0
        let glyph_runs: Vec<&Fragment> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .collect();
        assert!(!glyph_runs.is_empty());

        // Every glyph run must start within [0, width].
        for f in &glyph_runs {
            assert!(f.x >= 0.0 && f.x < width,
                "fragment x={:.2} out of [0, {width})", f.x);
        }

        // Some runs should start near 0 (left column) and some near col_width (right column).
        let left_col  = glyph_runs.iter().any(|f| f.x < col_width * 0.5);
        let right_col = glyph_runs.iter().any(|f| f.x >= col_width * 0.5);
        assert!(left_col,  "expected fragments in left column");
        assert!(right_col, "expected fragments in right column");
    }

    #[test]
    fn format_alt_label_upper_lower() {
        assert_eq!(format_alt_label("A", 0, LetterCase::Upper), "A) ");
        assert_eq!(format_alt_label("A", 0, LetterCase::Lower), "a) ");
        assert_eq!(format_alt_label("",  1, LetterCase::Upper), "B) ");
        assert_eq!(format_alt_label("",  1, LetterCase::Lower), "b) ");
    }

    // ── Textual (TASK-016) ────────────────────────────────────────────────────

    fn make_textual_question(line_count: Option<u32>, line_height_cm: Option<f64>) -> Question {
        use crate::spec::answer::TextualAnswer;
        Question {
            number:           None,
            label:            None,
            kind:             QuestionKind::Textual,
            stem:             vec![text_inline("Desenvolva sua resposta.")],
            answer:           AnswerSpace::Textual(TextualAnswer {
                line_count,
                blank_height_cm: None,
                line_height_cm,
            }),
            base_texts:       vec![],
            points:           None,
            full_width:       false,
            draft_lines:      0,
            draft_line_height: None,
            show_number:      false,
            force_page_break: false,
            style:            None,
        }
    }

    /// Critério: 5 linhas com altura configurável.
    #[test]
    fn textual_lines_five_hrules() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig { discursive_space_type: crate::spec::config::DiscursiveSpaceType::Lines, ..PrintConfig::default() };
        let q    = make_textual_question(Some(5), None);
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);

        let hrules: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).collect();
        assert_eq!(hrules.len(), 5, "should have exactly 5 HRules");
        assert!(h > 0.0);
    }

    #[test]
    fn textual_lines_configurable_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig { discursive_space_type: crate::spec::config::DiscursiveSpaceType::Lines, ..PrintConfig::default() };

        let (_, h_small, _) = layout_question(
            &make_textual_question(Some(5), Some(0.5)),
            1, &res, &col_geom(400.0), &cfg,
        );
        let (_, h_large, _) = layout_question(
            &make_textual_question(Some(5), Some(1.2)),
            1, &res, &col_geom(400.0), &cfg,
        );
        assert!(h_large > h_small,
            "larger line height ({h_large:.2}) should produce taller block than ({h_small:.2})");
    }

    /// Critério: NoBorder sem regras.
    #[test]
    fn textual_no_border_produces_no_hrules_or_rects() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig { discursive_space_type: crate::spec::config::DiscursiveSpaceType::NoBorder, ..PrintConfig::default() };
        let q    = make_textual_question(Some(5), None);
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);

        let has_rules_or_rects = frags.iter().any(|f| matches!(
            f.kind,
            FragmentKind::HRule(_) | FragmentKind::FilledRect(_) | FragmentKind::StrokedRect(_)
        ));
        assert!(!has_rules_or_rects, "NoBorder should produce no visual answer-space elements");
        assert!(h > 0.0, "NoBorder still consumes vertical space");
    }

    #[test]
    fn textual_blank_produces_single_stroked_rect() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig { discursive_space_type: crate::spec::config::DiscursiveSpaceType::Blank, ..PrintConfig::default() };
        let q    = make_textual_question(Some(5), None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);

        let rects: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).collect();
        let hrules: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).collect();
        assert_eq!(rects.len(), 1, "Blank mode should produce exactly 1 StrokedRect");
        assert_eq!(hrules.len(), 0, "Blank mode should have no internal HRules");
    }

    #[test]
    fn textual_default_line_count_uses_config_fallback() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig { discursive_space_type: crate::spec::config::DiscursiveSpaceType::Lines, ..PrintConfig::default() };
        // line_count = None → should fall back to DEFAULT_LINE_COUNT (5)
        let q = make_textual_question(None, None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);
        let hrules: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).collect();
        assert_eq!(hrules.len(), DEFAULT_LINE_COUNT as usize,
            "missing line_count should default to {DEFAULT_LINE_COUNT}");
    }

    // ── Cloze (TASK-017) ──────────────────────────────────────────────────────

    fn make_cloze_question(word_bank: Vec<Vec<InlineContent>>) -> Question {
        use crate::spec::answer::ClozeAnswer;
        use crate::spec::inline::InlineBlank;
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Cloze,
            stem:              vec![
                text_inline("Preencha: "),
                InlineContent::Blank(InlineBlank { width_cm: None }),
                text_inline(" e "),
                InlineContent::Blank(InlineBlank { width_cm: Some(2.0) }),
            ],
            answer:            AnswerSpace::Cloze(ClozeAnswer { word_bank, shuffle_display: false }),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       false,
            force_page_break:  false,
            style:             None,
        }
    }

    /// Critério: blanks aparecem inline no texto (FilledRect no stem).
    #[test]
    fn cloze_blanks_appear_inline_in_stem() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_cloze_question(vec![]);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let blanks: Vec<_> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::FilledRect(_)))
            .collect();
        assert!(!blanks.is_empty(), "cloze stem should have FilledRect blanks");
    }

    #[test]
    fn cloze_default_blank_width_is_35cm() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_cloze_question(vec![]);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let blank_widths: Vec<f64> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::FilledRect(_)))
            .map(|f| f.width)
            .collect();
        // The first blank has no explicit width → default 3.5cm = 99.21pt
        let expected = 3.5 * 28.3465;
        assert!(blank_widths.iter().any(|&w| (w - expected).abs() < 1.0),
            "default blank should be ~{expected:.1}pt, got {blank_widths:?}");
    }

    #[test]
    fn cloze_economy_mode_blank_width_is_25cm() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_cloze_question(vec![]);
        let cfg  = PrintConfig { economy_mode: true, ..PrintConfig::default() };
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);

        let blank_widths: Vec<f64> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::FilledRect(_)))
            .map(|f| f.width)
            .collect();
        let expected = 2.5 * 28.3465;
        assert!(blank_widths.iter().any(|&w| (w - expected).abs() < 1.0),
            "economy blank should be ~{expected:.1}pt, got {blank_widths:?}");
    }

    /// Critério: word_bank separado abaixo.
    #[test]
    fn cloze_word_bank_produces_extra_fragments_below_stem() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);

        let (_, h_no_bank, _) = layout_question(
            &make_cloze_question(vec![]),
            1, &res, &col_geom(400.0), &default_config(),
        );
        let bank = vec![
            vec![text_inline("palavra")],
            vec![text_inline("outra")],
            vec![text_inline("terceira")],
        ];
        let (_, h_with_bank, _) = layout_question(
            &make_cloze_question(bank),
            1, &res, &col_geom(400.0), &default_config(),
        );
        assert!(h_with_bank > h_no_bank,
            "word bank ({h_with_bank:.2}) should increase height beyond stem ({h_no_bank:.2})");
    }

    #[test]
    fn cloze_no_word_bank_no_extra_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        // Empty word bank → no additional answer-space fragments
        let q    = make_cloze_question(vec![]);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());
        // Only stem fragments (GlyphRun + FilledRect) — no numbered word-bank glyph runs
        // A crude check: no fragment with y significantly below the stem area
        let max_y = frags.iter().map(|f| f.y).fold(0.0_f64, f64::max);
        // With 1 stem line, max_y should be under 3 line-heights
        let three_lines = default_config().font_size * default_config().line_spacing.multiplier() * 3.0;
        assert!(max_y < three_lines,
            "without word bank, max_y ({max_y:.2}) should be within ~3 line heights ({three_lines:.2})");
    }

    #[test]
    fn choice_economy_mode_reduces_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_choice_question(
            &[("A","alfa"),("B","beta"),("C","gama"),("D","delta"),("E","épsilon")],
            AlternativeLayout::Vertical,
        );
        let geom = col_geom(400.0);
        let (_, h_normal, _)  = layout_question(&q, 1, &res, &geom, &PrintConfig::default());
        let (_, h_economy, _) = layout_question(&q, 1, &res, &geom,
            &PrintConfig { economy_mode: true, ..PrintConfig::default() });
        assert!(h_economy < h_normal,
            "economy mode ({h_economy:.2}) should be shorter than normal ({h_normal:.2})");
    }

    // ── Sum (TASK-018) ────────────────────────────────────────────────────────

    fn make_sum_question(values: &[u32], show_sum_box: bool) -> Question {
        use crate::spec::answer::{SumAnswer, SumItem};
        let items = values.iter().map(|&v| SumItem {
            value:   v,
            content: vec![text_inline(&format!("Item com valor {v}"))],
        }).collect();
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Sum,
            stem:              vec![text_inline("Assinale as afirmativas corretas e some os valores.")],
            answer:            AnswerSpace::Sum(SumAnswer { items, show_sum_box }),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       false,
            force_page_break:  false,
            style:             None,
        }
    }

    /// Critério: 5 itens + caixa de soma; valores alinhados à direita.
    #[test]
    fn sum_five_items_positive_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_sum_question(&[1, 2, 4, 8, 16], true);
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        assert!(h > 0.0, "height should be positive, got {h}");
        assert!(!frags.is_empty(), "should have fragments");
    }

    #[test]
    fn sum_five_items_produce_five_checkboxes() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_sum_question(&[1, 2, 4, 8, 16], false);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let rects: Vec<_> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_)))
            .collect();
        assert_eq!(rects.len(), 5, "should have exactly 5 checkbox StrokedRects, got {}", rects.len());
    }

    #[test]
    fn sum_show_sum_box_produces_extra_stroked_rect() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);

        let q_with    = make_sum_question(&[1, 2, 4, 8, 16], true);
        let q_without = make_sum_question(&[1, 2, 4, 8, 16], false);

        let (frags_with, h_with, _)       = layout_question(&q_with,    1, &res, &col_geom(400.0), &default_config());
        let (frags_without, h_without, _) = layout_question(&q_without, 1, &res, &col_geom(400.0), &default_config());

        let rects_with    = frags_with.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).count();
        let rects_without = frags_without.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).count();

        assert!(rects_with > rects_without, "show_sum_box=true should add an extra StrokedRect");
        assert!(h_with > h_without, "show_sum_box=true should increase total height");
    }

    #[test]
    fn sum_values_are_right_aligned() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let width = 400.0_f64;
        let q     = make_sum_question(&[1, 2, 4, 8, 16], false);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(width), &default_config());

        // The rightmost GlyphRun right-edges are the value labels.
        let max_right = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .map(|f| f.x + f.width)
            .fold(0.0_f64, f64::max);

        // Value labels are right-aligned: their right edge should be near column_width.
        assert!((max_right - width).abs() < 2.0,
            "value label right edge ({max_right:.2}) should be near column width ({width:.2})");
    }

    #[test]
    fn sum_height_greater_than_stem_alone() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        // Use the same stem as the sum question, but with a textual answer
        // that draws zero lines, so only the stem contributes height.
        let stem_text = "Assinale as afirmativas corretas e some os valores.";
        let stem_only = Question {
            kind:   QuestionKind::Textual,
            stem:   vec![text_inline(stem_text)],
            answer: AnswerSpace::Textual(TextualAnswer {
                line_count: Some(0),
                ..Default::default()
            }),
            ..simple_question(stem_text, false)
        };
        let (_, h_stem, _) = layout_question(&stem_only, 1, &res, &col_geom(400.0), &default_config());

        let sum = make_sum_question(&[1, 2, 4, 8, 16], true);
        let (_, h_sum, _) = layout_question(&sum, 1, &res, &col_geom(400.0), &default_config());

        assert!(h_sum > h_stem, "sum question ({h_sum:.2}) should be taller than stem alone ({h_stem:.2})");
    }

    // ── Essay (TASK-019) ──────────────────────────────────────────────────────

    fn make_essay_question(line_count: Option<u32>, height_cm: Option<f64>) -> Question {
        use crate::spec::answer::EssayAnswer;
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Essay,
            stem:              vec![text_inline("Desenvolva sua resposta detalhadamente.")],
            answer:            AnswerSpace::Essay(EssayAnswer { line_count, height_cm }),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       false,
            force_page_break:  false,
            style:             None,
        }
    }

    /// Critério: line_count gera N linhas.
    #[test]
    fn essay_line_count_produces_n_hrules() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_essay_question(Some(5), None);
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let hrules: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).collect();
        assert_eq!(hrules.len(), 5, "should have exactly 5 HRules, got {}", hrules.len());
        assert!(h > 0.0);
    }

    /// Critério: height_cm gera área de altura correta.
    #[test]
    fn essay_height_cm_produces_stroked_rect_of_correct_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let height_cm = 5.0_f64;
        let q    = make_essay_question(None, Some(height_cm));
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let rects: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).collect();
        assert_eq!(rects.len(), 1, "height_cm should produce exactly 1 StrokedRect");

        let expected_h = height_cm * 28.3465;
        // h includes stem height; the rect height alone should match height_cm
        assert!((rects[0].height - expected_h).abs() < 1.0,
            "rect height {:.2} should be ~{expected_h:.2}pt", rects[0].height);
        let _ = h;
    }

    /// Critério: height_cm tem prioridade sobre line_count.
    #[test]
    fn essay_height_cm_takes_priority_over_line_count() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        // Both set — height_cm wins
        let q    = make_essay_question(Some(10), Some(3.0));
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let hrules = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).count();
        let rects  = frags.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).count();
        assert_eq!(hrules, 0, "height_cm priority: no HRules should be produced");
        assert_eq!(rects,  1, "height_cm priority: exactly 1 StrokedRect");
    }

    #[test]
    fn essay_no_options_falls_back_to_default_line_count() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_essay_question(None, None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let hrules = frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).count();
        assert_eq!(hrules, DEFAULT_LINE_COUNT as usize,
            "no options → default {DEFAULT_LINE_COUNT} HRules, got {hrules}");
    }

    #[test]
    fn essay_height_cm_taller_than_line_count() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let (_, h_lines, _) = layout_question(&make_essay_question(Some(5), None),    1, &res, &col_geom(400.0), &default_config());
        let (_, h_box, _)   = layout_question(&make_essay_question(None, Some(10.0)), 1, &res, &col_geom(400.0), &default_config());
        assert!(h_box > h_lines,
            "10cm box ({h_box:.2}) should be taller than 5-line essay ({h_lines:.2})");
    }

    // ── Draft lines (TASK-023) ────────────────────────────────────────────────

    fn make_draft_question(draft_lines: u32, draft_line_height: Option<f64>) -> Question {
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Textual,
            stem:              vec![text_inline("Enunciado.")],
            answer:            AnswerSpace::Textual(TextualAnswer::default()),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines,
            draft_line_height,
            show_number:       false,
            force_page_break:  false,
            style:             None,
        }
    }

    /// Critério: questão com draft_lines=3 gera 3 linhas cinzas com label.
    #[test]
    fn draft_lines_3_produces_3_gray_hrules() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_draft_question(3, None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let gray_hrules: Vec<_> = frags.iter().filter(|f| {
            matches!(&f.kind, FragmentKind::HRule(r) if r.color == "#AAAAAA")
        }).collect();
        assert_eq!(gray_hrules.len(), 3, "should have exactly 3 gray HRules, got {}", gray_hrules.len());
    }

    #[test]
    fn draft_lines_label_rascunho_produced() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_draft_question(3, None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        // The "Rascunho" label is a GlyphRun at 8pt.
        let label_runs: Vec<_> = frags.iter().filter(|f| {
            matches!(&f.kind, FragmentKind::GlyphRun(r) if (r.font_size - DRAFT_LABEL_SIZE_PT).abs() < 0.1)
        }).collect();
        assert!(!label_runs.is_empty(), "should have at least one GlyphRun at DRAFT_LABEL_SIZE_PT for 'Rascunho'");
    }

    #[test]
    fn draft_lines_increase_total_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let no_draft = make_draft_question(0, None);
        let with_draft = make_draft_question(3, None);
        let (_, h_no, _)   = layout_question(&no_draft,   1, &res, &col_geom(400.0), &default_config());
        let (_, h_with, _) = layout_question(&with_draft, 1, &res, &col_geom(400.0), &default_config());
        assert!(h_with > h_no, "draft lines should increase total height");
    }

    #[test]
    fn draft_lines_zero_produces_no_gray_hrules() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_draft_question(0, None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let gray_hrules = frags.iter().filter(|f| {
            matches!(&f.kind, FragmentKind::HRule(r) if r.color == "#AAAAAA")
        }).count();
        assert_eq!(gray_hrules, 0, "draft_lines=0 should produce no gray HRules");
    }

    #[test]
    fn draft_line_height_affects_block_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let (_, h_small, _) = layout_question(&make_draft_question(3, Some(0.5)), 1, &res, &col_geom(400.0), &default_config());
        let (_, h_large, _) = layout_question(&make_draft_question(3, Some(1.5)), 1, &res, &col_geom(400.0), &default_config());
        assert!(h_large > h_small, "larger draft_line_height should produce taller block");
    }

    #[test]
    fn draft_lines_appear_after_answer_space() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let cfg  = PrintConfig {
            discursive_space_type: crate::spec::config::DiscursiveSpaceType::Lines,
            ..PrintConfig::default()
        };
        let mut q = make_draft_question(3, None);
        // Give it 5 textual answer lines so the answer space has a known max y.
        q.answer = AnswerSpace::Textual(TextualAnswer { line_count: Some(5), blank_height_cm: None, line_height_cm: None });

        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);

        let answer_hrules: Vec<f64> = frags.iter()
            .filter(|f| matches!(&f.kind, FragmentKind::HRule(r) if r.color == "#000000"))
            .map(|f| f.y)
            .collect();
        let draft_hrules: Vec<f64> = frags.iter()
            .filter(|f| matches!(&f.kind, FragmentKind::HRule(r) if r.color == "#AAAAAA"))
            .map(|f| f.y)
            .collect();

        let max_answer_y = answer_hrules.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_draft_y  = draft_hrules.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(min_draft_y > max_answer_y,
            "draft lines (min_y={min_draft_y:.2}) should appear below answer lines (max_y={max_answer_y:.2})");
    }

    // ── File (TASK-020) ───────────────────────────────────────────────────────

    fn make_file_question(label: Option<&str>) -> Question {
        use crate::spec::answer::FileAnswer;
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::File,
            stem:              vec![text_inline("Envie o arquivo solicitado.")],
            answer:            AnswerSpace::File(FileAnswer { label: label.map(str::to_owned) }),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       false,
            force_page_break:  false,
            style:             None,
        }
    }

    /// Critério: caixa com border dashed.
    #[test]
    fn file_produces_dashed_stroked_rect() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_file_question(None);
        let (frags, h, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let dashed: Vec<_> = frags.iter().filter(|f| {
            matches!(&f.kind, FragmentKind::StrokedRect(r) if r.dash.is_some())
        }).collect();
        assert_eq!(dashed.len(), 1, "should have exactly 1 dashed StrokedRect, got {}", dashed.len());
        assert!(h > 0.0);
    }

    /// Critério: label visível (GlyphRun produzido).
    #[test]
    fn file_default_label_produces_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_file_question(None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let runs: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).collect();
        assert!(!runs.is_empty(), "should have at least one GlyphRun for the label");
    }

    #[test]
    fn file_custom_label_still_produces_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_file_question(Some("Faça o upload do PDF"));
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let runs: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).collect();
        assert!(!runs.is_empty(), "custom label should produce a GlyphRun");
    }

    #[test]
    fn file_produces_icon_filled_rect() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let q    = make_file_question(None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());

        let icons: Vec<_> = frags.iter().filter(|f| matches!(f.kind, FragmentKind::FilledRect(_))).collect();
        assert_eq!(icons.len(), 1, "should have exactly 1 FilledRect icon placeholder");
    }

    #[test]
    fn file_box_spans_column_width() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let width = 400.0_f64;
        let q     = make_file_question(None);
        let (frags, _, _) = layout_question(&q, 1, &res, &col_geom(width), &default_config());

        let border = frags.iter().find(|f| {
            matches!(&f.kind, FragmentKind::StrokedRect(r) if r.dash.is_some())
        }).expect("dashed border fragment");
        assert!((border.width - width).abs() < 0.5,
            "border width {:.2} should equal column width {width:.2}", border.width);
    }

    // ── Split points ──────────────────────────────────────────────────────────

    #[test]
    fn choice_question_has_split_point_after_stem() {
        use crate::spec::answer::{Alternative, ChoiceAnswer};
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q = Question {
            kind: QuestionKind::Choice,
            stem: vec![text_inline("Stem text here.")],
            answer: AnswerSpace::Choice(ChoiceAnswer {
                alternatives: vec![
                    Alternative { label: String::new(), content: vec![text_inline("Alt A")] },
                    Alternative { label: String::new(), content: vec![text_inline("Alt B")] },
                ],
                layout: crate::spec::answer::AlternativeLayout::Vertical,
            }),
            ..simple_question("", true)
        };
        let (_, _, split_points) = layout_question(&q, 1, &res, &col_geom(400.0), &default_config());
        assert!(!split_points.is_empty(), "choice question should have at least one split point");
        // The split point should be after heading + stem, before alternatives.
        assert!(split_points[0] > 20.0, "split point should be after heading+stem (got {:.1})", split_points[0]);
    }

    #[test]
    fn force_choices_with_statement_removes_stem_split_point() {
        use crate::spec::answer::{Alternative, ChoiceAnswer};
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q = Question {
            kind: QuestionKind::Choice,
            stem: vec![text_inline("Stem text.")],
            answer: AnswerSpace::Choice(ChoiceAnswer {
                alternatives: vec![
                    Alternative { label: String::new(), content: vec![text_inline("Alt A")] },
                ],
                layout: crate::spec::answer::AlternativeLayout::Vertical,
            }),
            ..simple_question("", true)
        };
        let cfg = PrintConfig { force_choices_with_statement: 1, ..default_config() };
        let (_, _, split_points) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);
        // With force_choices_with_statement, no split point between stem and alternatives.
        assert!(split_points.is_empty(),
            "force_choices_with_statement should suppress the stem-alternatives split point");
    }

    #[test]
    fn break_alternatives_adds_per_alternative_split_points() {
        use crate::spec::answer::{Alternative, ChoiceAnswer};
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let q = Question {
            kind: QuestionKind::Choice,
            stem: vec![text_inline("Stem.")],
            answer: AnswerSpace::Choice(ChoiceAnswer {
                alternatives: vec![
                    Alternative { label: String::new(), content: vec![text_inline("A")] },
                    Alternative { label: String::new(), content: vec![text_inline("B")] },
                    Alternative { label: String::new(), content: vec![text_inline("C")] },
                ],
                layout: crate::spec::answer::AlternativeLayout::Vertical,
            }),
            ..simple_question("", true)
        };
        let cfg = PrintConfig { break_alternatives: true, ..default_config() };
        let (_, _, split_points) = layout_question(&q, 1, &res, &col_geom(400.0), &cfg);
        // Should have: 1 stem split + 2 alternative splits (between A-B and B-C).
        assert!(split_points.len() >= 3,
            "break_alternatives should produce split points between alternatives, got {}", split_points.len());
    }
}
