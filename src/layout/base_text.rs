//! Base-text rendering helpers.
//!
//! Handles all 7 [`BaseTextPosition`] variants:
//!
//! | Position          | Rendered by                        |
//! |-------------------|------------------------------------|
//! | `BeforeQuestion`  | [`layout_question`] (question.rs)  |
//! | `AfterQuestion`   | [`layout_question`] (question.rs)  |
//! | `LeftOfQuestion`  | [`layout_side_by_side`] (here)     |
//! | `RightOfQuestion` | [`layout_side_by_side`] (here)     |
//! | `SectionTop`      | section-level caller + [`render_base_text`] |
//! | `ExamTop`         | exam/page-composer + [`render_base_text`]   |
//! | `ExamBottom`      | exam/page-composer + [`render_base_text`]   |
//!
//! This module provides the low-level rendering primitives used by all callers.

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::Fragment;
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::question::{layout_question, ColumnGeometry};
use crate::spec::config::PrintConfig;
use crate::spec::inline::{InlineContent, InlineText};
use crate::spec::question::{BaseText, BaseTextPosition, Question};
use crate::spec::style::ResolvedStyle;

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants
// ─────────────────────────────────────────────────────────────────────────────

/// Scale factor applied to the attribution line relative to body font size.
const SMALL_FACTOR: f64 = 0.9;
/// Vertical gap between the title and the content body (matches lize h6 block spacing).
const TITLE_CONTENT_GAP_PT: f64 = 6.0;
/// Vertical gap between the content body and the attribution line.
const INNER_V_GAP_PT: f64 = 2.0;
/// Horizontal gap between the two columns in a side-by-side layout.
const SIDE_GAP_PT: f64 = 8.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Render a single [`BaseText`] block into a flat list of [`Fragment`]s.
///
/// The block consists of (in order, each optional):
/// 1. Title — small italic GlyphRun
/// 2. Content — `InlineLayoutEngine` layout at `font_size`
/// 3. Attribution — small italic GlyphRun
///
/// All fragments are placed with x = `origin_x + [0 .. width_pt]` and
/// y = `origin_y + [0 .. height]`.
///
/// Returns `(fragments, block_height_pt)`.
pub fn render_base_text<'a>(
    bt:               &BaseText,
    resolver:         &'a FontResolver<'a>,
    width_pt:         f64,
    font_size:        f64,
    line_spacing:     f64,
    blank_default_cm: f64,
    origin_x:         f64,
    origin_y:         f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = origin_y;

    let small_size   = font_size * SMALL_FACTOR;
    let small_style  = ResolvedStyle {
        font_size:  small_size,
        font_style: crate::spec::style::FontStyle::Italic,
        line_spacing,
        ..ResolvedStyle::default()
    };
    let body_style   = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };
    // Title style: bold, same font size, uppercase — matches lize h6.text-uppercase.font-weight-bold.
    let title_style  = ResolvedStyle {
        font_size,
        font_weight: crate::spec::style::FontWeight::Bold,
        line_spacing,
        ..ResolvedStyle::default()
    };

    // ── Optional title ───────────────────────────────────────────────────────
    if let Some(ref title) = bt.title {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let title_upper = title.to_uppercase();
        let content = vec![InlineContent::Text(InlineText { value: title_upper, style: None })];
        let (f, h) = engine.layout(&content, FontRole::Body, &title_style, origin_x, local_y);
        frags.extend(f);
        local_y += h + TITLE_CONTENT_GAP_PT;
    }

    // ── Content ──────────────────────────────────────────────────────────────
    if !bt.content.is_empty() {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: true,
        };
        let (f, h) = engine.layout(&bt.content, FontRole::Body, &body_style, origin_x, local_y);
        frags.extend(f);
        local_y += h + INNER_V_GAP_PT;
    }

    // ── Optional attribution ─────────────────────────────────────────────────
    if let Some(ref attr) = bt.attribution {
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  width_pt,
            font_size:        small_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let content = vec![InlineContent::Text(InlineText { value: attr.clone(), style: None })];
        let (f, h) = engine.layout(&content, FontRole::Body, &small_style, origin_x, local_y);
        frags.extend(f);
        local_y += h + INNER_V_GAP_PT;
    }

    let total_h = local_y - origin_y;
    (frags, total_h)
}

/// Filter a slice of `BaseText` items to only those matching `pos`.
pub fn filter_by_position(base_texts: &[BaseText], pos: BaseTextPosition) -> Vec<&BaseText> {
    base_texts.iter().filter(|b| b.position == pos).collect()
}

/// Lay out a question alongside its `LeftOfQuestion` or `RightOfQuestion` base texts
/// in a side-by-side mini two-column layout.
///
/// The available column width is split 50/50 with [`SIDE_GAP_PT`] between the halves:
/// - `LeftOfQuestion`  → base text on the **left**, question on the **right**
/// - `RightOfQuestion` → question on the **left**, base text on the **right**
///
/// Fragments from both sides are translated so their x coordinates are relative
/// to the start of `geometry` (x = 0 at the left edge of the full column).
/// Y coordinates start at `origin_y`.
///
/// Returns `(fragments, total_height_pt)`.
pub fn layout_side_by_side<'a>(
    question:   &Question,
    number:     u32,
    resolver:   &'a FontResolver<'a>,
    geometry:   &ColumnGeometry,
    config:     &PrintConfig,
    origin_y:   f64,
    position:   BaseTextPosition,
) -> (Vec<Fragment>, f64) {
    let half_w        = (geometry.column_width_pt - SIDE_GAP_PT) / 2.0;
    let font_size     = config.font_size;
    let line_spacing  = config.line_spacing.multiplier();
    let blank_default = if config.economy_mode { 2.5 } else { 3.5 };

    // ── Render all relevant base texts stacked in their half ─────────────────
    let mut bt_frags = Vec::new();
    let mut bt_y     = 0.0_f64; // accumulate height; translate below
    for bt in question.base_texts.iter().filter(|b| b.position == position) {
        let (f, h) = render_base_text(bt, resolver, half_w, font_size, line_spacing, blank_default, 0.0, bt_y);
        bt_frags.extend(f);
        bt_y += h;
    }
    let bt_height = bt_y;

    // ── Render question in its half ──────────────────────────────────────────
    let q_col = ColumnGeometry { column_width_pt: half_w };
    let (q_frags, q_height, _split_points) = layout_question(question, number, resolver, &q_col, config);

    // ── Determine x offsets for each half ───────────────────────────────────
    let (bt_x, q_x) = match position {
        BaseTextPosition::LeftOfQuestion  => (0.0,              half_w + SIDE_GAP_PT),
        BaseTextPosition::RightOfQuestion => (half_w + SIDE_GAP_PT, 0.0),
        _ => (0.0, 0.0),
    };

    // ── Translate and collect ────────────────────────────────────────────────
    let mut all_frags: Vec<Fragment> = bt_frags
        .into_iter()
        .map(|mut f| { f.x += bt_x; f.y += origin_y; f })
        .collect();

    let q_translated: Vec<Fragment> = q_frags
        .into_iter()
        .map(|mut f| { f.x += q_x; f.y += origin_y; f })
        .collect();
    all_frags.extend(q_translated);

    let total_h = bt_height.max(q_height);
    (all_frags, total_h)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::question::Question;
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    const FIXTURE: &str = include_str!("fixtures/base_text_positions.json");

    fn default_config() -> PrintConfig { PrintConfig::default() }

    fn col_geom(width_pt: f64) -> ColumnGeometry {
        ColumnGeometry { column_width_pt: width_pt }
    }

    /// Load and parse the fixture JSON — returns 7 questions in position order.
    fn load_fixture() -> Vec<Question> {
        serde_json::from_str(FIXTURE).expect("fixture JSON should parse")
    }

    // ── Fixture round-trip ───────────────────────────────────────────────────

    #[test]
    fn fixture_parses_seven_questions() {
        let questions = load_fixture();
        assert_eq!(questions.len(), 7, "fixture must have exactly 7 questions");
    }

    #[test]
    fn fixture_covers_all_seven_positions() {
        let questions = load_fixture();
        let positions: Vec<BaseTextPosition> = questions
            .iter()
            .flat_map(|q| q.base_texts.iter().map(|bt| bt.position))
            .collect();
        for pos in [
            BaseTextPosition::BeforeQuestion,
            BaseTextPosition::AfterQuestion,
            BaseTextPosition::LeftOfQuestion,
            BaseTextPosition::RightOfQuestion,
            BaseTextPosition::SectionTop,
            BaseTextPosition::ExamTop,
            BaseTextPosition::ExamBottom,
        ] {
            assert!(positions.contains(&pos), "fixture missing position {pos:?}");
        }
    }

    // ── render_base_text ─────────────────────────────────────────────────────

    #[test]
    fn render_base_text_produces_fragments() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let questions = load_fixture();
        // q[0] has BeforeQuestion
        let bt = &questions[0].base_texts[0];
        let (frags, h) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5, 0.0, 0.0);
        assert!(!frags.is_empty(), "render_base_text should produce fragments");
        assert!(h > 0.0, "height should be positive");
    }

    #[test]
    fn render_base_text_respects_origin_y() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let questions = load_fixture();
        let bt = &questions[0].base_texts[0];

        let (frags_at_0,  _) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5, 0.0,   0.0);
        let (frags_at_50, _) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5, 0.0, 50.0);

        let min_y_0  = frags_at_0.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
        let min_y_50 = frags_at_50.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
        assert!(min_y_50 > min_y_0, "origin_y=50 should shift all fragments down");
    }

    #[test]
    fn render_base_text_respects_origin_x() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let questions = load_fixture();
        let bt = &questions[0].base_texts[0];

        let (frags_x0,  _) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5,  0.0, 0.0);
        let (frags_x30, _) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5, 30.0, 0.0);

        let min_x_0  = frags_x0.iter().map(|f| f.x).fold(f64::INFINITY, f64::min);
        let min_x_30 = frags_x30.iter().map(|f| f.x).fold(f64::INFINITY, f64::min);
        assert!(min_x_30 > min_x_0, "origin_x=30 should shift all fragments right");
    }

    // ── filter_by_position ───────────────────────────────────────────────────

    #[test]
    fn filter_by_position_returns_correct_subset() {
        let questions = load_fixture();
        // q[0] has only BeforeQuestion
        let bt_before = filter_by_position(&questions[0].base_texts, BaseTextPosition::BeforeQuestion);
        assert_eq!(bt_before.len(), 1);
        let bt_after  = filter_by_position(&questions[0].base_texts, BaseTextPosition::AfterQuestion);
        assert_eq!(bt_after.len(), 0);
    }

    #[test]
    fn filter_by_position_all_seven_from_combined_list() {
        let questions = load_fixture();
        // Flatten all base_texts into one list
        let all_bts: Vec<BaseText> = questions.into_iter().flat_map(|q| q.base_texts).collect();
        for pos in [
            BaseTextPosition::BeforeQuestion,
            BaseTextPosition::AfterQuestion,
            BaseTextPosition::LeftOfQuestion,
            BaseTextPosition::RightOfQuestion,
            BaseTextPosition::SectionTop,
            BaseTextPosition::ExamTop,
            BaseTextPosition::ExamBottom,
        ] {
            let found = filter_by_position(&all_bts, pos);
            assert_eq!(found.len(), 1, "expected exactly 1 BaseText for {pos:?}, got {}", found.len());
        }
    }

    // ── layout_side_by_side ──────────────────────────────────────────────────

    #[test]
    fn side_by_side_left_base_text_on_left_half() {
        let (reg, rules) = make_resolver_and_rules();
        let res     = FontResolver::new(&reg, &rules);
        let width   = 400.0_f64;
        let questions = load_fixture();
        let q = &questions[2]; // LeftOfQuestion

        let (frags, h) = layout_side_by_side(
            q, 1, &res, &col_geom(width), &default_config(), 0.0,
            BaseTextPosition::LeftOfQuestion,
        );
        assert!(h > 0.0, "height should be positive");
        assert!(!frags.is_empty());

        let half = (width - SIDE_GAP_PT) / 2.0;

        // Base-text fragments (from left half) must have x < half + SIDE_GAP_PT
        // Question fragments (from right half) must have x >= half
        // We check that some fragments are on each side.
        let left_frags  = frags.iter().filter(|f| f.x < half).count();
        let right_frags = frags.iter().filter(|f| f.x >= half).count();
        assert!(left_frags  > 0, "should have fragments in the left half");
        assert!(right_frags > 0, "should have fragments in the right half");
    }

    #[test]
    fn side_by_side_right_base_text_on_right_half() {
        let (reg, rules) = make_resolver_and_rules();
        let res     = FontResolver::new(&reg, &rules);
        let width   = 400.0_f64;
        let questions = load_fixture();
        let q = &questions[3]; // RightOfQuestion

        let (frags, h) = layout_side_by_side(
            q, 1, &res, &col_geom(width), &default_config(), 0.0,
            BaseTextPosition::RightOfQuestion,
        );
        assert!(h > 0.0);

        let half = (width - SIDE_GAP_PT) / 2.0;
        let left_frags  = frags.iter().filter(|f| f.x < half).count();
        let right_frags = frags.iter().filter(|f| f.x >= half).count();
        assert!(left_frags  > 0, "question should be in the left half");
        assert!(right_frags > 0, "base text should be in the right half");
    }

    #[test]
    fn side_by_side_origin_y_is_respected() {
        let (reg, rules) = make_resolver_and_rules();
        let res     = FontResolver::new(&reg, &rules);
        let questions = load_fixture();
        let q = &questions[2]; // LeftOfQuestion

        let (frags_0,  _) = layout_side_by_side(q, 1, &res, &col_geom(400.0), &default_config(),  0.0, BaseTextPosition::LeftOfQuestion);
        let (frags_80, _) = layout_side_by_side(q, 1, &res, &col_geom(400.0), &default_config(), 80.0, BaseTextPosition::LeftOfQuestion);

        let min_y_0  = frags_0.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
        let min_y_80 = frags_80.iter().map(|f| f.y).fold(f64::INFINITY, f64::min);
        assert!(min_y_80 > min_y_0, "origin_y=80 should produce higher y values");
    }

    #[test]
    fn side_by_side_no_fragment_exceeds_column_width() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let width = 400.0_f64;
        let questions = load_fixture();
        let q = &questions[2]; // LeftOfQuestion

        let (frags, _) = layout_side_by_side(
            q, 1, &res, &col_geom(width), &default_config(), 0.0,
            BaseTextPosition::LeftOfQuestion,
        );
        for f in &frags {
            assert!(f.x + f.width <= width + 0.5,
                "fragment x={:.2} w={:.2} exceeds column_width={width:.2}", f.x, f.width);
        }
    }

    // ── Section/Exam-level positions render correctly via render_base_text ────

    #[test]
    fn section_top_exam_top_exam_bottom_render_via_render_base_text() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let questions = load_fixture();

        for (i, pos) in [
            (4, BaseTextPosition::SectionTop),
            (5, BaseTextPosition::ExamTop),
            (6, BaseTextPosition::ExamBottom),
        ] {
            let bt = &questions[i].base_texts[0];
            assert_eq!(bt.position, pos, "fixture q[{i}] should have position {pos:?}");
            let (frags, h) = render_base_text(bt, &res, 400.0, 12.0, 1.2, 3.5, 0.0, 0.0);
            assert!(!frags.is_empty(), "{pos:?}: should produce fragments");
            assert!(h > 0.0,          "{pos:?}: height should be positive");
        }
    }
}
