//! Section header layout.
//!
//! Renders the visual header of a [`Section`] in this order:
//!
//! 1. `PageBreak` — when `section.force_page_break` is true.
//! 2. **Category badge** — `FilledRect` background + `GlyphRun` label (if present).
//! 3. **Title** — heading role, bold, `font_size × 1.2` + separator `HRule`.
//! 4. **Instructions** — body role, italic (via `InlineLayoutEngine`).
//! 5. **`SectionTop` base texts** — collected from `section.questions` and
//!    rendered using [`render_base_text`].
//!
//! The caller iterates the returned [`RenderedSectionItem`]s and pushes each
//! `Block` into the [`PageComposer`], calling `new_page()` for `PageBreak`.

use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::base_text::render_base_text;
use crate::layout::fragment::{FilledRect, Fragment, FragmentKind, GlyphRun};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::question::ColumnGeometry;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::config::PrintConfig;
use crate::spec::exam::Section;
use crate::spec::inline::{InlineContent, InlineText};
use crate::spec::question::BaseTextPosition;
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants
// ─────────────────────────────────────────────────────────────────────────────

/// Scale factor for the section title (heading role).
/// Slightly smaller than question content — subtle label, not a large heading.
const TITLE_SCALE: f64 = 0.85;
/// Vertical gap below the title text (includes HRule separator).
const TITLE_GAP_PT: f64 = 4.0;
/// Top margin before a section header — provides visual separation from the
/// previous section's last question.  Matches lize CSS `.mt-3` (~12pt).
const SECTION_TOP_MARGIN_PT: f64 = 12.0;
/// Color used for the section title text — subtle gray.
const TITLE_COLOR: &str = "#999999";
/// Vertical gap below the instructions block.
const INSTRUCTIONS_GAP_PT: f64 = 4.0;
/// Horizontal padding inside the category badge.
const BADGE_H_PAD_PT: f64 = 6.0;
/// Vertical padding inside the category badge.
const BADGE_V_PAD_PT: f64 = 3.0;
/// Background color of the category badge.
const BADGE_COLOR: &str = "#E8E8E8";
/// Vertical gap below the category badge.
const BADGE_GAP_PT: f64 = 4.0;
/// Vertical gap between SectionTop base-text entries.
const SECTION_TOP_GAP_PT: f64 = 4.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// A rendered piece of a section header, ready for the PageComposer.
pub enum RenderedSectionItem {
    /// Visual content block: push via `PageComposer::push_block`.
    Block { fragments: Vec<Fragment>, height: f64 },
    /// Signals the caller to call `PageComposer::new_page()`.
    PageBreak,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the header of a [`Section`] into [`RenderedSectionItem`]s.
///
/// Returns items in render order:
/// - `PageBreak` first (if `section.force_page_break`).
/// - All visual content in a **single `Block`**: category → title → instructions
///   → `SectionTop` base texts.
pub fn layout_section_header<'a>(
    section:  &Section,
    resolver: &'a FontResolver<'a>,
    geometry: &ColumnGeometry,
    config:   &PrintConfig,
) -> Vec<RenderedSectionItem> {
    let mut out = Vec::new();

    if section.force_page_break {
        out.push(RenderedSectionItem::PageBreak);
    }

    let font_size     = config.font_size;
    let line_spacing  = config.line_spacing.multiplier();
    let blank_default = if config.economy_mode { 2.5 } else { 3.5 };
    let spc           = if config.economy_mode { 0.7 } else { 1.0 };

    let mut frags   = Vec::new();
    let mut local_y = SECTION_TOP_MARGIN_PT * spc;

    // ── Category badge ───────────────────────────────────────────────────────
    if let Some(ref cat) = section.category {
        let (f, h) = render_category_badge(cat, resolver, font_size, local_y, spc);
        frags.extend(f);
        local_y += h + BADGE_GAP_PT * spc;
    }

    // ── Title + separator rule ───────────────────────────────────────────────
    if let Some(ref title) = section.title {
        let (f, h) = render_title(title, resolver, geometry, font_size, line_spacing, local_y, spc);
        frags.extend(f);
        local_y += h;
    }

    // ── Instructions ─────────────────────────────────────────────────────────
    if !section.instructions.is_empty() {
        let italic_style = ResolvedStyle {
            font_size,
            font_style: FontStyle::Italic,
            line_spacing,
            ..ResolvedStyle::default()
        };
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size,
            line_spacing,
            blank_default_cm: blank_default,
            justify: false,
        };
        let (f, h) = engine.layout(&section.instructions, FontRole::Body, &italic_style, 0.0, local_y);
        frags.extend(f);
        local_y += h + INSTRUCTIONS_GAP_PT * spc;
    }

    // ── SectionTop base texts (from questions) ────────────────────────────────
    for q in &section.questions {
        for bt in q.base_texts.iter().filter(|b| b.position == BaseTextPosition::SectionTop) {
            let (f, h) = render_base_text(
                bt, resolver, geometry.column_width_pt,
                font_size, line_spacing, blank_default,
                0.0, local_y,
            );
            frags.extend(f);
            local_y += h + SECTION_TOP_GAP_PT * spc;
        }
    }

    if !frags.is_empty() {
        out.push(RenderedSectionItem::Block { fragments: frags, height: local_y });
    }

    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Render the category badge: a filled rect with the category label over it.
fn render_category_badge<'a>(
    category:  &str,
    resolver:  &'a FontResolver<'a>,
    font_size: f64,
    origin_y:  f64,
    _spc:      f64,
) -> (Vec<Fragment>, f64) {
    let fd     = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
    let glyphs = shape_text(fd, category);
    let text_w = shaped_text_width(&glyphs, font_size, fd.units_per_em);
    let ascent = fd.ascender as f64 / fd.units_per_em as f64 * font_size;
    let family = Rc::from(resolver.resolve_family_name(FontRole::Body, None));

    let badge_w = text_w + 2.0 * BADGE_H_PAD_PT;
    let badge_h = font_size + 2.0 * BADGE_V_PAD_PT;

    let bg = Fragment {
        x:      0.0,
        y:      origin_y,
        width:  badge_w,
        height: badge_h,
        kind:   FragmentKind::FilledRect(FilledRect { color: BADGE_COLOR.to_owned() }),
    };

    let label = Fragment {
        x:      BADGE_H_PAD_PT,
        y:      origin_y + BADGE_V_PAD_PT,
        width:  text_w,
        height: font_size,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, font_size, family, 0, Rc::from("#000000"), ascent,
        )),
    };

    (vec![bg, label], badge_h)
}

/// Render the section title (colored, semi-bold heading with thin separator).
///
/// Matches lize HTML: colored subject name + thin `<hr>` below.
fn render_title<'a>(
    title:        &str,
    resolver:     &'a FontResolver<'a>,
    _geometry:    &ColumnGeometry,
    font_size:    f64,
    _line_spacing: f64,
    origin_y:     f64,
    spc:          f64,
) -> (Vec<Fragment>, f64) {
    let title_size = font_size * TITLE_SCALE;
    let fd     = resolver.resolve(FontRole::Heading, FontWeight::Normal, FontStyle::Normal, None);
    let glyphs = shape_text(fd, title);
    let text_w = shaped_text_width(&glyphs, title_size, fd.units_per_em);
    let ascent = fd.ascender as f64 / fd.units_per_em as f64 * title_size;
    let family = Rc::from(resolver.resolve_family_name(FontRole::Heading, None));

    let mut frags = Vec::new();

    // Title text — colored, regular weight (not bold), matching lize's subtle style.
    frags.push(Fragment {
        x:      0.0,
        y:      origin_y,
        width:  text_w,
        height: title_size,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, title_size, family, 0, Rc::from(TITLE_COLOR), ascent,
        )),
    });

    let local_h = title_size * 1.2 + TITLE_GAP_PT * spc;

    (frags, local_h)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::answer::{AnswerSpace, TextualAnswer};
    use crate::spec::exam::Section;
    use crate::spec::inline::{InlineContent, InlineText};
    use crate::spec::question::{BaseText, Question, QuestionKind};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn col_geom(w: f64) -> ColumnGeometry { ColumnGeometry { column_width_pt: w } }
    fn default_config() -> PrintConfig { PrintConfig::default() }

    fn inline_text(s: &str) -> InlineContent {
        InlineContent::Text(InlineText { value: s.to_owned(), style: None })
    }

    fn empty_question() -> Question {
        Question {
            number: None, label: None,
            kind: QuestionKind::Textual,
            stem: vec![],
            answer: AnswerSpace::Textual(TextualAnswer::default()),
            base_texts: vec![],
            points: None,
            full_width: false,
            draft_lines: 0, draft_line_height: None,
            show_number: false, force_page_break: false, style: None,
        }
    }

    fn empty_section() -> Section {
        Section {
            title: None, instructions: vec![], questions: vec![],
            category: None, style: None, force_page_break: false,
        }
    }

    // ── force_page_break ──────────────────────────────────────────────────────

    #[test]
    fn force_page_break_emits_page_break_first() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.force_page_break = true;
        sec.title = Some("Seção A".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        assert!(items.len() >= 2, "should have PageBreak + Block");
        assert!(matches!(items[0], RenderedSectionItem::PageBreak), "first item must be PageBreak");
    }

    #[test]
    fn no_force_page_break_no_page_break_item() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title = Some("Seção B".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        let has_pb = items.iter().any(|i| matches!(i, RenderedSectionItem::PageBreak));
        assert!(!has_pb, "force_page_break=false must not emit PageBreak");
    }

    // ── category badge ────────────────────────────────────────────────────────

    /// Critério: seção com título, instruções e categoria renderiza em ordem.
    #[test]
    fn category_badge_produces_filled_rect_and_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.category = Some("Matemática".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        let block = items.iter().find(|i| matches!(i, RenderedSectionItem::Block { .. })).unwrap();
        if let RenderedSectionItem::Block { fragments, .. } = block {
            let filled = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::FilledRect(_))).count();
            let runs   = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).count();
            assert!(filled >= 1, "category badge needs a FilledRect");
            assert!(runs   >= 1, "category badge needs a GlyphRun");
        }
    }

    #[test]
    fn category_appears_before_title_in_y() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.category = Some("Física".to_owned());
        sec.title    = Some("Seção C".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        if let RenderedSectionItem::Block { fragments, .. } = items.last().unwrap() {
            let badge_y = fragments.iter()
                .filter(|f| matches!(f.kind, FragmentKind::FilledRect(_)))
                .map(|f| f.y).fold(f64::INFINITY, f64::min);
            let title_y = fragments.iter()
                .filter(|f| matches!(&f.kind, FragmentKind::GlyphRun(r) if &*r.color == TITLE_COLOR))
                .map(|f| f.y).fold(f64::INFINITY, f64::min);
            assert!(badge_y < title_y, "badge (y={badge_y:.2}) must appear above title (y={title_y:.2})");
        }
    }

    // ── title ─────────────────────────────────────────────────────────────────

    #[test]
    fn title_produces_colored_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title = Some("Seção D".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        if let RenderedSectionItem::Block { fragments, .. } = &items[0] {
            let run = fragments.iter().find(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).unwrap();
            if let FragmentKind::GlyphRun(ref r) = run.kind {
                assert_eq!(r.variant, 0, "title should be regular (variant=0), matching lize's subtle style");
                assert_eq!(&*r.color, TITLE_COLOR, "title should use primary color");
                let expected = default_config().font_size * TITLE_SCALE;
                assert!((r.font_size - expected).abs() < 0.5,
                    "title font_size ({:.1}pt) should match config ({:.1}pt)",
                    r.font_size, expected);
            }
        }
    }

    #[test]
    fn title_no_hrule_separator() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title = Some("Seção E".to_owned());

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        if let RenderedSectionItem::Block { fragments, .. } = &items[0] {
            let hrules = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).count();
            assert_eq!(hrules, 0, "section title should not have separator HRule");
        }
    }

    // ── instructions ─────────────────────────────────────────────────────────

    #[test]
    fn instructions_produce_glyph_runs() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title        = Some("Seção G".to_owned());
        sec.instructions = vec![inline_text("Responda todas as questões.")];

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        if let RenderedSectionItem::Block { fragments, height } = &items[0] {
            assert!(*height > 0.0);
            let runs = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).count();
            assert!(runs >= 2, "should have ≥2 GlyphRuns (title + instructions)");
        }
    }

    #[test]
    fn instructions_appear_below_title() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title        = Some("Seção H".to_owned());
        sec.instructions = vec![inline_text("Instrução de exemplo.")];

        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        if let RenderedSectionItem::Block { fragments, .. } = &items[0] {
            // Title uses TITLE_COLOR, instructions use default black
            let title_y = fragments.iter()
                .filter(|f| matches!(&f.kind, FragmentKind::GlyphRun(r) if &*r.color == TITLE_COLOR))
                .map(|f| f.y).fold(f64::INFINITY, f64::min);
            let instr_y = fragments.iter()
                .filter(|f| matches!(&f.kind, FragmentKind::GlyphRun(r) if &*r.color != TITLE_COLOR))
                .map(|f| f.y).fold(f64::INFINITY, f64::min);
            assert!(instr_y > title_y,
                "instructions (y={instr_y:.2}) should appear below title (y={title_y:.2})");
        }
    }

    #[test]
    fn no_instructions_does_not_crash() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let mut sec = empty_section();
        sec.title = Some("Seção I".to_owned());
        // instructions is empty by default
        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        assert!(!items.is_empty());
    }

    // ── SectionTop base texts ─────────────────────────────────────────────────

    #[test]
    fn section_top_base_texts_increase_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let mut sec_no_bt = empty_section();
        sec_no_bt.title = Some("Seção J".to_owned());

        let mut sec_with_bt = sec_no_bt.clone();
        let mut q = empty_question();
        q.base_texts = vec![BaseText {
            content:     vec![inline_text("Leia o texto base.")],
            position:    BaseTextPosition::SectionTop,
            title:       None,
            attribution: None,
            style:       None,
        }];
        sec_with_bt.questions = vec![q];

        let h_no = block_height(layout_section_header(&sec_no_bt,   &res, &col_geom(400.0), &default_config()));
        let h_bt = block_height(layout_section_header(&sec_with_bt, &res, &col_geom(400.0), &default_config()));
        assert!(h_bt > h_no, "SectionTop base text should increase header height");
    }

    #[test]
    fn non_section_top_base_texts_not_included() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let mut sec = empty_section();
        sec.title = Some("Seção K".to_owned());
        let mut q = empty_question();
        // BeforeQuestion should NOT appear in section header
        q.base_texts = vec![BaseText {
            content:     vec![inline_text("Texto antes da questão.")],
            position:    BaseTextPosition::BeforeQuestion,
            title:       None,
            attribution: None,
            style:       None,
        }];
        sec.questions = vec![q.clone()];

        let h_no_bt     = block_height(layout_section_header(&{ let mut s = sec.clone(); s.questions = vec![]; s }, &res, &col_geom(400.0), &default_config()));
        let h_before_q  = block_height(layout_section_header(&sec, &res, &col_geom(400.0), &default_config()));
        assert_eq!(h_no_bt, h_before_q, "BeforeQuestion base text must not affect section header height");
    }

    // ── Empty section ─────────────────────────────────────────────────────────

    #[test]
    fn empty_section_produces_no_items() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let sec = empty_section();
        let items = layout_section_header(&sec, &res, &col_geom(400.0), &default_config());
        assert!(items.is_empty(), "a section with no title/instructions/category/base-texts produces no items");
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    fn block_height(items: Vec<RenderedSectionItem>) -> f64 {
        items.iter().map(|i| {
            if let RenderedSectionItem::Block { height, .. } = i { *height } else { 0.0 }
        }).sum()
    }
}
