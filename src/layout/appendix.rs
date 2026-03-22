//! Appendix layout — renders `Appendix` content at the end of the document.
//!
//! # Output model
//!
//! [`layout_appendix`] returns a `Vec<RenderedAppendixItem>`.  The caller
//! (PageComposer loop) iterates the items:
//! - `Block`     → push into [`PageComposer::push_block`] / [`push_block_full_width`].
//! - `PageBreak` → call [`PageComposer::new_page`].
//!
//! # Feature gate
//!
//! `AppendixItem::FormulaSheet` renders each formula with `InlineContent::Math`
//! (display mode).  When the `math` feature is disabled, the
//! [`InlineLayoutEngine`] silently skips `Math` nodes, so formula items still
//! produce the label and title fragments but no typeset mathematics.

use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun, HRule};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::question::ColumnGeometry;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::config::PrintConfig;
use crate::spec::exam::{Appendix, AppendixItem};
use crate::spec::inline::{InlineContent, InlineMath, InlineText};
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants
// ─────────────────────────────────────────────────────────────────────────────

/// Scale factor for the appendix main title (heading role).
const APPENDIX_TITLE_SCALE: f64 = 1.2;
/// Vertical gap below the main title separator rule.
const TITLE_RULE_GAP_PT: f64 = 4.0;
/// Stroke width of the separator rule below the appendix title.
const TITLE_RULE_STROKE_PT: f64 = 0.7;
/// Scale factor for block / formula-sheet sub-titles.
const SUBTITLE_SCALE: f64 = 1.05;
/// Vertical margin between items inside the appendix.
const ITEM_V_GAP_PT: f64 = 6.0;
/// Vertical gap between a formula label and its math content.
const FORMULA_LABEL_GAP_PT: f64 = 2.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// A single rendered piece of the appendix, ready for the PageComposer.
pub enum RenderedAppendixItem {
    /// A block of positioned fragments with its total height.
    Block { fragments: Vec<Fragment>, height: f64 },
    /// Signals the caller to call `PageComposer::new_page()`.
    PageBreak,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out an [`Appendix`] into a sequence of [`RenderedAppendixItem`]s.
///
/// The first item (when `appendix.title` is present) is always the heading
/// block.  Subsequent items correspond to each `AppendixItem` in order.
pub fn layout_appendix<'a>(
    appendix: &Appendix,
    resolver: &'a FontResolver<'a>,
    geometry: &ColumnGeometry,
    config:   &PrintConfig,
) -> Vec<RenderedAppendixItem> {
    let mut out = Vec::new();

    let font_size    = config.font_size;
    let line_spacing = config.line_spacing.multiplier();
    let blank_default = if config.economy_mode { 2.5 } else { 3.5 };
    let spc          = if config.economy_mode { 0.7 } else { 1.0 };

    // ── Appendix main title ──────────────────────────────────────────────────
    if let Some(ref title) = appendix.title {
        let (frags, h) = render_appendix_title(title, resolver, geometry, font_size, line_spacing, spc);
        out.push(RenderedAppendixItem::Block { fragments: frags, height: h });
    }

    // ── Items ────────────────────────────────────────────────────────────────
    for item in &appendix.content {
        match item {
            AppendixItem::Block(block) => {
                let (frags, h) = render_block_item(block.title.as_deref(), &block.content,
                    resolver, geometry, font_size, line_spacing, blank_default, spc);
                out.push(RenderedAppendixItem::Block { fragments: frags, height: h });
            }

            AppendixItem::FormulaSheet(sheet) => {
                let (frags, h) = render_formula_sheet(sheet.title.as_deref(), &sheet.formulas,
                    resolver, geometry, font_size, line_spacing, blank_default, spc);
                out.push(RenderedAppendixItem::Block { fragments: frags, height: h });
            }

            AppendixItem::PageBreak => {
                out.push(RenderedAppendixItem::PageBreak);
            }
        }
    }

    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Render the appendix main title as a bold heading + separator HRule.
fn render_appendix_title<'a>(
    title:        &str,
    resolver:     &'a FontResolver<'a>,
    geometry:     &ColumnGeometry,
    font_size:    f64,
    line_spacing: f64,
    spc:          f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = 0.0_f64;

    let title_size = font_size * APPENDIX_TITLE_SCALE;
    let fd     = resolver.resolve(FontRole::Heading, FontWeight::Bold, FontStyle::Normal, None);
    let glyphs = shape_text(fd, title);
    let w      = shaped_text_width(&glyphs, title_size, fd.units_per_em);
    let ascent = fd.ascender as f64 / fd.units_per_em as f64 * title_size;
    let family = Rc::from(resolver.resolve_family_name(FontRole::Heading, None));

    frags.push(Fragment {
        x:      0.0,
        y:      local_y,
        width:  w,
        height: title_size,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, title_size, family, 1, Rc::from("#000000"), ascent,
        )),
    });
    local_y += title_size * line_spacing;

    // Separator rule.
    frags.push(Fragment {
        x:      0.0,
        y:      local_y,
        width:  geometry.column_width_pt,
        height: TITLE_RULE_STROKE_PT,
        kind:   FragmentKind::HRule(HRule {
            stroke_width: TITLE_RULE_STROKE_PT,
            color:        "#000000".to_owned(),
        }),
    });
    local_y += TITLE_RULE_STROKE_PT + TITLE_RULE_GAP_PT * spc;

    (frags, local_y)
}

/// Render an `AppendixItem::Block`: optional sub-title + inline content.
fn render_block_item<'a>(
    title:            Option<&str>,
    content:          &[InlineContent],
    resolver:         &'a FontResolver<'a>,
    geometry:         &ColumnGeometry,
    font_size:        f64,
    line_spacing:     f64,
    blank_default_cm: f64,
    spc:              f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = 0.0_f64;

    if let Some(t) = title {
        let (f, h) = render_subtitle(t, resolver, geometry, font_size, line_spacing);
        frags.extend(f);
        local_y += h;
    }

    if !content.is_empty() {
        let style  = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let (f, h) = engine.layout(content, FontRole::Body, &style, 0.0, local_y);
        frags.extend(f);
        local_y += h;
    }

    local_y += ITEM_V_GAP_PT * spc;
    (frags, local_y)
}

/// Render an `AppendixItem::FormulaSheet`: optional title + formula entries.
///
/// Each `FormulaEntry` is rendered as: optional label text + `InlineContent::Math`
/// in display mode.  When the `math` feature is disabled the math node is
/// silently skipped by `InlineLayoutEngine`.
fn render_formula_sheet<'a>(
    title:            Option<&str>,
    formulas:         &[crate::spec::exam::FormulaEntry],
    resolver:         &'a FontResolver<'a>,
    geometry:         &ColumnGeometry,
    font_size:        f64,
    line_spacing:     f64,
    blank_default_cm: f64,
    spc:              f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = 0.0_f64;

    if let Some(t) = title {
        let (f, h) = render_subtitle(t, resolver, geometry, font_size, line_spacing);
        frags.extend(f);
        local_y += h;
    }

    let style  = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };

    for entry in formulas {
        // Optional label.
        if let Some(ref label) = entry.label {
            let label_content = vec![InlineContent::Text(InlineText { value: label.clone(), style: None })];
            let engine = InlineLayoutEngine {
                resolver,
                available_width:  geometry.column_width_pt,
                font_size:        font_size * 0.9,
                line_spacing,
                blank_default_cm,
            justify: false,
            };
            let label_style = ResolvedStyle {
                font_size:  font_size * 0.9,
                font_style: FontStyle::Italic,
                line_spacing,
                ..ResolvedStyle::default()
            };
            let (f, h) = engine.layout(&label_content, FontRole::Body, &label_style, 0.0, local_y);
            frags.extend(f);
            local_y += h + FORMULA_LABEL_GAP_PT * spc;
        }

        // LaTeX formula as display math.
        let math_content = vec![InlineContent::Math(InlineMath { latex: entry.latex.clone(), display: true })];
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let (f, h) = engine.layout(&math_content, FontRole::Body, &style, 0.0, local_y);
        frags.extend(f);
        // When math feature is off, h == 0; advance by at least one line.
        local_y += h.max(font_size * line_spacing);
        local_y += ITEM_V_GAP_PT * spc;
    }

    local_y += ITEM_V_GAP_PT * spc;
    (frags, local_y)
}

/// Render a block / formula-sheet sub-title at a slightly larger size.
fn render_subtitle<'a>(
    title:        &str,
    resolver:     &'a FontResolver<'a>,
    geometry:     &ColumnGeometry,
    font_size:    f64,
    line_spacing: f64,
) -> (Vec<Fragment>, f64) {
    let size   = font_size * SUBTITLE_SCALE;
    let fd     = resolver.resolve(FontRole::Heading, FontWeight::Bold, FontStyle::Normal, None);
    let glyphs = shape_text(fd, title);
    let w      = shaped_text_width(&glyphs, size, fd.units_per_em);
    let ascent = fd.ascender as f64 / fd.units_per_em as f64 * size;
    let family = Rc::from(resolver.resolve_family_name(FontRole::Heading, None));
    let h      = size * line_spacing;

    let frag = Fragment {
        x:      0.0,
        y:      0.0,
        width:  w,
        height: size,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, size, family, 1, Rc::from("#000000"), ascent,
        )),
    };
    let _ = geometry; // width available but subtitle uses natural width
    (vec![frag], h)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::exam::{AppendixBlock, AppendixItem, FormulaEntry, FormulaSheet};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn col_geom(w: f64) -> ColumnGeometry { ColumnGeometry { column_width_pt: w } }
    fn default_config() -> PrintConfig { PrintConfig::default() }

    fn inline_text(s: &str) -> InlineContent {
        InlineContent::Text(InlineText { value: s.to_owned(), style: None })
    }

    /// Critério: appendix com 3 items de tipos diferentes em posições corretas.
    #[test]
    fn three_items_different_types_all_rendered() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix {
            title: Some("Anexos".to_owned()),
            content: vec![
                AppendixItem::Block(AppendixBlock {
                    content: vec![inline_text("Texto do bloco.")],
                    title:   None,
                    style:   None,
                }),
                AppendixItem::PageBreak,
                AppendixItem::FormulaSheet(FormulaSheet {
                    title:    Some("Fórmulas".to_owned()),
                    formulas: vec![FormulaEntry { label: Some("Área".to_owned()), latex: "A = \\pi r^2".to_owned() }],
                }),
            ],
        };

        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());

        // title + Block + PageBreak + FormulaSheet = 4 items
        assert_eq!(items.len(), 4, "should produce 4 rendered items");

        assert!(matches!(items[0], RenderedAppendixItem::Block { .. }), "item 0 should be heading block");
        assert!(matches!(items[1], RenderedAppendixItem::Block { .. }), "item 1 should be Block");
        assert!(matches!(items[2], RenderedAppendixItem::PageBreak),    "item 2 should be PageBreak");
        assert!(matches!(items[3], RenderedAppendixItem::Block { .. }), "item 3 should be FormulaSheet block");
    }

    #[test]
    fn appendix_title_produces_glyph_run_and_hrule() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix {
            title:   Some("Apêndice".to_owned()),
            content: vec![],
        };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());
        assert_eq!(items.len(), 1);

        if let RenderedAppendixItem::Block { ref fragments, height } = items[0] {
            assert!(height > 0.0);
            let runs  = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).count();
            let rules = fragments.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).count();
            assert!(runs  >= 1, "title should produce at least one GlyphRun");
            assert!(rules >= 1, "title should produce a separator HRule");
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn page_break_item_emits_page_break_variant() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix {
            title:   None,
            content: vec![AppendixItem::PageBreak],
        };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], RenderedAppendixItem::PageBreak));
    }

    #[test]
    fn block_item_produces_fragments() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix {
            title:   None,
            content: vec![AppendixItem::Block(AppendixBlock {
                content: vec![inline_text("Conteúdo do anexo.")],
                title:   Some("Bloco 1".to_owned()),
                style:   None,
            })],
        };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());
        assert_eq!(items.len(), 1);

        if let RenderedAppendixItem::Block { ref fragments, height } = items[0] {
            assert!(!fragments.is_empty(), "block should produce fragments");
            assert!(height > 0.0, "block height should be positive");
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn formula_sheet_produces_block_with_positive_height() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix {
            title:   None,
            content: vec![AppendixItem::FormulaSheet(FormulaSheet {
                title:    None,
                formulas: vec![
                    FormulaEntry { label: None,                        latex: "E = mc^2".to_owned() },
                    FormulaEntry { label: Some("Pitágoras".to_owned()), latex: "a^2 + b^2 = c^2".to_owned() },
                ],
            })],
        };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());
        assert_eq!(items.len(), 1);

        if let RenderedAppendixItem::Block { height, .. } = items[0] {
            assert!(height > 0.0, "formula sheet height should be positive");
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn formula_sheet_with_labels_produces_extra_fragments() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let no_label = Appendix {
            title: None,
            content: vec![AppendixItem::FormulaSheet(FormulaSheet {
                title: None,
                formulas: vec![FormulaEntry { label: None, latex: "x^2".to_owned() }],
            })],
        };
        let with_label = Appendix {
            title: None,
            content: vec![AppendixItem::FormulaSheet(FormulaSheet {
                title: None,
                formulas: vec![FormulaEntry { label: Some("Quadrado".to_owned()), latex: "x^2".to_owned() }],
            })],
        };

        let items_no    = layout_appendix(&no_label,    &res, &col_geom(400.0), &default_config());
        let items_label = layout_appendix(&with_label,  &res, &col_geom(400.0), &default_config());

        let frags_no = if let RenderedAppendixItem::Block { ref fragments, .. } = items_no[0] { fragments.len() } else { 0 };
        let frags_lb = if let RenderedAppendixItem::Block { ref fragments, .. } = items_label[0] { fragments.len() } else { 0 };

        assert!(frags_lb > frags_no, "formula with label should produce more fragments than without");
    }

    #[test]
    fn no_title_produces_no_heading_block() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix { title: None, content: vec![] };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());
        assert!(items.is_empty(), "no title + empty content should produce zero items");
    }

    #[test]
    fn title_larger_than_body_font() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        let appendix = Appendix { title: Some("Título".to_owned()), content: vec![] };
        let items = layout_appendix(&appendix, &res, &col_geom(400.0), &default_config());

        if let RenderedAppendixItem::Block { ref fragments, .. } = items[0] {
            let title_run = fragments.iter().find(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).unwrap();
            let body_size = default_config().font_size;
            if let FragmentKind::GlyphRun(ref r) = title_run.kind {
                assert!(r.font_size > body_size,
                    "appendix title ({:.1}pt) should be larger than body ({body_size:.1}pt)", r.font_size);
            }
        }
    }
}
